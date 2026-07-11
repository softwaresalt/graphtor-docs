//! User-friendly fatal-error rendering for the CLI.
//!
//! Turns an [`anyhow::Error`] into a readable, actionable console block:
//!
//! ```text
//! error: failed to build acquisition plan
//!
//! Caused by:
//!   path violation: 'D:\docs' is outside the workspace root 'C:\Tools'
//!
//! hint: every source path in .graphtor/config/sources.yaml must resolve inside
//!       the current working directory ('C:\Tools'). ...
//! ```
//!
//! The full `anyhow` cause chain is surfaced (not just the top-level context),
//! known [`GraphtorError`] variants are rendered without their internal
//! category tags and paired with an actionable hint, and the human block is
//! coloured via `anstyle`. Colour is applied only when stderr is a terminal:
//! [`eprint_fatal`] writes through an [`anstream::AutoStream`] which strips ANSI
//! escapes when the destination is not a TTY or when `NO_COLOR` is set.
//!
//! For `--json` mode, [`fatal_error_data`] produces a structured payload
//! (`category`, `cause_chain`, optional `hint`) for the JSON-RPC error envelope.

use std::fmt::Write as _;
use std::io::Write as _;

use graphtor_core::GraphtorError;

/// Print a friendly, coloured fatal-error block to stderr.
///
/// Colour is emitted only when stderr is a terminal (auto-detected, and
/// honouring `NO_COLOR`); otherwise the ANSI escapes are stripped.
pub(crate) fn eprint_fatal(err: &anyhow::Error) {
    let rendered = render_fatal(err, true);
    let mut stream = anstream::AutoStream::auto(std::io::stderr());
    // A failure to write the error report to stderr is itself unrecoverable
    // and has nowhere left to be reported, so the result is intentionally
    // ignored.
    let _ = writeln!(stream, "{rendered}");
}

/// Build the structured `data` payload for a JSON-RPC error envelope.
///
/// Contains the error `category`, the full `cause_chain`, and an optional
/// actionable `hint`.
#[must_use]
pub(crate) fn fatal_error_data(err: &anyhow::Error) -> serde_json::Value {
    let mut chain: Vec<String> = vec![friendly_head(err)];
    chain.extend(cause_lines(err));

    let mut map = serde_json::Map::new();
    map.insert(
        "category".to_string(),
        serde_json::Value::from(category(err)),
    );
    map.insert("cause_chain".to_string(), serde_json::Value::from(chain));
    if let Some(hint) = actionable_hint(err) {
        map.insert("hint".to_string(), serde_json::Value::from(hint));
    }
    serde_json::Value::Object(map)
}

/// Render the full human-facing error block.
///
/// When `color` is `true`, section labels are wrapped in ANSI style escapes.
#[must_use]
fn render_fatal(err: &anyhow::Error, color: bool) -> String {
    let mut out = String::new();
    out.push_str(&paint("error: ", error_style(), color));
    out.push_str(&friendly_head(err));

    let causes = cause_lines(err);
    if !causes.is_empty() {
        out.push_str("\n\n");
        out.push_str(&paint("Caused by:", header_style(), color));
        for cause in &causes {
            out.push_str("\n  ");
            out.push_str(cause);
        }
    }

    if let Some(hint) = actionable_hint(err) {
        out.push_str("\n\n");
        out.push_str(&paint("hint: ", hint_style(), color));
        out.push_str(&hint);
    }

    out
}

/// Friendly, de-duplicated cause lines (the chain below the top-level context).
fn cause_lines(err: &anyhow::Error) -> Vec<String> {
    let head = err.to_string();
    let mut lines: Vec<String> = Vec::new();
    for cause in err.chain().skip(1) {
        let line = friendly_cause(cause);
        // Skip lines that merely repeat the head or the previous cause.
        if line == head || lines.last().map(String::as_str) == Some(line.as_str()) {
            continue;
        }
        lines.push(line);
    }
    lines
}

/// Render a single chain element, preferring a tag-free [`GraphtorError`] form.
fn friendly_cause(cause: &(dyn std::error::Error + 'static)) -> String {
    cause
        .downcast_ref::<GraphtorError>()
        .map_or_else(|| cause.to_string(), friendly_graphtor)
}

/// Friendly rendering of the top-level error.
///
/// Tag-free when the error reaches `main` as a bare [`GraphtorError`] (no
/// `anyhow` context wrapper); otherwise the top-level context string as-is.
fn friendly_head(err: &anyhow::Error) -> String {
    err.chain()
        .next()
        .map_or_else(|| err.to_string(), friendly_cause)
}

/// The friendly, tag-free headline for a fatal error.
///
/// Shared by the human block, the JSON envelope `message`, and
/// `cause_chain[0]` so every surface presents the same tag-free headline —
/// including bare [`GraphtorError`]s that reach `main` without a context wrapper.
#[must_use]
pub(crate) fn fatal_headline(err: &anyhow::Error) -> String {
    friendly_head(err)
}

/// Human-friendly, category-tag-free rendering of a [`GraphtorError`].
fn friendly_graphtor(err: &GraphtorError) -> String {
    match err {
        GraphtorError::PathViolation {
            attempted,
            allowed_root,
        } => format!(
            "path violation: '{}' is outside the workspace root '{}'",
            attempted.display(),
            allowed_root.display()
        ),
        GraphtorError::DatabaseLocked {
            db_name,
            holder_pid,
        } => holder_pid.map_or_else(
            || format!("database '{db_name}' is locked"),
            |pid| format!("database '{db_name}' is locked by process {pid}"),
        ),
        other => strip_category_tag(&other.to_string()),
    }
}

/// Strip a leading `"[category] "` tag from a [`GraphtorError`] Display string.
fn strip_category_tag(msg: &str) -> String {
    if let Some(rest) = msg.strip_prefix('[') {
        if let Some(idx) = rest.find("] ") {
            return rest[idx + 2..].to_string();
        }
    }
    msg.to_string()
}

/// Derive an actionable hint from the first [`GraphtorError`] in the chain.
fn actionable_hint(err: &anyhow::Error) -> Option<String> {
    let graphtor = err
        .chain()
        .find_map(|cause| cause.downcast_ref::<GraphtorError>())?;
    match graphtor {
        GraphtorError::PathViolation { allowed_root, .. } => Some(format!(
            "every source path in .graphtor/config/sources.yaml must resolve inside the \
             current working directory ('{}'). Run graphtor-docs from your project root, \
             or move the source under that directory.",
            allowed_root.display()
        )),
        GraphtorError::DatabaseLocked { .. } => Some(
            "another graphtor-docs process may be running. Wait for it to finish, or remove \
             the stale lock file if you are sure no process is active."
                .to_string(),
        ),
        GraphtorError::Io(io) => match io.kind() {
            std::io::ErrorKind::NotFound => {
                Some("check that the path exists and is spelled correctly.".to_string())
            }
            std::io::ErrorKind::PermissionDenied => {
                Some("check filesystem permissions for the target path.".to_string())
            }
            _ => None,
        },
        _ => None,
    }
}

/// The stable category slug for the first [`GraphtorError`] in the chain.
fn category(err: &anyhow::Error) -> &'static str {
    err.chain()
        .find_map(|cause| cause.downcast_ref::<GraphtorError>())
        .map_or("error", graphtor_category)
}

/// Map a [`GraphtorError`] variant to its stable category slug.
fn graphtor_category(err: &GraphtorError) -> &'static str {
    match err {
        GraphtorError::Config { .. } => "config",
        GraphtorError::Database { .. } => "database",
        GraphtorError::DatabaseLocked { .. } => "database_locked",
        GraphtorError::Pipeline { .. } => "pipeline",
        GraphtorError::Parse { .. } => "parse",
        GraphtorError::Embed { .. } => "embed",
        GraphtorError::PathViolation { .. } => "path_violation",
        GraphtorError::Sync { .. } => "sync",
        GraphtorError::Contract { .. } => "contract",
        GraphtorError::Io(_) => "io",
        _ => "error",
    }
}

/// Wrap `text` in ANSI style escapes when `color` is enabled.
fn paint(text: &str, style: anstyle::Style, color: bool) -> String {
    if color {
        format!("{}{text}{}", style.render(), anstyle::Reset.render())
    } else {
        text.to_string()
    }
}

/// Style for the `error:` label — bold red.
fn error_style() -> anstyle::Style {
    anstyle::Style::new()
        .fg_color(Some(anstyle::AnsiColor::Red.into()))
        .bold()
}

/// Style for the `Caused by:` header — bold.
fn header_style() -> anstyle::Style {
    anstyle::Style::new().bold()
}

/// Style for the `hint:` label — bold cyan.
fn hint_style() -> anstyle::Style {
    anstyle::Style::new()
        .fg_color(Some(anstyle::AnsiColor::Cyan.into()))
        .bold()
}

/// Print a prominent, coloured warning that embeddings were skipped during sync.
///
/// Written to stderr through an [`anstream::AutoStream`], so ANSI escapes are
/// stripped on non-terminals and when `NO_COLOR` is set.
pub(crate) fn eprint_embeddings_skipped_warning(chunks_created: usize) {
    let rendered = format_embeddings_skipped_warning(chunks_created, true);
    let mut stream = anstream::AutoStream::auto(std::io::stderr());
    let _ = writeln!(stream, "{rendered}");
}

/// Build the "embeddings skipped" warning block.
///
/// When `color` is `true`, the `warning:` label is wrapped in ANSI escapes.
#[must_use]
fn format_embeddings_skipped_warning(chunks_created: usize, color: bool) -> String {
    let mut out = String::new();
    out.push_str(&paint("warning: ", warning_style(), color));
    out.push_str("embeddings were skipped — the embedding model was unavailable.");
    let _ = write!(
        out,
        "\n  {chunks_created} chunk(s) were processed this run without generating embeddings; \
         those chunks will not be found by semantic search."
    );
    out.push_str(
        "\n  set GRAPHTOR_EMBED_MODEL_DIR to a local all-MiniLM-L6-v2 directory (or fix the \
         Hugging Face cache) and re-run `graphtor-docs sync --full` to embed them.",
    );
    out
}

/// Style for the `warning:` label — bold red.
fn warning_style() -> anstyle::Style {
    anstyle::Style::new()
        .fg_color(Some(anstyle::AnsiColor::Red.into()))
        .bold()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn path_violation_error() -> anyhow::Error {
        let g = GraphtorError::PathViolation {
            attempted: PathBuf::from(r"D:\docs"),
            allowed_root: PathBuf::from(r"C:\Tools"),
        };
        anyhow::Error::new(g).context("failed to build acquisition plan")
    }

    #[test]
    fn plain_render_shows_top_context_causes_and_hint() {
        let err = path_violation_error();
        let out = render_fatal(&err, false);

        assert!(
            out.starts_with("error: failed to build acquisition plan"),
            "should lead with the top-level context: {out}"
        );
        assert!(
            out.contains("Caused by:"),
            "should have a causes section: {out}"
        );
        assert!(
            out.contains("path violation: 'D:\\docs' is outside the workspace root 'C:\\Tools'"),
            "cause should be the tag-free friendly form: {out}"
        );
        assert!(
            out.contains("hint:"),
            "should include an actionable hint: {out}"
        );
    }

    #[test]
    fn plain_render_has_no_ansi_escapes() {
        let out = render_fatal(&path_violation_error(), false);
        assert!(
            !out.contains('\u{1b}'),
            "plain render must not embed ANSI escapes: {out:?}"
        );
    }

    #[test]
    fn colored_render_embeds_ansi_escapes() {
        let out = render_fatal(&path_violation_error(), true);
        assert!(
            out.contains('\u{1b}'),
            "coloured render must embed ANSI escapes"
        );
        // Content is still present alongside the styling.
        assert!(out.contains("failed to build acquisition plan"));
        assert!(out.contains("path violation:"));
    }

    #[test]
    fn plain_error_without_source_has_no_causes_or_hint() {
        let err = anyhow::anyhow!("something broke");
        let out = render_fatal(&err, false);
        assert_eq!(out, "error: something broke");
        assert!(actionable_hint(&err).is_none());
    }

    #[test]
    fn database_locked_hint_and_friendly_message() {
        let g = GraphtorError::DatabaseLocked {
            db_name: "graph.db".to_string(),
            holder_pid: Some(4321),
        };
        let err = anyhow::Error::new(g).context("failed to open database");
        let out = render_fatal(&err, false);
        assert!(out.contains("database 'graph.db' is locked by process 4321"));
        assert!(out.contains("hint:"));
        assert!(
            !out.contains("[database_locked]"),
            "tag must be stripped: {out}"
        );
    }

    #[test]
    fn unknown_variant_falls_back_to_stripped_tag_without_hint() {
        let g = GraphtorError::Config {
            message: "unexpected token".to_string(),
            field: Some("sources".to_string()),
        };
        let err = anyhow::Error::new(g).context("failed to load config");
        let out = render_fatal(&err, false);
        assert!(out.contains("unexpected token"), "message preserved: {out}");
        assert!(!out.contains("[config]"), "category tag stripped: {out}");
        assert!(
            !out.contains("hint:"),
            "no hint for generic config error: {out}"
        );
    }

    #[test]
    fn strip_category_tag_removes_leading_bracket_tag() {
        assert_eq!(strip_category_tag("[config] bad value"), "bad value");
        assert_eq!(strip_category_tag("no tag here"), "no tag here");
        assert_eq!(strip_category_tag("[io] disk full"), "disk full");
    }

    #[test]
    fn cause_lines_deduplicates_repeated_context() {
        // Same string used for context and wrapped error must not double-print.
        let err = anyhow::anyhow!("boom").context("boom");
        assert!(cause_lines(&err).is_empty());
    }

    #[test]
    fn bare_top_level_graphtor_error_head_is_friendly() {
        // A GraphtorError reaching main without a `.context(...)` wrapper must
        // still render tag-free on the headline (P3 review finding).
        let err = anyhow::Error::new(GraphtorError::PathViolation {
            attempted: PathBuf::from(r"D:\docs"),
            allowed_root: PathBuf::from(r"C:\Tools"),
        });
        let out = render_fatal(&err, false);
        assert!(
            out.contains(
                "error: path violation: 'D:\\docs' is outside the workspace root 'C:\\Tools'"
            ),
            "head must be the tag-free friendly form: {out}"
        );
        assert!(
            !out.contains("[path_violation]"),
            "category tag must be stripped from the head: {out}"
        );
        let data = fatal_error_data(&err);
        assert_eq!(
            data["cause_chain"][0],
            "path violation: 'D:\\docs' is outside the workspace root 'C:\\Tools'"
        );
    }

    #[test]
    fn fatal_headline_strips_tag_for_bare_error() {
        let err = anyhow::Error::new(GraphtorError::DatabaseLocked {
            db_name: "graph.db".to_string(),
            holder_pid: Some(7),
        });
        let head = fatal_headline(&err);
        assert!(
            head.contains("database 'graph.db' is locked by process 7"),
            "{head}"
        );
        assert!(
            !head.contains("[database_locked]"),
            "tag must be stripped: {head}"
        );
    }

    #[test]
    fn fatal_headline_preserves_context_wrapped_head() {
        let err = anyhow::Error::new(GraphtorError::PathViolation {
            attempted: PathBuf::from(r"D:\docs"),
            allowed_root: PathBuf::from(r"C:\Tools"),
        })
        .context("failed to build acquisition plan");
        assert_eq!(fatal_headline(&err), "failed to build acquisition plan");
    }

    #[test]
    fn embeddings_skipped_warning_plain_has_content_and_no_ansi() {
        let out = format_embeddings_skipped_warning(4082, false);
        assert!(out.contains("warning: embeddings were skipped"), "{out}");
        assert!(out.contains("4082 chunk"), "{out}");
        assert!(out.contains("without generating embeddings"), "{out}");
        assert!(out.contains("GRAPHTOR_EMBED_MODEL_DIR"), "{out}");
        assert!(out.contains("sync --full"), "{out}");
        // Must not overstate: chunks may retain vectors on incremental re-sync,
        // and semantic search fails (not "returns no results") without the model.
        assert!(
            !out.contains("will return no results"),
            "warning must not overstate semantic-search behaviour: {out}"
        );
        assert!(
            !out.contains("stored without vectors"),
            "warning must not claim all chunks lack vectors: {out}"
        );
        assert!(
            !out.contains('\u{1b}'),
            "plain warning must have no ANSI: {out:?}"
        );
    }

    #[test]
    fn embeddings_skipped_warning_colored_has_ansi() {
        let out = format_embeddings_skipped_warning(1, true);
        assert!(out.contains('\u{1b}'), "coloured warning must embed ANSI");
        assert!(out.contains("embeddings were skipped"));
    }

    #[test]
    fn fatal_error_data_has_category_chain_and_hint() {
        let data = fatal_error_data(&path_violation_error());
        assert_eq!(data["category"], "path_violation");
        let chain = data["cause_chain"].as_array().expect("cause_chain array");
        assert_eq!(
            chain.len(),
            2,
            "top context + one friendly cause: {chain:?}"
        );
        assert_eq!(chain[0], "failed to build acquisition plan");
        assert!(
            data.get("hint").is_some(),
            "hint present for path violation"
        );
    }

    #[test]
    fn fatal_error_data_omits_hint_for_plain_error() {
        let data = fatal_error_data(&anyhow::anyhow!("plain"));
        assert_eq!(data["category"], "error");
        assert!(data.get("hint").is_none(), "no hint for plain error");
        let chain = data["cause_chain"].as_array().expect("array");
        assert_eq!(chain.len(), 1);
    }
}
