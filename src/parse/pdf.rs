//! PDF document parsing pipeline.
//!
//! Uses a two-pass [`pdf_extract::OutputDev`]-based architecture:
//!
//! 1. **Pass 1 — [`FontSizeHistogram`]**: Scans the first
//!    [`HISTOGRAM_SAMPLE_PAGES`] pages to determine the dominant body font
//!    size without accumulating text.
//! 2. **Pass 2 — [`HeadingAwareOutput`]**: Processes all pages via an
//!    `output_doc_page` loop with the known body font size and emits
//!    [`PdfSection`]s which are converted to [`Chunk`]s.
//!
//! **Fallback**: When all text shares a single quantized font size (uniform
//! typography), the pipeline falls back to [`chunk_pdf_pages`] with
//! `["Page N"]` heading hierarchy — identical to the previous behavior.
//!
//! ## Chunk ID Format
//!
//! - Section-based (heading-aware path): `{source_path}#section={N}#segment={M}`
//! - Page-based (uniform-font fallback): `{source_path}#page={N}#segment={M}`
//!
//! Previously ingested PDFs produced by a version that used the old
//! `LARGE_PDF_THRESHOLD` bypass must be re-synced (`graphtor sync --force`)
//! to rebuild chunk IDs with section-based keys.
//!
//! Graph link types (`references`, `code_snippets`) are not produced for
//! PDF sources — structure cannot be recovered deterministically from
//! rendered text output.

use std::any::Any;
use std::cell::Cell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use crate::chunk::generate_chunk_id;
use crate::error::GraphtorError;
use crate::parse::types::{Chunk, ParsedDocument};

/// Maximum characters in a single chunk before splitting at paragraph boundaries.
const MAX_CHUNK_CHARS: usize = 2_000;

/// Minimum rendered-font-size ratio above body size to classify a line as H1.
const H1_RATIO: f64 = 1.6;

/// Minimum rendered-font-size ratio above body size to classify a line as H2.
const H2_RATIO: f64 = 1.3;

/// Maximum number of pages sampled by [`FontSizeHistogram`] in Pass 1.
///
/// Body font size is reliably determined from the first few pages of most
/// documents.  Capping the scan at 30 pages avoids an O(n) Pass 1 cost on
/// large PDFs while keeping histogram accuracy for typical technical documents.
const HISTOGRAM_SAMPLE_PAGES: u32 = 30;

/// File-size threshold (bytes) above which [`parse_pdf_document`] prefers the
/// [`PdfiumBackend`] (lazy document opening, lower peak memory) over
/// [`PdfExtractBackend`] (eager `Document::load_mem` parse) for improved
/// startup performance on large files.
///
/// This is a **performance** threshold only — [`PdfExtractBackend`] performs
/// full heading-aware extraction for all file sizes regardless of this
/// constant.  When the `PDFium` native library is not available,
/// [`PdfExtractBackend`] is used unconditionally.
const LARGE_PDF_THRESHOLD: usize = 20 * 1_024 * 1_024;

// ── rendered-font-size helper ─────────────────────────────────────────────────

/// Compute the rendered font size from a text-rendering matrix `trm`.
///
/// Mirrors the geometric-mean formula used internally by `PlainTextOutput`:
/// ```text
/// v = trm.transform_vector(vec2(font_size, font_size))
/// rendered_size = sqrt(v.x * v.y)
/// ```
///
/// Accesses the `euclid::Transform2D` matrix components directly to avoid
/// adding `euclid` as an explicit Cargo dependency (it is already a
/// transitive dependency of `pdf-extract`).
///
/// Falls back to `font_size.abs()` when the product would be non-positive
/// (degenerate or heavily-rotated matrix).
#[inline]
fn rendered_size(trm: &pdf_extract::Transform, font_size: f64) -> f64 {
    // transform_vector((font_size, font_size)):
    //   vx = m11 * font_size + m21 * font_size
    //   vy = m12 * font_size + m22 * font_size
    let vx = (trm.m11 + trm.m21) * font_size;
    let vy = (trm.m12 + trm.m22) * font_size;
    let product = vx * vy;
    if product > 0.0 {
        product.sqrt()
    } else {
        font_size.abs()
    }
}

// ── Unit 2: FontSizeHistogram ─────────────────────────────────────────────────

/// First-pass [`pdf_extract::OutputDev`] that builds a histogram of rendered
/// character counts per quantized font size.
///
/// Used to determine the dominant body font size before the heading-detection
/// pass. No text is accumulated — only font-size counts are recorded.
struct FontSizeHistogram {
    /// Histogram: key = `(quantized_size × 10) as u16`, value = character count.
    counts: HashMap<u16, usize>,
    /// Number of pages seen (incremented in `begin_page`).
    ///
    /// When `pages_seen > HISTOGRAM_SAMPLE_PAGES`, `output_character` becomes
    /// a no-op so the scan stops accumulating after the first 30 pages.
    pages_seen: u32,
}

impl FontSizeHistogram {
    /// Create an empty histogram.
    fn new() -> Self {
        Self {
            counts: HashMap::new(),
            pages_seen: 0,
        }
    }

    /// Quantize a font size to the nearest 0.5pt.
    ///
    /// Maps similar sizes (e.g. 9.8pt and 10.2pt) into the same bucket.
    /// The key is `(quantized × 10) as u16` to avoid floating-point map keys.
    ///
    /// Font sizes above ~6553pt are clamped to `u16::MAX` — this distorts the
    /// histogram for unusually large fonts, but practical PDFs never exceed
    /// ~200pt so the clamp is safe in production.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn quantize(font_size: f64) -> u16 {
        let quantized = (font_size * 2.0).round() / 2.0;
        let key = (quantized * 10.0).round();
        key.clamp(0.0, f64::from(u16::MAX)) as u16
    }

    /// Return the modal (most-frequent) quantized font size in points.
    ///
    /// Returns `10.0` when the histogram is empty — a reasonable default
    /// body-text size for most PDFs.
    fn body_font_size(&self) -> f64 {
        self.counts
            .iter()
            .max_by_key(|(_, &count)| count)
            .map_or(10.0, |(&key, _)| f64::from(key) / 10.0)
    }
}

impl pdf_extract::OutputDev for FontSizeHistogram {
    fn begin_page(
        &mut self,
        _page_num: u32,
        _media_box: &pdf_extract::MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), pdf_extract::OutputError> {
        self.pages_seen = self.pages_seen.saturating_add(1);
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn output_character(
        &mut self,
        trm: &pdf_extract::Transform,
        _width: f64,
        _spacing: f64,
        font_size: f64,
        char: &str,
    ) -> Result<(), pdf_extract::OutputError> {
        // Stop accumulating once we have sampled enough pages.
        if self.pages_seen > HISTOGRAM_SAMPLE_PAGES {
            return Ok(());
        }
        let size = rendered_size(trm, font_size);
        let key = Self::quantize(size);
        // Count Unicode scalar values (not bytes) to weight the histogram fairly.
        let char_count = char.chars().count();
        *self.counts.entry(key).or_insert(0) += char_count;
        Ok(())
    }

    fn begin_word(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn end_word(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }
}

// ── Unit 4: PageTextAccumulator ───────────────────────────────────────────────

/// [`pdf_extract::OutputDev`] that accumulates rendered text into one
/// `String` per page.
///
/// Used in the uniform-typography fallback and the large-file fast path to
/// reuse the already-loaded [`pdf_extract::Document`] rather than re-parsing
/// raw bytes via [`pdf_extract::extract_text_from_mem_by_pages`].
struct PageTextAccumulator {
    /// Accumulated text for all pages in document order.
    pages: Vec<String>,
    /// Text buffer for the page currently being processed.
    current_page: String,
    /// `true` when positioned at the first character of a new word.
    at_word_start: bool,
}

impl PageTextAccumulator {
    /// Create an empty accumulator.
    fn new() -> Self {
        Self {
            pages: Vec::new(),
            current_page: String::new(),
            at_word_start: false,
        }
    }

    /// Consume the accumulator and return the collected per-page strings.
    fn finish(self) -> Vec<String> {
        self.pages
    }
}

impl pdf_extract::OutputDev for PageTextAccumulator {
    fn begin_page(
        &mut self,
        _page_num: u32,
        _media_box: &pdf_extract::MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), pdf_extract::OutputError> {
        self.current_page.clear();
        self.at_word_start = false;
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), pdf_extract::OutputError> {
        self.pages.push(std::mem::take(&mut self.current_page));
        Ok(())
    }

    fn output_character(
        &mut self,
        _trm: &pdf_extract::Transform,
        _width: f64,
        _spacing: f64,
        _font_size: f64,
        char: &str,
    ) -> Result<(), pdf_extract::OutputError> {
        if self.at_word_start {
            if !self.current_page.is_empty() && !self.current_page.ends_with('\n') {
                self.current_page.push(' ');
            }
            self.at_word_start = false;
        }
        self.current_page.push_str(char);
        Ok(())
    }

    fn begin_word(&mut self) -> Result<(), pdf_extract::OutputError> {
        self.at_word_start = true;
        Ok(())
    }

    fn end_word(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), pdf_extract::OutputError> {
        if !self.current_page.is_empty() && !self.current_page.ends_with('\n') {
            self.current_page.push('\n');
        }
        Ok(())
    }
}

// ── Unit 5: HeadingFontDetector ───────────────────────────────────────────────

/// Third-pass [`pdf_extract::OutputDev`] that detects whether any character
/// in the scanned pages has a rendered font size qualifying as a heading.
///
/// Short-circuits after the first qualifying character so the scan terminates
/// as early as possible. Used to resolve the "uniform-sample false-positive":
/// when [`HISTOGRAM_SAMPLE_PAGES`] are all single-font but later pages contain
/// heading-sized text, [`PdfExtractBackend::parse`] uses this detector before
/// deciding to fall back to per-page chunking.
struct HeadingFontDetector {
    /// Minimum rendered font size (in points) to classify a character as a
    /// heading. Typically `body_font_size * H2_RATIO`.
    threshold: f64,
    /// Set to `true` the first time a character with rendered size ≥
    /// `threshold` is observed.
    found: bool,
}

impl HeadingFontDetector {
    /// Create a new detector that flags characters at or above `threshold`
    /// points as heading-sized.
    fn new(threshold: f64) -> Self {
        Self {
            threshold,
            found: false,
        }
    }

    /// Return `true` if any heading-sized character was observed.
    fn found_heading(&self) -> bool {
        self.found
    }
}

impl pdf_extract::OutputDev for HeadingFontDetector {
    fn begin_page(
        &mut self,
        _page_num: u32,
        _media_box: &pdf_extract::MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn output_character(
        &mut self,
        trm: &pdf_extract::Transform,
        _width: f64,
        _spacing: f64,
        font_size: f64,
        _char: &str,
    ) -> Result<(), pdf_extract::OutputError> {
        if !self.found {
            let size = rendered_size(trm, font_size);
            if size >= self.threshold {
                self.found = true;
            }
        }
        Ok(())
    }

    fn begin_word(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn end_word(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }
}

// ── Unit 3: HeadingAwareOutput ────────────────────────────────────────────────

/// A contiguous section of a PDF document bounded by heading boundaries.
///
/// Produced by [`HeadingAwareOutput`] and converted to [`Chunk`]s by
/// [`sections_to_chunks`].
#[derive(Debug)]
struct PdfSection {
    /// Heading text that opened this section, or `None` for the document intro.
    heading: Option<String>,
    /// Heading level: `1` = H1, `2` = H2, `0` = no heading (body-only section).
    heading_level: u8,
    /// Accumulated body text for this section.
    content: String,
}

/// Second-pass [`pdf_extract::OutputDev`] that accumulates text into
/// heading-bounded [`PdfSection`]s.
///
/// Heading boundaries are detected when a line's dominant rendered font size
/// exceeds `body_font_size` by [`H1_RATIO`] (≥ 1.6×) or [`H2_RATIO`] (≥ 1.3×).
/// Line boundaries are detected from y-coordinate changes in `trm.m32`.
struct HeadingAwareOutput {
    /// Dominant body font size determined by the first pass.
    body_font_size: f64,

    // ── current-line accumulation ─────────────────────────────────────────────
    /// Characters of the line being built.
    current_line: String,
    /// Rendered font sizes observed for each character on the current line.
    current_line_sizes: Vec<f64>,
    /// Raw y-coordinate of the last character (`trm.m32`), or `NaN` before
    /// the first character on a page.
    last_y: f64,
    /// `true` when positioned at the first character of a new word.
    word_start: bool,
    /// Rendered x-position after the last character (`trm.m31 + width × size`).
    last_x_end: f64,

    // ── section accumulation ──────────────────────────────────────────────────
    /// Accumulated body text for the current section.
    current_content: String,
    /// Heading text that opened the current section, or `None` (doc intro).
    current_heading: Option<String>,
    /// Heading level of the current section (`0` = no heading).
    current_heading_level: u8,

    // ── output ────────────────────────────────────────────────────────────────
    /// Completed sections in document order.
    sections: Vec<PdfSection>,
}

impl HeadingAwareOutput {
    /// Create a new accumulator with the known `body_font_size` from Pass 1.
    fn new(body_font_size: f64) -> Self {
        Self {
            body_font_size,
            current_line: String::new(),
            current_line_sizes: Vec::new(),
            last_y: f64::NAN,
            word_start: false,
            last_x_end: 0.0,
            current_content: String::new(),
            current_heading: None,
            current_heading_level: 0,
            sections: Vec::new(),
        }
    }

    /// Classify the completed current line and route it to the right buffer.
    ///
    /// Lines whose dominant font size meets [`H1_RATIO`] or [`H2_RATIO`]
    /// flush the current section and start a new one. Body lines are appended
    /// to `current_content`.
    fn flush_line(&mut self) {
        let line = std::mem::take(&mut self.current_line);
        let sizes = std::mem::take(&mut self.current_line_sizes);
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            return;
        }

        // Dominant size = maximum font size observed on this line.
        let dominant = sizes
            .iter()
            .copied()
            .filter(|s| s.is_finite() && *s > 0.0)
            .fold(0.0_f64, f64::max);

        if dominant >= self.body_font_size * H1_RATIO {
            self.flush_section();
            self.current_heading = Some(trimmed);
            self.current_heading_level = 1;
        } else if dominant >= self.body_font_size * H2_RATIO {
            self.flush_section();
            self.current_heading = Some(trimmed);
            self.current_heading_level = 2;
        } else {
            if !self.current_content.is_empty() {
                self.current_content.push('\n');
            }
            self.current_content.push_str(&trimmed);
        }
    }

    /// Commit the current section to `sections` and reset accumulation state.
    ///
    /// Skips empty sections (no heading and no content) to avoid emitting
    /// spurious chunks for intro gaps before the first heading.
    fn flush_section(&mut self) {
        let content = std::mem::take(&mut self.current_content);
        let heading = std::mem::take(&mut self.current_heading);
        let heading_level = self.current_heading_level;
        self.current_heading_level = 0;

        if heading.is_none() && content.trim().is_empty() {
            return;
        }

        self.sections.push(PdfSection {
            heading,
            heading_level,
            content,
        });
    }

    /// Flush all remaining buffered state and return the collected sections.
    fn finish(mut self) -> Vec<PdfSection> {
        self.flush_line();
        self.flush_section();
        self.sections
    }
}

impl pdf_extract::OutputDev for HeadingAwareOutput {
    fn begin_page(
        &mut self,
        _page_num: u32,
        _media_box: &pdf_extract::MediaBox,
        _art_box: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), pdf_extract::OutputError> {
        self.last_y = f64::NAN;
        self.last_x_end = 0.0;
        Ok(())
    }

    fn end_page(&mut self) -> Result<(), pdf_extract::OutputError> {
        // Flush the in-progress line at each page boundary.
        self.flush_line();
        Ok(())
    }

    fn output_character(
        &mut self,
        trm: &pdf_extract::Transform,
        width: f64,
        _spacing: f64,
        font_size: f64,
        char: &str,
    ) -> Result<(), pdf_extract::OutputError> {
        let size = rendered_size(trm, font_size);
        let raw_y = trm.m32;
        let raw_x = trm.m31;

        // Significant y-change → flush the completed line.
        if !self.last_y.is_nan() && (raw_y - self.last_y).abs() > size * 0.5 {
            self.flush_line();
        }

        // Insert word-spacing gap when the x-position has a notable gap.
        if self.word_start {
            if !self.current_line.is_empty() && raw_x > self.last_x_end + size * 0.1 {
                self.current_line.push(' ');
            }
            self.word_start = false;
        }

        self.current_line.push_str(char);
        self.current_line_sizes.push(size);
        self.last_y = raw_y;
        self.last_x_end = raw_x + width * size;
        Ok(())
    }

    fn begin_word(&mut self) -> Result<(), pdf_extract::OutputError> {
        self.word_start = true;
        Ok(())
    }

    fn end_word(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }

    fn end_line(&mut self) -> Result<(), pdf_extract::OutputError> {
        self.flush_line();
        Ok(())
    }
}

// ── Section → Chunk conversion ────────────────────────────────────────────────

/// Convert [`PdfSection`]s into [`Chunk`]s with proper heading hierarchies.
///
/// Tracks the most-recently seen H1 and H2 headings to build breadcrumb paths:
/// - Body after H1: `["H1 text"]`
/// - Body after H2: `["H1 text", "H2 text"]`
/// - Document intro (no heading encountered): `[]`
///
/// Long sections are split at paragraph boundaries via [`split_long_text`].
/// Chunk IDs use `{source_path}#section={N}#segment={M}`.
fn sections_to_chunks(
    sections: Vec<PdfSection>,
    source_path: &str,
) -> Result<Vec<Chunk>, GraphtorError> {
    let mut chunks = Vec::new();
    let mut position = 0_usize;
    let mut char_offset = 0_usize;
    let mut last_h1: Option<String> = None;
    let mut last_h2: Option<String> = None;

    for (section_idx, section) in sections.into_iter().enumerate() {
        let PdfSection {
            heading,
            heading_level,
            content,
        } = section;

        // Build hierarchy for chunks in this section using the CURRENT tracking
        // state (before updating it). For heading sections, the heading itself
        // is included; for body sections, accumulated h1/h2 context is used.
        let heading_hierarchy = match heading_level {
            1 => build_heading_hierarchy(heading.as_deref(), None),
            2 => build_heading_hierarchy(last_h1.as_deref(), heading.as_deref()),
            _ => build_heading_hierarchy(last_h1.as_deref(), last_h2.as_deref()),
        };

        // Chunk content: body text, or heading text when the body is empty.
        let content_text = if content.trim().is_empty() {
            heading.as_deref().unwrap_or("").trim().to_string()
        } else {
            content.trim().to_string()
        };

        // Update tracking state by moving `heading` — no clone needed.
        match heading_level {
            1 => {
                last_h1 = heading;
                last_h2 = None;
            }
            2 => {
                last_h2 = heading;
            }
            _ => {}
        }

        if content_text.is_empty() {
            continue;
        }

        let segments = split_long_text(&content_text);
        for (seg_idx, segment) in segments.into_iter().enumerate() {
            let chunk_id_source = format!("{source_path}#section={section_idx}#segment={seg_idx}");
            let chunk_id = generate_chunk_id(&segment, &chunk_id_source)?;
            let content_len = segment.len();
            chunks.push(Chunk {
                chunk_id,
                content: segment,
                heading_hierarchy: heading_hierarchy.clone(),
                position,
                char_offset,
                source_path: source_path.to_string(),
            });
            position += 1;
            char_offset += content_len;
        }
    }

    Ok(chunks)
}

/// Build a heading breadcrumb from the current H1 and H2 context.
fn build_heading_hierarchy(last_h1: Option<&str>, last_h2: Option<&str>) -> Vec<String> {
    let mut hier = Vec::new();
    if let Some(h1) = last_h1 {
        hier.push(h1.to_owned());
    }
    if let Some(h2) = last_h2 {
        hier.push(h2.to_owned());
    }
    hier
}

/// Extract a document title from a slice of [`PdfSection`]s.
///
/// Preference order:
/// 1. First heading text (H1 or H2) in the sections.
/// 2. First meaningful line (4–200 chars) in the first non-empty section body.
/// 3. File stem of `source_path`.
fn extract_title_from_sections(sections: &[PdfSection], source_path: &str) -> Option<String> {
    sections
        .iter()
        .find_map(|s| s.heading.as_deref())
        .map(str::to_owned)
        .or_else(|| {
            sections
                .iter()
                .find(|s| !s.content.is_empty())
                .and_then(|s| {
                    s.content
                        .lines()
                        .find(|l| l.len() > 3 && l.len() < 200)
                        .map(str::to_owned)
                })
        })
        .or_else(|| {
            Path::new(source_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(String::from)
        })
}

// ── PdfExtractBackend ─────────────────────────────────────────────────────────

/// Backend that encapsulates the [`pdf_extract`] two-pass extraction pipeline.
///
/// Acts as a concrete type boundary between the public [`parse_pdf_document`]
/// function and the internal extraction logic.  Future alternative backends
/// (e.g. `PdfiumBackend`) can be introduced without changing the public API.
pub(crate) struct PdfExtractBackend;

fn pdf_panic_hook_lock() -> &'static Mutex<()> {
    static PDF_PANIC_HOOK_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    PDF_PANIC_HOOK_LOCK.get_or_init(|| Mutex::new(()))
}

std::thread_local! {
    static PDF_PANIC_HOOK_SUPPRESSION_DEPTH: Cell<usize> = const { Cell::new(0) };
}

fn pdf_panic_hook_suppressed() -> bool {
    PDF_PANIC_HOOK_SUPPRESSION_DEPTH.with(|depth| depth.get() > 0)
}

fn with_pdf_panic_hook_suppressed<T, F>(work: F) -> T
where
    F: FnOnce() -> T,
{
    PDF_PANIC_HOOK_SUPPRESSION_DEPTH.with(|depth| {
        let previous_depth = depth.get();
        depth.set(previous_depth.saturating_add(1));
        let result = work();
        depth.set(previous_depth);
        result
    })
}

/// Execute a `pdf-extract` operation and convert dependency panics into parse errors.
fn with_pdf_panic_guard<T, F>(
    source_path: &str,
    operation: &str,
    work: F,
) -> Result<T, GraphtorError>
where
    F: FnOnce() -> Result<T, GraphtorError> + std::panic::UnwindSafe,
{
    let _panic_hook_guard = pdf_panic_hook_lock()
        .lock()
        .map_err(|_| GraphtorError::Parse {
            message: format!(
                "failed to silence pdf-extract panic hook during {operation}: panic hook lock poisoned"
            ),
            path: Some(source_path.into()),
        })?;
    let previous_hook_slot = Arc::new(Mutex::new(Some(std::panic::take_hook())));
    std::panic::set_hook({
        let previous_hook_slot = Arc::clone(&previous_hook_slot);
        Box::new(move |panic_info| {
            if pdf_panic_hook_suppressed() {
                return;
            }

            let previous_hook = previous_hook_slot.lock().ok();
            if let Some(previous_hook) = previous_hook {
                if let Some(previous_hook) = previous_hook.as_ref() {
                    previous_hook(panic_info);
                }
            }
        })
    });
    let result = with_pdf_panic_hook_suppressed(|| std::panic::catch_unwind(work));
    let _ = std::panic::take_hook();
    let previous_hook = previous_hook_slot
        .lock()
        .map_err(|_| GraphtorError::Parse {
            message: format!(
                "failed to restore pdf-extract panic hook during {operation}: previous panic hook lock poisoned"
            ),
            path: Some(source_path.into()),
        })?
        .take()
        .ok_or_else(|| GraphtorError::Parse {
            message: format!(
                "failed to restore pdf-extract panic hook during {operation}: previous panic hook missing"
            ),
            path: Some(source_path.into()),
        })?;
    std::panic::set_hook(previous_hook);
    result.map_err(|payload| GraphtorError::Parse {
        message: format!(
            "pdf-extract panicked during {operation}: {}",
            panic_payload_message(payload.as_ref())
        ),
        path: Some(source_path.into()),
    })?
}

/// Convert an unwind payload into a human-readable panic message.
fn panic_payload_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else {
        "unknown panic payload".to_string()
    }
}

impl PdfExtractBackend {
    /// Parse raw PDF bytes using the two-pass `pdf-extract` pipeline.
    ///
    /// ## Two-Pass Architecture
    ///
    /// **Pass 1 — Font-size histogram**: [`FontSizeHistogram`] scans the first
    /// [`HISTOGRAM_SAMPLE_PAGES`] pages (30) to determine the dominant body
    /// font size. No text is accumulated.
    ///
    /// **Pass 2 — Heading-aware extraction**: [`HeadingAwareOutput`] processes
    /// all pages via an `output_doc_page` loop with the known body font size
    /// and emits [`PdfSection`]s.  The incremental loop keeps each page's
    /// content in scope only while it is being processed, and is equivalent
    /// in total work to `output_doc` while allowing early termination and
    /// finer error attribution.
    ///
    /// **Fallback**: When `distinct_sizes ≤ 1` (uniform font size or empty
    /// document), the pipeline checks pages beyond the sample window using
    /// [`HeadingFontDetector`]. If heading-sized characters are found, the
    /// heading-aware pass runs normally. Otherwise, falls back to
    /// [`PageTextAccumulator`]-based per-page chunking with `["Page N"]`
    /// hierarchy.
    ///
    /// This function handles all file sizes — there is no large-file bypass.
    /// For large files where the `PDFium` native library is available, prefer
    /// [`parse_pdf_document`] which tries [`PdfiumBackend`] first for lower
    /// peak memory.
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Parse`] if `pdf-extract` cannot decode the
    /// bytes as a valid PDF, if `pdf-extract` panics internally, or if chunk
    /// ID generation fails.
    pub(crate) fn parse(bytes: &[u8], source_path: &str) -> Result<ParsedDocument, GraphtorError> {
        with_pdf_panic_guard(source_path, "pdf parsing", || {
            // Load the PDF document once — reused for all passes.
            let doc = pdf_extract::Document::load_mem(bytes).map_err(|e| GraphtorError::Parse {
                message: format!("pdf load failed: {e}"),
                path: Some(source_path.into()),
            })?;

            // Pass 1: build font-size histogram (first HISTOGRAM_SAMPLE_PAGES pages only).
            let page_count = u32::try_from(doc.get_pages().len()).unwrap_or(u32::MAX);
            let sample_end = page_count.min(HISTOGRAM_SAMPLE_PAGES);
            let mut histogram = FontSizeHistogram::new();
            for page_num in 1..=sample_end {
                pdf_extract::output_doc_page(&doc, &mut histogram, page_num).map_err(|e| {
                    GraphtorError::Parse {
                        message: format!("pdf font-size scan failed: {e}"),
                        path: Some(source_path.into()),
                    }
                })?;
            }

            let body_font_size = histogram.body_font_size();
            let distinct_sizes = histogram.counts.len();

            // Resolve the "uniform-sample" false-positive: when the first
            // HISTOGRAM_SAMPLE_PAGES pages all share one font size but later pages
            // contain heading-sized text, we must not fall back to per-page chunking.
            let really_uniform = if distinct_sizes <= 1 && page_count > HISTOGRAM_SAMPLE_PAGES {
                let h2_threshold = body_font_size * H2_RATIO;
                let mut detector = HeadingFontDetector::new(h2_threshold);
                for page_num in (sample_end + 1)..=page_count {
                    pdf_extract::output_doc_page(&doc, &mut detector, page_num).map_err(|e| {
                        GraphtorError::Parse {
                            message: format!("pdf heading-font scan failed: {e}"),
                            path: Some(source_path.into()),
                        }
                    })?;
                    if detector.found_heading() {
                        break;
                    }
                }
                !detector.found_heading()
            } else {
                distinct_sizes <= 1
            };

            let (chunks, title) = if really_uniform {
                // Uniform or empty document — fall back to per-page chunking.
                let mut acc = PageTextAccumulator::new();
                for page_num in 1..=page_count {
                    pdf_extract::output_doc_page(&doc, &mut acc, page_num).map_err(|e| {
                        GraphtorError::Parse {
                            message: format!(
                                "pdf per-page extraction failed at page {page_num}: {e}"
                            ),
                            path: Some(source_path.into()),
                        }
                    })?;
                }
                let pages = acc.finish();
                let title = extract_title_from_pages(&pages, source_path);
                let chunks = chunk_pdf_pages(&pages, source_path)?;
                (chunks, title)
            } else {
                // Pass 2: heading-aware extraction using an output_doc_page loop so
                // heading state accumulates incrementally across all pages.
                let mut heading_output = HeadingAwareOutput::new(body_font_size);
                for page_num in 1..=page_count {
                    pdf_extract::output_doc_page(&doc, &mut heading_output, page_num).map_err(
                        |e| GraphtorError::Parse {
                            message: format!(
                                "pdf heading-aware extraction failed at page {page_num}: {e}"
                            ),
                            path: Some(source_path.into()),
                        },
                    )?;
                }
                let sections = heading_output.finish();
                let title = extract_title_from_sections(&sections, source_path);
                let chunks = sections_to_chunks(sections, source_path)?;
                (chunks, title)
            };

            Ok(ParsedDocument {
                path: source_path.to_string(),
                title,
                frontmatter: None,
                chunks,
                references: Vec::new(),
                code_snippets: Vec::new(),
            })
        })
    }
}

// ── PdfiumBackend ─────────────────────────────────────────────────────────────

/// Backend that uses `pdfium-render` for text extraction via the `PDFium` library.
///
/// `PDFium` opens documents lazily — only the cross-reference index is read at
/// open time, and page content is decompressed on demand.  This makes it
/// dramatically faster than `pdf-extract` (which eagerly parses every object)
/// for large PDFs (100+ MB).
///
/// the `PDFium` native library (`.dll`/`.so`/`.dylib`) must be present at
/// runtime.  Discovery order:
///
/// 1. `$GRAPHTOR_PDFIUM_PATH` environment variable (directory containing the
///    library)
/// 2. Executable's own directory
/// 3. System library search path
///
/// When the library is not found, [`PdfiumBackend::try_parse`] returns
/// [`PdfiumBindError::NotAvailable`] and the caller falls back to
/// [`PdfExtractBackend`].
pub(crate) struct PdfiumBackend;

/// Categorized pdfium errors to distinguish "library not installed" (expected
/// fallback) from "library found but extraction failed" (real bug).
#[derive(Debug)]
enum PdfiumBindError {
    /// the `PDFium` native library could not be located or loaded.
    NotAvailable(String),
    /// The library loaded but PDF parsing or text extraction failed.
    ExtractionFailed(String),
}

impl std::fmt::Display for PdfiumBindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAvailable(msg) => write!(f, "pdfium not available: {msg}"),
            Self::ExtractionFailed(msg) => write!(f, "pdfium extraction failed: {msg}"),
        }
    }
}

impl PdfiumBackend {
    /// Attempt to parse PDF bytes using the `PDFium` backend.
    ///
    /// Returns `Ok(ParsedDocument)` on success, `Err(PdfiumBindError::NotAvailable)`
    /// when the `PDFium` library is not found (caller should fall back), or
    /// `Err(PdfiumBindError::ExtractionFailed)` when the library loaded but
    /// extraction failed.  The caller (`parse_pdf_document`) logs and falls
    /// back to `PdfExtractBackend` on either error variant — `ExtractionFailed`
    /// is logged at error level to surface potential issues while still
    /// providing a best-effort result.
    fn try_parse(bytes: &[u8], source_path: &str) -> Result<ParsedDocument, PdfiumBindError> {
        let pdfium = Self::load_pdfium()?;

        let document = pdfium
            .load_pdf_from_byte_slice(bytes, None)
            .map_err(|e| PdfiumBindError::ExtractionFailed(format!("pdf open failed: {e}")))?;

        let page_count = document.pages().len();
        let page_count_usize = usize::try_from(page_count).unwrap_or(0);
        tracing::info!(
            pages = page_count_usize,
            source_path,
            "pdfium: document opened (lazy)"
        );

        // Extract text page-by-page using pdfium's text extraction.
        let mut pages: Vec<String> = Vec::with_capacity(page_count_usize);
        for page_idx in 0..page_count {
            let page = document.pages().get(page_idx).map_err(|e| {
                PdfiumBindError::ExtractionFailed(format!("page {page_idx} access failed: {e}"))
            })?;
            let text = page
                .text()
                .map_err(|e| {
                    PdfiumBindError::ExtractionFailed(format!(
                        "page {page_idx} text extraction failed: {e}"
                    ))
                })?
                .all();
            pages.push(text);
        }

        let title = extract_title_from_pages(&pages, source_path);
        let chunks = chunk_pdf_pages(&pages, source_path)
            .map_err(|e| PdfiumBindError::ExtractionFailed(format!("chunking failed: {e}")))?;

        tracing::info!(
            chunks = chunks.len(),
            pages = page_count_usize,
            source_path,
            backend = "pdfium",
            "pdfium: extraction complete"
        );

        Ok(ParsedDocument {
            path: source_path.to_string(),
            title,
            frontmatter: None,
            chunks,
            references: Vec::new(),
            code_snippets: Vec::new(),
        })
    }

    /// Locate and bind the `PDFium` native library.
    ///
    /// Search order:
    /// 1. `$GRAPHTOR_PDFIUM_PATH` (directory containing the library file)
    /// 2. Executable's directory
    /// 3. System library search path
    fn load_pdfium() -> Result<pdfium_render::prelude::Pdfium, PdfiumBindError> {
        use pdfium_render::prelude::*;

        // 1. Explicit environment variable
        if let Ok(dir) = std::env::var("GRAPHTOR_PDFIUM_PATH") {
            let path = PathBuf::from(&dir);
            if path.is_dir() {
                if let Ok(bindings) =
                    Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&dir))
                {
                    tracing::debug!(path = %dir, "pdfium: loaded from GRAPHTOR_PDFIUM_PATH");
                    return Ok(Pdfium::new(bindings));
                }
            }
        }

        // 2. Executable's directory
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let dir_str = exe_dir.to_string_lossy();
                if let Ok(bindings) = Pdfium::bind_to_library(
                    Pdfium::pdfium_platform_library_name_at_path(dir_str.as_ref()),
                ) {
                    tracing::debug!(path = %dir_str, "pdfium: loaded from executable dir");
                    return Ok(Pdfium::new(bindings));
                }
            }
        }

        // 3. System library search path
        if let Ok(bindings) = Pdfium::bind_to_system_library() {
            tracing::debug!("pdfium: loaded from system library path");
            return Ok(Pdfium::new(bindings));
        }

        Err(PdfiumBindError::NotAvailable(
            "pdfium native library not found in GRAPHTOR_PDFIUM_PATH, \
             executable directory, or system search path"
                .to_string(),
        ))
    }
}

// ── Main entry point ──────────────────────────────────────────────────────────

/// Parse raw PDF bytes into a fully assembled [`ParsedDocument`].
///
/// ## Backend Selection
///
/// For PDFs larger than [`LARGE_PDF_THRESHOLD`] (20 MiB), the parser first
/// attempts the [`PdfiumBackend`], which opens documents lazily (low peak
/// memory for very large files).  When the `PDFium` native library is not
/// installed, the system falls back to [`PdfExtractBackend`].
///
/// For PDFs smaller than the threshold, [`PdfExtractBackend`] is used
/// directly.  Both backends produce high-quality heading-aware extraction
/// when possible — the threshold is a **performance** hint, not a quality
/// boundary.
///
/// ## Chunk ID Format
///
/// - Section-based (heading-aware path): `{source_path}#section={N}#segment={M}`
/// - Page-based (uniform-font fallback): `{source_path}#page={N}#segment={M}`
///
/// Previously ingested PDFs produced by an older version that used the
/// `LARGE_PDF_THRESHOLD` bypass must be re-synced (`graphtor sync --force`)
/// to rebuild chunk IDs with section-based keys.
///
/// # Errors
///
/// Returns [`GraphtorError::Parse`] if the bytes are not a valid PDF, if a
/// PDF backend panics internally, or if chunk ID generation fails.
pub fn parse_pdf_document(
    bytes: &[u8],
    source_path: &str,
) -> Result<ParsedDocument, GraphtorError> {
    // Large PDFs: try pdfium first for instant document opening.
    if bytes.len() >= LARGE_PDF_THRESHOLD {
        match PdfiumBackend::try_parse(bytes, source_path) {
            Ok(doc) => return Ok(doc),
            Err(PdfiumBindError::NotAvailable(reason)) => {
                tracing::warn!(
                    source_path,
                    reason = %reason,
                    hint = "set GRAPHTOR_PDFIUM_PATH to the directory containing the pdfium \
                            library, or place it next to the graphtor-docs executable",
                    "pdfium unavailable for large pdf, falling back to pdf-extract \
                     (this may be slow for files >20 MiB)"
                );
            }
            Err(PdfiumBindError::ExtractionFailed(reason)) => {
                tracing::error!(
                    source_path,
                    reason = %reason,
                    "pdfium extraction failed, falling back to pdf-extract"
                );
            }
        }
    }

    PdfExtractBackend::parse(bytes, source_path)
}

// ── Unit 1: per-page chunking (fallback path) ─────────────────────────────────

/// Split per-page text strings into [`Chunk`]s at page and paragraph boundaries.
///
/// Each page's text is trimmed; empty pages are skipped. Pages longer than
/// [`MAX_CHUNK_CHARS`] are further split at double-newline paragraph boundaries.
///
/// Chunk IDs use the format `{source_path}#page={N}#segment={M}` (1-based).
fn chunk_pdf_pages(pages: &[String], source_path: &str) -> Result<Vec<Chunk>, GraphtorError> {
    let mut chunks: Vec<Chunk> = Vec::new();
    let mut position = 0_usize;
    let mut char_offset = 0_usize;

    for (page_idx, page_text) in pages.iter().enumerate() {
        let trimmed = page_text.trim();
        if trimmed.is_empty() {
            continue;
        }

        let page_label = format!("Page {}", page_idx + 1);
        let segments = split_long_text(trimmed);

        for (segment_idx, segment) in segments.into_iter().enumerate() {
            let chunk_id_source =
                format!("{source_path}#page={}#segment={segment_idx}", page_idx + 1);
            let chunk_id = generate_chunk_id(&segment, &chunk_id_source)?;
            let content_len = segment.len();
            chunks.push(Chunk {
                chunk_id,
                content: segment,
                heading_hierarchy: vec![page_label.clone()],
                position,
                char_offset,
                source_path: source_path.to_string(),
            });
            position += 1;
            char_offset += content_len;
        }
    }

    Ok(chunks)
}

/// Extract a title from per-page text strings.
///
/// Searches pages in order for the first meaningful line (4–200 chars).
/// Falls back to the file stem of `source_path`.
fn extract_title_from_pages(pages: &[String], source_path: &str) -> Option<String> {
    let candidate = pages
        .iter()
        .flat_map(|p| p.lines())
        .map(str::trim)
        .find(|line| line.len() > 3 && line.len() < 200);

    candidate.map(String::from).or_else(|| {
        Path::new(source_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from)
    })
}

/// Split a text segment at paragraph (`\n\n`) boundaries when it exceeds
/// [`MAX_CHUNK_CHARS`].
///
/// Short segments are returned unchanged in a single-element `Vec`.
///
/// If a single paragraph itself exceeds [`MAX_CHUNK_CHARS`] (no `\n\n`
/// separators available), the paragraph is split at word boundaries so no
/// output segment exceeds the limit.
fn split_long_text(text: &str) -> Vec<String> {
    if text.len() <= MAX_CHUNK_CHARS {
        return vec![text.to_string()];
    }

    let mut segments: Vec<String> = Vec::new();
    let mut current = String::new();

    for para in text.split("\n\n") {
        // If the paragraph itself exceeds the limit, split it at word boundaries.
        let para_pieces: Vec<&str> = if para.len() > MAX_CHUNK_CHARS {
            split_at_word_boundaries(para)
        } else {
            vec![para]
        };

        for piece in para_pieces {
            if current.is_empty() {
                current.push_str(piece);
            } else if current.len() + 2 + piece.len() > MAX_CHUNK_CHARS {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    segments.push(trimmed);
                }
                current = piece.to_string();
            } else {
                current.push_str("\n\n");
                current.push_str(piece);
            }
        }
    }

    let tail = current.trim().to_string();
    if !tail.is_empty() {
        segments.push(tail);
    }

    if segments.is_empty() {
        vec![text.to_string()]
    } else {
        segments
    }
}

/// Split `text` at word boundaries so each piece is at most [`MAX_CHUNK_CHARS`]
/// characters long.
///
/// Uses character-count boundaries (via `char_indices`) to avoid slicing
/// mid-UTF-8 code point — safe for non-ASCII text including CJK and emoji.
///
/// Used as a fallback inside [`split_long_text`] when a single paragraph has no
/// `\n\n` separators but exceeds the chunk size limit.
fn split_at_word_boundaries(text: &str) -> Vec<&str> {
    let mut pieces: Vec<&str> = Vec::new();
    let mut start = 0;

    while start < text.len() {
        let remaining = &text[start..];
        if remaining.len() <= MAX_CHUNK_CHARS {
            pieces.push(remaining);
            break;
        }
        // Advance exactly MAX_CHUNK_CHARS *characters* (not bytes) to guarantee
        // the cut falls on a valid UTF-8 char boundary.
        let end_byte = match remaining.char_indices().nth(MAX_CHUNK_CHARS) {
            Some((idx, _)) => idx,
            None => remaining.len(),
        };
        // Walk back to the last space within the char-bounded slice.
        let split_byte = remaining[..end_byte]
            .rfind(' ')
            .map_or(end_byte, |pos| pos + 1); // +1: skip the space itself
                                              // Guard against zero advance (e.g. space at position 0).
        let advance = if split_byte == 0 {
            end_byte.max(1)
        } else {
            split_byte
        };
        pieces.push(remaining[..advance].trim_end());
        start += advance;
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::{
        build_heading_hierarchy, chunk_pdf_pages, extract_title_from_pages,
        extract_title_from_sections, panic_payload_message, sections_to_chunks,
        split_at_word_boundaries, split_long_text, with_pdf_panic_guard, FontSizeHistogram,
        HeadingAwareOutput, HeadingFontDetector, PageTextAccumulator, PdfExtractBackend,
        PdfSection, H1_RATIO, H2_RATIO, HISTOGRAM_SAMPLE_PAGES, LARGE_PDF_THRESHOLD,
        MAX_CHUNK_CHARS,
    };
    // Import the OutputDev trait so method calls resolve on concrete types.
    use crate::error::GraphtorError;
    use pdf_extract::OutputDev as _;

    // ── test helpers ─────────────────────────────────────────────────────────

    /// Construct a pure-scaling + translation text-rendering matrix.
    ///
    /// With this matrix, `rendered_size(trm, font_size) = scale * font_size`
    /// and `trm.m32 = y` (raw y-position for line-change detection).
    fn make_trm(scale: f64, x: f64, y: f64) -> pdf_extract::Transform {
        // row_major(m11, m12, m21, m22, m31, m32)
        pdf_extract::Transform::row_major(scale, 0.0, 0.0, scale, x, y)
    }

    fn default_media_box() -> pdf_extract::MediaBox {
        pdf_extract::MediaBox {
            llx: 0.0,
            lly: 0.0,
            urx: 612.0,
            ury: 792.0,
        }
    }

    /// Emit a sequence of characters from `text` at the given position and
    /// font size, via the `OutputDev` interface.
    fn emit_text(
        output: &mut dyn pdf_extract::OutputDev,
        text: &str,
        font_size: f64,
        x: f64,
        y: f64,
    ) {
        let trm = make_trm(1.0, x, y);
        for ch in text.chars() {
            output
                .output_character(&trm, 0.1, 0.0, font_size, &ch.to_string())
                .expect("output_character should not fail in tests");
        }
    }

    // ── Unit 1: chunk_pdf_pages ───────────────────────────────────────────────

    #[test]
    fn chunk_pages_empty_produces_no_chunks() {
        let result = chunk_pdf_pages(&[], "test.pdf").expect("empty pages should not fail");
        assert!(
            result.is_empty(),
            "empty page slice must produce zero chunks"
        );
    }

    #[test]
    fn chunk_pages_single_page_produces_one_chunk() {
        let pages = vec!["Hello, world!".to_string()];
        let result = chunk_pdf_pages(&pages, "test.pdf").expect("single page should succeed");
        assert_eq!(result.len(), 1, "single page must produce one chunk");
        assert_eq!(result[0].heading_hierarchy, vec!["Page 1"]);
        assert_eq!(result[0].position, 0);
        assert_eq!(result[0].source_path, "test.pdf");
    }

    #[test]
    fn chunk_pages_two_pages_produces_two_chunks() {
        let pages = vec![
            "Page one content".to_string(),
            "Page two content".to_string(),
        ];
        let result = chunk_pdf_pages(&pages, "two_pages.pdf").expect("two pages should succeed");
        assert_eq!(result.len(), 2, "two pages must produce two chunks");
        assert_eq!(result[0].heading_hierarchy, vec!["Page 1"]);
        assert_eq!(result[1].heading_hierarchy, vec!["Page 2"]);
    }

    #[test]
    fn chunk_pages_empty_page_is_skipped() {
        let pages = vec![
            "Page one".to_string(),
            "   \n  ".to_string(), // whitespace-only → empty
            "Page three".to_string(),
        ];
        let result =
            chunk_pdf_pages(&pages, "skip.pdf").expect("empty page should not cause failure");
        assert_eq!(result.len(), 2, "whitespace-only page must be skipped");
    }

    #[test]
    fn chunk_pages_positions_are_sequential() {
        let pages = vec!["Alpha".to_string(), "Beta".to_string(), "Gamma".to_string()];
        let result = chunk_pdf_pages(&pages, "seq.pdf").expect("three pages should succeed");
        assert_eq!(result.len(), 3);
        for (i, chunk) in result.iter().enumerate() {
            assert_eq!(chunk.position, i, "chunk position must equal its index");
        }
    }

    #[test]
    fn chunk_pages_long_page_splits_at_paragraphs() {
        let para_a = "A".repeat(MAX_CHUNK_CHARS / 2 + 10);
        let para_b = "B".repeat(MAX_CHUNK_CHARS / 2 + 10);
        let pages = vec![format!("{para_a}\n\n{para_b}")];
        let result = chunk_pdf_pages(&pages, "long.pdf").expect("long page should succeed");
        assert!(
            result.len() >= 2,
            "a long page must be split into at least two chunks"
        );
    }

    #[test]
    fn chunk_pages_ids_are_unique() {
        let pages = vec!["First page".to_string(), "Second page".to_string()];
        let result = chunk_pdf_pages(&pages, "unique.pdf").expect("should succeed");
        assert_eq!(result.len(), 2);
        assert_ne!(
            result[0].chunk_id, result[1].chunk_id,
            "different pages must produce different chunk IDs"
        );
        for chunk in &result {
            assert_eq!(
                chunk.chunk_id.len(),
                64,
                "chunk ID must be 64 hex characters"
            );
        }
    }

    #[test]
    fn chunk_pages_same_count_as_form_feed_split() {
        // Verify Unit 1 behavior: per-page produces the same chunk count as the
        // former form-feed split approach for equivalent input.
        let text_blob = "Alpha\x0cBeta\x0cGamma";
        let page_count = text_blob
            .split('\x0c')
            .filter(|p| !p.trim().is_empty())
            .count();
        let pages: Vec<String> = text_blob.split('\x0c').map(ToString::to_string).collect();
        let result = chunk_pdf_pages(&pages, "equiv.pdf").expect("should succeed");
        assert_eq!(
            result.len(),
            page_count,
            "per-page chunking must produce the same chunk count as form-feed splitting"
        );
    }

    // ── Unit 2: FontSizeHistogram ─────────────────────────────────────────────

    #[test]
    #[allow(clippy::float_cmp)]
    fn histogram_empty_returns_default_body_size() {
        let hist = FontSizeHistogram::new();
        assert_eq!(
            hist.body_font_size(),
            10.0,
            "empty histogram must return 10.0 as default"
        );
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn histogram_single_size_returns_that_size() {
        let mut hist = FontSizeHistogram::new();
        let trm = make_trm(1.0, 0.0, 0.0);
        hist.output_character(&trm, 0.1, 0.0, 12.0, "a")
            .expect("should not fail");
        hist.output_character(&trm, 0.1, 0.0, 12.0, "b")
            .expect("should not fail");
        assert_eq!(
            hist.body_font_size(),
            12.0,
            "single-size histogram must return that size"
        );
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn histogram_mixed_sizes_returns_mode() {
        let mut hist = FontSizeHistogram::new();
        let trm = make_trm(1.0, 0.0, 0.0);
        // 3 chars at 10pt (body) — dominant
        for _ in 0..3 {
            hist.output_character(&trm, 0.1, 0.0, 10.0, "x")
                .expect("ok");
        }
        // 1 char at 18pt (heading)
        hist.output_character(&trm, 0.1, 0.0, 18.0, "H")
            .expect("ok");
        assert_eq!(
            hist.body_font_size(),
            10.0,
            "mode of histogram must be the most-frequent size"
        );
    }

    #[test]
    fn histogram_quantization_groups_similar_sizes() {
        // 9.8pt and 10.2pt both quantize to 10.0pt.
        assert_eq!(
            FontSizeHistogram::quantize(9.8),
            FontSizeHistogram::quantize(10.2),
            "sizes within 0.5pt of each other must map to the same bucket"
        );
        assert_ne!(
            FontSizeHistogram::quantize(10.0),
            FontSizeHistogram::quantize(10.6),
            "sizes more than 0.5pt apart must map to different buckets"
        );
    }

    #[test]
    fn histogram_quantization_zero_returns_zero() {
        assert_eq!(FontSizeHistogram::quantize(0.0), 0);
    }

    // ── Unit 3: HeadingAwareOutput ────────────────────────────────────────────

    #[test]
    fn heading_aware_large_font_becomes_h1_section() {
        let mb = default_media_box();
        let mut out = HeadingAwareOutput::new(10.0);
        out.begin_page(1, &mb, None).expect("begin_page ok");

        // H1 line at y=700 (18pt > 10 * 1.6 = 16)
        emit_text(&mut out, "Chapter One", 18.0, 50.0, 700.0);
        // Body line at y=680 — y-change triggers flush of "Chapter One"
        emit_text(&mut out, "Body text here", 10.0, 50.0, 680.0);
        out.end_page().expect("end_page ok");

        let sections = out.finish();
        let h1 = sections.iter().find(|s| s.heading_level == 1);
        assert!(h1.is_some(), "must produce an H1 section");
        assert_eq!(
            h1.unwrap().heading.as_deref(),
            Some("Chapter One"),
            "H1 heading text must match"
        );
    }

    #[test]
    fn heading_aware_medium_font_becomes_h2_section() {
        let mb = default_media_box();
        let mut out = HeadingAwareOutput::new(10.0);
        out.begin_page(1, &mb, None).expect("begin_page ok");

        // H2 line at y=700 (14pt, >= 10 * 1.3 = 13 but < 10 * 1.6 = 16)
        emit_text(&mut out, "Section 1.1", 14.0, 50.0, 700.0);
        emit_text(&mut out, "Section body", 10.0, 50.0, 680.0);
        out.end_page().expect("end_page ok");

        let sections = out.finish();
        let h2 = sections.iter().find(|s| s.heading_level == 2);
        assert!(h2.is_some(), "must produce an H2 section");
        assert_eq!(h2.unwrap().heading.as_deref(), Some("Section 1.1"));
    }

    #[test]
    fn heading_aware_document_intro_has_no_heading() {
        let mb = default_media_box();
        let mut out = HeadingAwareOutput::new(10.0);
        out.begin_page(1, &mb, None).expect("begin_page ok");

        // Only body text — no heading-sized characters
        emit_text(&mut out, "Introduction paragraph", 10.0, 50.0, 700.0);
        out.end_page().expect("end_page ok");

        let sections = out.finish();
        assert!(!sections.is_empty(), "must produce at least one section");
        assert!(
            sections[0].heading.is_none(),
            "intro section must have no heading"
        );
        assert_eq!(sections[0].heading_level, 0);
    }

    #[test]
    fn heading_aware_empty_page_produces_no_section() {
        let mb = default_media_box();
        let mut out = HeadingAwareOutput::new(10.0);
        out.begin_page(1, &mb, None).expect("begin_page ok");
        // No characters emitted — page is empty
        out.end_page().expect("end_page ok");

        let sections = out.finish();
        assert!(sections.is_empty(), "empty page must produce no sections");
    }

    #[test]
    fn heading_aware_multiple_levels_detected() {
        let mb = default_media_box();
        let mut out = HeadingAwareOutput::new(10.0);
        out.begin_page(1, &mb, None).expect("begin_page ok");

        // H1 at y=750
        emit_text(&mut out, "Chapter One", 18.0, 50.0, 750.0);
        // H2 at y=720 — y-change flushes H1 line
        emit_text(&mut out, "Section 1.1", 14.0, 50.0, 720.0);
        // Body at y=690
        emit_text(&mut out, "Body content", 10.0, 50.0, 690.0);
        out.end_page().expect("end_page ok");

        let sections = out.finish();
        let h1_count = sections.iter().filter(|s| s.heading_level == 1).count();
        let h2_count = sections.iter().filter(|s| s.heading_level == 2).count();
        assert_eq!(h1_count, 1, "must produce exactly one H1 section");
        assert_eq!(h2_count, 1, "must produce exactly one H2 section");
    }

    // ── sections_to_chunks ────────────────────────────────────────────────────

    #[test]
    fn sections_to_chunks_empty_produces_no_chunks() {
        let chunks = sections_to_chunks(vec![], "doc.pdf").expect("empty sections ok");
        assert!(chunks.is_empty());
    }

    #[test]
    fn sections_to_chunks_h1_hierarchy() {
        let sections = vec![PdfSection {
            heading: Some("Chapter 1".to_string()),
            heading_level: 1,
            content: "Body text".to_string(),
        }];
        let chunks = sections_to_chunks(sections, "doc.pdf").expect("ok");
        assert!(!chunks.is_empty());
        assert_eq!(chunks[0].heading_hierarchy, vec!["Chapter 1"]);
    }

    #[test]
    fn sections_to_chunks_h2_inherits_h1() {
        let sections = vec![
            PdfSection {
                heading: Some("Chapter 1".to_string()),
                heading_level: 1,
                content: "Intro".to_string(),
            },
            PdfSection {
                heading: Some("Section 1.1".to_string()),
                heading_level: 2,
                content: "Detail".to_string(),
            },
        ];
        let chunks = sections_to_chunks(sections, "doc.pdf").expect("ok");
        // Second chunk belongs to the H2 section
        let h2_chunk = chunks
            .iter()
            .find(|c| c.content == "Detail")
            .expect("h2 chunk");
        assert_eq!(
            h2_chunk.heading_hierarchy,
            vec!["Chapter 1", "Section 1.1"],
            "H2 chunk must inherit H1 in its hierarchy"
        );
    }

    #[test]
    fn sections_to_chunks_intro_has_empty_hierarchy() {
        let sections = vec![PdfSection {
            heading: None,
            heading_level: 0,
            content: "Before any heading".to_string(),
        }];
        let chunks = sections_to_chunks(sections, "doc.pdf").expect("ok");
        assert!(!chunks.is_empty());
        assert!(
            chunks[0].heading_hierarchy.is_empty(),
            "pre-heading section must have empty hierarchy"
        );
    }

    #[test]
    fn sections_to_chunks_ids_use_section_format() {
        let sections = vec![PdfSection {
            heading: Some("H1".to_string()),
            heading_level: 1,
            content: "content".to_string(),
        }];
        let chunks = sections_to_chunks(sections, "doc.pdf").expect("ok");
        // Chunk IDs are SHA-256 hashes — we can't assert the format from the ID itself.
        // Verify uniqueness and length instead.
        assert_eq!(chunks[0].chunk_id.len(), 64);
    }

    #[test]
    fn sections_to_chunks_h2_hierarchy_resets_after_new_h1() {
        let sections = vec![
            PdfSection {
                heading: Some("Chapter 1".to_string()),
                heading_level: 1,
                content: String::new(),
            },
            PdfSection {
                heading: Some("Section 1.1".to_string()),
                heading_level: 2,
                content: String::new(),
            },
            PdfSection {
                heading: Some("Chapter 2".to_string()),
                heading_level: 1,
                content: "Chapter 2 body".to_string(),
            },
        ];
        let chunks = sections_to_chunks(sections, "doc.pdf").expect("ok");
        let ch2_chunk = chunks
            .iter()
            .find(|c| c.content == "Chapter 2 body")
            .expect("chapter 2 chunk");
        assert_eq!(
            ch2_chunk.heading_hierarchy,
            vec!["Chapter 2"],
            "H2 context must reset when a new H1 is encountered"
        );
    }

    // ── build_heading_hierarchy ───────────────────────────────────────────────

    #[test]
    fn heading_hierarchy_none_none_is_empty() {
        assert!(build_heading_hierarchy(None, None).is_empty());
    }

    #[test]
    fn heading_hierarchy_h1_only() {
        let h = build_heading_hierarchy(Some("Ch 1"), None);
        assert_eq!(h, vec!["Ch 1"]);
    }

    #[test]
    fn heading_hierarchy_h1_and_h2() {
        let h = build_heading_hierarchy(Some("Ch 1"), Some("Sec 1.1"));
        assert_eq!(h, vec!["Ch 1", "Sec 1.1"]);
    }

    // ── extract_title_from_pages ──────────────────────────────────────────────

    #[test]
    fn title_from_pages_returns_first_meaningful_line() {
        let pages = vec!["\n\nDocument Title\nSome other content".to_string()];
        let title = extract_title_from_pages(&pages, "my_doc.pdf");
        assert_eq!(title, Some("Document Title".to_string()));
    }

    #[test]
    fn title_from_pages_skips_short_lines() {
        let pages = vec!["a\nb\nc\nA real title here".to_string()];
        let title = extract_title_from_pages(&pages, "my_doc.pdf");
        assert_eq!(title, Some("A real title here".to_string()));
    }

    #[test]
    fn title_from_pages_falls_back_to_file_stem() {
        let pages = vec!["a\nb\nc".to_string()];
        let title = extract_title_from_pages(&pages, "some_document.pdf");
        assert_eq!(title, Some("some_document".to_string()));
    }

    #[test]
    fn title_from_pages_empty_pages_falls_back_to_file_stem() {
        let title = extract_title_from_pages(&[], "readme.pdf");
        assert_eq!(title, Some("readme".to_string()));
    }

    // ── extract_title_from_sections ──────────────────────────────────────────

    #[test]
    fn title_from_sections_returns_first_heading() {
        let sections = vec![PdfSection {
            heading: Some("My Document Title".to_string()),
            heading_level: 1,
            content: "content".to_string(),
        }];
        let title = extract_title_from_sections(&sections, "doc.pdf");
        assert_eq!(title, Some("My Document Title".to_string()));
    }

    #[test]
    fn title_from_sections_falls_back_to_content_line() {
        let sections = vec![PdfSection {
            heading: None,
            heading_level: 0,
            content: "A meaningful first line\nmore text".to_string(),
        }];
        let title = extract_title_from_sections(&sections, "doc.pdf");
        assert_eq!(title, Some("A meaningful first line".to_string()));
    }

    #[test]
    fn title_from_sections_empty_falls_back_to_file_stem() {
        let title = extract_title_from_sections(&[], "my_file.pdf");
        assert_eq!(title, Some("my_file".to_string()));
    }

    // ── split_long_text ───────────────────────────────────────────────────────

    #[test]
    fn split_short_input_unchanged() {
        let input = "short text";
        let segs = split_long_text(input);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0], input);
    }

    #[test]
    fn split_long_text_at_paragraphs() {
        let para_a = "A".repeat(MAX_CHUNK_CHARS / 2 + 10);
        let para_b = "B".repeat(MAX_CHUNK_CHARS / 2 + 10);
        let input = format!("{para_a}\n\n{para_b}");
        let segs = split_long_text(&input);
        assert!(
            segs.len() >= 2,
            "long text must be split into at least 2 segments"
        );
    }

    #[test]
    fn split_long_text_single_paragraph_no_newlines_is_split() {
        // A single paragraph with no \n\n that exceeds MAX_CHUNK_CHARS.
        // Words are space-separated so word-boundary splitting can trigger.
        let words: Vec<String> = (0..400).map(|i| format!("word{i}")).collect();
        let input = words.join(" ");
        assert!(
            input.len() > MAX_CHUNK_CHARS,
            "input must exceed MAX_CHUNK_CHARS for this test to be meaningful"
        );
        let segs = split_long_text(&input);
        assert!(
            segs.len() >= 2,
            "single oversized paragraph must be split into multiple segments"
        );
        for seg in &segs {
            assert!(
                seg.len() <= MAX_CHUNK_CHARS,
                "every segment must be within MAX_CHUNK_CHARS, got {}",
                seg.len()
            );
        }
    }

    #[test]
    fn split_at_word_boundaries_produces_bounded_pieces() {
        let words: Vec<String> = (0..300).map(|i| format!("word{i}")).collect();
        let input = words.join(" ");
        assert!(input.len() > MAX_CHUNK_CHARS);
        let pieces = split_at_word_boundaries(&input);
        assert!(pieces.len() >= 2, "must split into multiple pieces");
        for piece in &pieces {
            assert!(
                piece.len() <= MAX_CHUNK_CHARS,
                "piece length {} exceeds MAX_CHUNK_CHARS",
                piece.len()
            );
        }
    }

    #[test]
    fn quantize_large_font_clamped_to_u16_max() {
        // Font sizes far above any real PDF (> ~6553pt) clamp to u16::MAX.
        // Verify the function doesn't panic and returns a valid key.
        let key = FontSizeHistogram::quantize(10_000.0);
        assert_eq!(key, u16::MAX, "extreme font size must clamp to u16::MAX");
    }

    #[test]
    fn split_at_word_boundaries_handles_unicode_without_panic() {
        // Each Chinese character is 3 UTF-8 bytes; slicing at a raw byte
        // offset of MAX_CHUNK_CHARS would land mid-codepoint for some offsets.
        // The char-count-based implementation must not panic.
        let cjk_char = '中'; // 3 bytes in UTF-8
        let long_cjk: String = std::iter::repeat(cjk_char)
            .take(MAX_CHUNK_CHARS + 50)
            .collect();
        let pieces = split_at_word_boundaries(&long_cjk);
        assert!(
            !pieces.is_empty(),
            "unicode text must produce at least one piece"
        );
        // Verify every piece is valid UTF-8 (i.e., no mid-codepoint slice).
        for piece in &pieces {
            assert!(std::str::from_utf8(piece.as_bytes()).is_ok());
        }
    }

    // ── Unit 4: PageTextAccumulator ───────────────────────────────────────────

    #[test]
    fn page_text_accumulator_empty_produces_empty_pages() {
        let acc = PageTextAccumulator::new();
        let pages = acc.finish();
        assert!(
            pages.is_empty(),
            "new accumulator must produce no pages before any output"
        );
    }

    #[test]
    fn page_text_accumulator_single_page_with_text() {
        let mb = default_media_box();
        let mut acc = PageTextAccumulator::new();
        acc.begin_page(1, &mb, None).expect("begin_page ok");
        acc.begin_word().expect("begin_word ok");
        acc.output_character(&make_trm(1.0, 0.0, 700.0), 0.1, 0.0, 10.0, "H")
            .expect("output_character ok");
        acc.output_character(&make_trm(1.0, 0.1, 700.0), 0.1, 0.0, 10.0, "i")
            .expect("output_character ok");
        acc.end_page().expect("end_page ok");

        let pages = acc.finish();
        assert_eq!(pages.len(), 1, "one end_page must produce one page");
        assert!(
            pages[0].contains("Hi"),
            "accumulated characters must appear in page text"
        );
    }

    #[test]
    fn page_text_accumulator_word_separator_inserted() {
        let mb = default_media_box();
        let mut acc = PageTextAccumulator::new();
        acc.begin_page(1, &mb, None).expect("ok");
        // First word
        acc.begin_word().expect("ok");
        acc.output_character(&make_trm(1.0, 0.0, 700.0), 0.1, 0.0, 10.0, "A")
            .expect("ok");
        acc.end_word().expect("ok");
        // Second word — begin_word fires before characters
        acc.begin_word().expect("ok");
        acc.output_character(&make_trm(1.0, 1.0, 700.0), 0.1, 0.0, 10.0, "B")
            .expect("ok");
        acc.end_page().expect("ok");

        let pages = acc.finish();
        assert_eq!(pages.len(), 1);
        assert!(
            pages[0].contains("A B"),
            "word separator must be inserted between words: got {:?}",
            pages[0]
        );
    }

    #[test]
    fn page_text_accumulator_newline_at_end_line() {
        let mb = default_media_box();
        let mut acc = PageTextAccumulator::new();
        acc.begin_page(1, &mb, None).expect("ok");
        acc.begin_word().expect("ok");
        acc.output_character(&make_trm(1.0, 0.0, 700.0), 0.1, 0.0, 10.0, "X")
            .expect("ok");
        acc.end_line().expect("ok");
        acc.end_page().expect("ok");

        let pages = acc.finish();
        assert_eq!(pages.len(), 1);
        assert!(
            pages[0].contains('\n'),
            "end_line must produce a newline character"
        );
    }

    #[test]
    fn page_text_accumulator_two_pages() {
        let mb = default_media_box();
        let mut acc = PageTextAccumulator::new();
        // Page 1
        acc.begin_page(1, &mb, None).expect("ok");
        acc.begin_word().expect("ok");
        acc.output_character(&make_trm(1.0, 0.0, 700.0), 0.1, 0.0, 10.0, "P")
            .expect("ok");
        acc.end_page().expect("ok");
        // Page 2
        acc.begin_page(2, &mb, None).expect("ok");
        acc.begin_word().expect("ok");
        acc.output_character(&make_trm(1.0, 0.0, 700.0), 0.1, 0.0, 10.0, "Q")
            .expect("ok");
        acc.end_page().expect("ok");

        let pages = acc.finish();
        assert_eq!(pages.len(), 2, "two end_page calls must produce two pages");
        assert!(pages[0].contains('P'));
        assert!(pages[1].contains('Q'));
    }

    // ── HISTOGRAM_SAMPLE_PAGES constant ──────────────────────────────────────

    #[test]
    fn histogram_sample_pages_constant_is_30() {
        assert_eq!(
            HISTOGRAM_SAMPLE_PAGES, 30,
            "HISTOGRAM_SAMPLE_PAGES must be 30"
        );
    }

    #[test]
    fn histogram_stops_counting_after_sample_limit() {
        let mut hist = FontSizeHistogram::new();
        let mb = default_media_box();
        let trm = make_trm(1.0, 0.0, 0.0);

        // Emit one character on each of the first 30 pages — all should count.
        for page in 1..=HISTOGRAM_SAMPLE_PAGES {
            hist.begin_page(page, &mb, None).expect("begin_page ok");
            hist.output_character(&trm, 0.1, 0.0, 10.0, "x")
                .expect("ok");
        }
        let count_at_30 = hist.counts.values().sum::<usize>();

        // Emit characters on page 31 — should NOT count.
        hist.begin_page(HISTOGRAM_SAMPLE_PAGES + 1, &mb, None)
            .expect("begin_page ok");
        for _ in 0..10 {
            hist.output_character(&trm, 0.1, 0.0, 10.0, "z")
                .expect("ok");
        }
        let count_after_limit = hist.counts.values().sum::<usize>();

        assert_eq!(
            count_at_30, count_after_limit,
            "characters on pages beyond HISTOGRAM_SAMPLE_PAGES must not be counted"
        );
    }

    #[test]
    fn page_text_accumulator_preserves_page_count() {
        let mb = default_media_box();
        let mut acc = PageTextAccumulator::new();
        let n: u32 = 5;
        for page_num in 1..=n {
            acc.begin_page(page_num, &mb, None).expect("begin_page ok");
            acc.begin_word().expect("ok");
            acc.output_character(&make_trm(1.0, 0.0, 700.0), 0.1, 0.0, 10.0, "X")
                .expect("ok");
            acc.end_page().expect("end_page ok");
        }
        let pages = acc.finish();
        assert_eq!(
            pages.len(),
            n as usize,
            "finish() must return exactly one entry per end_page call"
        );
    }

    // ── Unit 5: HeadingFontDetector ───────────────────────────────────────────

    #[test]
    fn heading_font_detector_new_has_not_found() {
        let det = HeadingFontDetector::new(12.0);
        assert!(
            !det.found_heading(),
            "fresh HeadingFontDetector must not report found_heading"
        );
    }

    #[test]
    fn heading_font_detector_detects_heading_sized_character() {
        let mb = default_media_box();
        // H2 threshold at body_font_size 10pt: 10.0 * H2_RATIO = 13.0
        let threshold = 10.0 * H2_RATIO;
        let mut det = HeadingFontDetector::new(threshold);
        det.begin_page(1, &mb, None).expect("begin_page ok");
        // 14pt rendered size (scale 1.0 × 14pt) ≥ 13pt threshold
        det.output_character(&make_trm(1.0, 0.0, 700.0), 0.1, 0.0, 14.0, "H")
            .expect("output_character ok");
        det.end_page().expect("end_page ok");
        assert!(
            det.found_heading(),
            "detector must report found_heading after a qualifying character"
        );
    }

    #[test]
    fn heading_font_detector_ignores_body_sized_characters() {
        let mb = default_media_box();
        let threshold = 10.0 * H2_RATIO;
        let mut det = HeadingFontDetector::new(threshold);
        det.begin_page(1, &mb, None).expect("begin_page ok");
        // 10pt rendered size < 13pt threshold → must not set found
        det.output_character(&make_trm(1.0, 0.0, 700.0), 0.1, 0.0, 10.0, "x")
            .expect("output_character ok");
        det.end_page().expect("end_page ok");
        assert!(
            !det.found_heading(),
            "detector must not report found_heading for body-sized characters"
        );
    }

    // ── LARGE_PDF_THRESHOLD constant ─────────────────────────────────────────

    #[test]
    fn large_pdf_threshold_is_20_mib() {
        assert_eq!(
            LARGE_PDF_THRESHOLD,
            20 * 1_024 * 1_024,
            "LARGE_PDF_THRESHOLD must be exactly 20 MiB"
        );
    }

    fn pdf_panic_guard_test_lock() -> &'static std::sync::Mutex<()> {
        static TEST_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
        TEST_LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    // ── PdfExtractBackend ─────────────────────────────────────────────────────

    #[test]
    fn pdf_extract_backend_returns_error_on_empty_bytes() {
        let _test_lock = pdf_panic_guard_test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Empty bytes are not a valid PDF — parse must return an error, not panic.
        let result = PdfExtractBackend::parse(&[], "empty.pdf");
        assert!(
            result.is_err(),
            "PdfExtractBackend::parse must fail on empty bytes"
        );
    }

    #[test]
    fn pdf_panic_guard_returns_inner_ok() {
        let _test_lock = pdf_panic_guard_test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let result =
            with_pdf_panic_guard("ok.pdf", "test operation", || Ok::<usize, GraphtorError>(7));
        assert!(matches!(result, Ok(7)));
    }

    #[test]
    fn pdf_panic_guard_preserves_inner_error() {
        let _test_lock = pdf_panic_guard_test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let result = with_pdf_panic_guard("error.pdf", "test operation", || {
            Err::<(), GraphtorError>(GraphtorError::Parse {
                message: "pdf load failed".to_string(),
                path: Some("error.pdf".into()),
            })
        });

        assert!(matches!(
            result,
            Err(GraphtorError::Parse { message, .. }) if message == "pdf load failed"
        ));
    }

    #[test]
    fn pdf_panic_guard_converts_str_panic_to_parse_error() {
        let _test_lock = pdf_panic_guard_test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let result = with_pdf_panic_guard("panic.pdf", "test operation", || {
            panic!("dependency blew up");
            #[allow(unreachable_code)]
            Ok::<(), GraphtorError>(())
        });

        assert!(matches!(
            result,
            Err(GraphtorError::Parse { message, .. })
                if message.contains("pdf-extract panicked during test operation: dependency blew up")
        ));
    }

    #[test]
    fn pdf_panic_guard_converts_string_panic_to_parse_error() {
        let _test_lock = pdf_panic_guard_test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let result = with_pdf_panic_guard("panic.pdf", "test operation", || {
            std::panic::panic_any(String::from("owned panic message"));
            #[allow(unreachable_code)]
            Ok::<(), GraphtorError>(())
        });

        assert!(matches!(
            result,
            Err(GraphtorError::Parse { message, .. }) if message.contains("owned panic message")
        ));
    }

    #[test]
    fn pdf_panic_guard_only_silences_guarded_thread() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };

        let _test_lock = pdf_panic_guard_test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let hook_calls = Arc::new(AtomicUsize::new(0));
        let hook_calls_for_hook = Arc::clone(&hook_calls);
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |_| {
            hook_calls_for_hook.fetch_add(1, Ordering::SeqCst);
        }));

        let result = with_pdf_panic_guard("panic.pdf", "test operation", || {
            let child = std::thread::spawn(|| {
                panic!("other thread should still hit the hook");
            });
            let _ = child.join();
            panic!("guarded thread should stay silent");
            #[allow(unreachable_code)]
            Ok::<(), GraphtorError>(())
        });

        assert!(
            result.is_err(),
            "panic guard must still convert the panic to an error"
        );
        assert_eq!(
            hook_calls.load(Ordering::SeqCst),
            1,
            "only the unrelated thread panic should reach the custom hook"
        );

        let _ = std::panic::catch_unwind(|| panic!("hook restored"));
        assert_eq!(
            hook_calls.load(Ordering::SeqCst),
            2,
            "custom hook must still run after the pdf panic guard completes"
        );

        std::panic::set_hook(previous_hook);
    }

    #[test]
    fn panic_payload_message_falls_back_for_unknown_payloads() {
        let payload = Box::new(7_u8) as Box<dyn std::any::Any + Send>;
        assert_eq!(
            panic_payload_message(payload.as_ref()),
            "unknown panic payload"
        );
    }

    // ── PdfiumBackend tests ───────────────────────────────────────────────────

    use super::{PdfiumBackend, PdfiumBindError};

    #[test]
    fn pdfium_bind_error_not_available_display() {
        let err = PdfiumBindError::NotAvailable("library not found".to_string());
        assert_eq!(err.to_string(), "pdfium not available: library not found");
    }

    #[test]
    fn pdfium_bind_error_extraction_failed_display() {
        let err =
            PdfiumBindError::ExtractionFailed("page 0 text extraction failed: corrupt".to_string());
        assert_eq!(
            err.to_string(),
            "pdfium extraction failed: page 0 text extraction failed: corrupt"
        );
    }

    #[test]
    fn pdfium_load_returns_not_available_without_panic() {
        // In CI or developer environments without the PDFium DLL installed,
        // `load_pdfium()` must return `NotAvailable`, never panic.
        // We do NOT mutate env vars here to avoid parallel test interference.
        // The test validates the no-panic invariant regardless of DLL presence.
        let result = PdfiumBackend::load_pdfium();

        // If the DLL happens to be on the system path, load succeeds — that's fine.
        // The important invariant is no panic. If it fails, it must be NotAvailable.
        if let Err(e) = result {
            assert!(
                matches!(e, PdfiumBindError::NotAvailable(_)),
                "expected NotAvailable, got: {e}"
            );
        }
    }

    #[test]
    fn pdfium_try_parse_without_dll_returns_not_available() {
        // Validates that try_parse returns the correct error variant
        // without panicking. We do NOT mutate env vars to avoid parallel
        // test interference — the test handles both DLL-present and
        // DLL-absent scenarios gracefully.
        let result = PdfiumBackend::try_parse(b"%PDF-1.4 fake", "test.pdf");

        // If the DLL is available on the system path, the result may differ.
        // The test verifies no panic and correct error variant when unavailable.
        if let Err(e) = result {
            assert!(
                matches!(
                    e,
                    PdfiumBindError::NotAvailable(_) | PdfiumBindError::ExtractionFailed(_)
                ),
                "expected NotAvailable or ExtractionFailed, got: {e}"
            );
        }
    }

    #[test]
    fn parse_pdf_document_falls_back_for_large_input() {
        use super::parse_pdf_document;
        let _test_lock = pdf_panic_guard_test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A large (>20 MiB) byte slice of non-PDF junk should attempt
        // pdfium first, then fall back to pdf-extract, which also fails
        // on invalid content. The important thing is no panic.
        let large_junk = vec![0u8; LARGE_PDF_THRESHOLD + 1];
        let result = parse_pdf_document(&large_junk, "junk.pdf");
        // Both backends should fail on non-PDF bytes, returning an error.
        assert!(result.is_err(), "non-PDF bytes must produce an error");
    }

    #[test]
    fn parse_pdf_document_uses_pdf_extract_for_small_files() {
        use super::parse_pdf_document;
        let _test_lock = pdf_panic_guard_test_lock()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A small input (below threshold) goes directly to pdf-extract,
        // skipping the pdfium path. Non-PDF bytes produce an error.
        let small_junk = vec![0u8; LARGE_PDF_THRESHOLD - 1];
        let result = parse_pdf_document(&small_junk, "small_junk.pdf");
        assert!(result.is_err(), "non-PDF bytes must produce an error");
    }

    // ── HeadingAwareOutput: page-boundary state persistence ──────────────────

    /// Verify that heading state accumulated on page 1 persists into the
    /// body text emitted on page 2 when `HeadingAwareOutput` is fed via the
    /// `output_doc_page` loop pattern (simulated here with manual begin/end
    /// page calls).
    ///
    /// Acceptance criteria for task `027.004-T`:
    /// - body chunk on page 2 inherits the H1 heading from page 1
    /// - no chunk has a `["Page N"]` heading hierarchy
    #[test]
    fn heading_aware_heading_state_persists_across_page_boundaries() {
        let body_font_size = 10.0_f64;
        let h1_font_size = body_font_size * H1_RATIO + 1.0; // clearly H1
        let mb = default_media_box();

        let mut output = HeadingAwareOutput::new(body_font_size);

        // --- Page 1: emit an H1 heading ----------------------------------------
        output.begin_page(1, &mb, None).expect("begin_page 1 ok");
        // Emit the heading text "Introduction" at H1 size.
        emit_text(&mut output, "Introduction", h1_font_size, 72.0, 720.0);
        output.end_page().expect("end_page 1 ok");

        // --- Page 2: emit body text beneath the H1 heading ---------------------
        output.begin_page(2, &mb, None).expect("begin_page 2 ok");
        // Emit body text at normal body size (different y → new line).
        emit_text(
            &mut output,
            "Body content on page two.",
            body_font_size,
            72.0,
            600.0,
        );
        output.end_page().expect("end_page 2 ok");

        let sections = output.finish();

        // Must have produced at least one section.
        assert!(
            !sections.is_empty(),
            "HeadingAwareOutput must produce sections when content is emitted"
        );

        // The heading section ("Introduction") must be present.
        let has_intro = sections
            .iter()
            .any(|s| s.heading.as_deref() == Some("Introduction"));
        assert!(
            has_intro,
            "sections must contain the H1 heading 'Introduction' from page 1"
        );

        // Convert to chunks to verify hierarchies.
        let chunks = sections_to_chunks(sections, "test.pdf")
            .expect("sections_to_chunks must not fail on valid sections");

        // At least one chunk must carry the 'Introduction' heading in its hierarchy.
        let body_with_intro = chunks
            .iter()
            .any(|c| c.heading_hierarchy.iter().any(|h| h == "Introduction"));
        assert!(
            body_with_intro,
            "a chunk must carry 'Introduction' in heading_hierarchy, proving \
             heading state persists from page 1 to page 2"
        );

        // No chunk must have a "Page N" heading hierarchy entry — the
        // heading-aware path must never produce page-based hierarchies.
        let has_page_marker = chunks.iter().any(|c| {
            c.heading_hierarchy
                .iter()
                .any(|h| h.starts_with("Page ") && h[5..].parse::<u32>().is_ok())
        });
        assert!(
            !has_page_marker,
            "heading-aware chunks must not contain 'Page N' hierarchy entries"
        );
    }
}
