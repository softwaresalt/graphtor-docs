//! Diagnostic binary to identify where time is spent in large PDF processing.
//!
//! Reports: load time, page count, and per-page extraction time for the first
//! N pages to identify scaling characteristics.

#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use std::fs;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let pdf_path = if args.len() > 1 {
        args[1].as_str()
    } else {
        "./tmp/azure-cosmos-db.pdf"
    };

    if !std::path::Path::new(pdf_path).exists() {
        eprintln!("PDF not found: {pdf_path}");
        std::process::exit(1);
    }

    let file_size = fs::metadata(pdf_path).map_or(0, |m| m.len());
    println!("=== PDF DIAGNOSTIC ===");
    println!("File: {pdf_path}");
    println!("Size: {} MB\n", file_size / (1024 * 1024));

    // Phase 1: Read file into memory
    let t0 = Instant::now();
    let bytes = fs::read(pdf_path).expect("read file");
    let read_time = t0.elapsed();
    println!(
        "[1] File read:     {:.3} s ({} bytes)",
        read_time.as_secs_f64(),
        bytes.len()
    );

    // Phase 2: Parse PDF structure (load_mem)
    let t1 = Instant::now();
    let doc = pdf_extract::Document::load_mem(&bytes).expect("load pdf");
    let load_time = t1.elapsed();
    let page_count = doc.get_pages().len();
    println!(
        "[2] PDF load_mem:  {:.3} s ({page_count} pages)",
        load_time.as_secs_f64()
    );

    // Phase 3: Extract first 10 pages one-by-one to measure per-page cost
    let sample_pages = 10.min(page_count as u32);
    println!("\n[3] Per-page extraction timing (first {sample_pages} pages):");

    let mut page_times = Vec::new();
    for page_num in 1..=sample_pages {
        let tp = Instant::now();
        let mut acc = SimplePageAcc::new();
        let _ = pdf_extract::output_doc_page(&doc, &mut acc, page_num);
        let pt = tp.elapsed();
        println!(
            "    Page {page_num:4}: {:.3} s ({} chars)",
            pt.as_secs_f64(),
            acc.char_count
        );
        page_times.push(pt.as_secs_f64());
    }

    let avg_page = page_times.iter().sum::<f64>() / page_times.len() as f64;
    let estimated_total = avg_page * page_count as f64;
    println!("\n[4] Estimated full extraction: {estimated_total:.1} s ({page_count} pages × {avg_page:.3} s/page)");

    // Phase 4: Try output_doc on the full document with a no-op dev that just counts
    println!("\n[5] Full output_doc (counting only)...");
    let t2 = Instant::now();
    let mut counter = CharCounter::new();
    let _ = pdf_extract::output_doc(&doc, &mut counter);
    let full_time = t2.elapsed();
    println!("    Time: {:.3} s", full_time.as_secs_f64());
    println!("    Total chars: {}", counter.char_count);
    println!("    Total pages processed: {}", counter.page_count);
}

/// Minimal `OutputDev` that only counts characters on one page.
struct SimplePageAcc {
    char_count: usize,
}

impl SimplePageAcc {
    fn new() -> Self {
        Self { char_count: 0 }
    }
}

impl pdf_extract::OutputDev for SimplePageAcc {
    fn begin_page(
        &mut self,
        _: u32,
        _: &pdf_extract::MediaBox,
        _: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }
    fn end_page(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }
    fn output_character(
        &mut self,
        _: &pdf_extract::Transform,
        _: f64,
        _: f64,
        _: f64,
        c: &str,
    ) -> Result<(), pdf_extract::OutputError> {
        self.char_count += c.len();
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

/// Minimal `OutputDev` that counts chars across all pages.
struct CharCounter {
    char_count: usize,
    page_count: u32,
}

impl CharCounter {
    fn new() -> Self {
        Self {
            char_count: 0,
            page_count: 0,
        }
    }
}

impl pdf_extract::OutputDev for CharCounter {
    fn begin_page(
        &mut self,
        _: u32,
        _: &pdf_extract::MediaBox,
        _: Option<(f64, f64, f64, f64)>,
    ) -> Result<(), pdf_extract::OutputError> {
        self.page_count += 1;
        Ok(())
    }
    fn end_page(&mut self) -> Result<(), pdf_extract::OutputError> {
        Ok(())
    }
    fn output_character(
        &mut self,
        _: &pdf_extract::Transform,
        _: f64,
        _: f64,
        _: f64,
        c: &str,
    ) -> Result<(), pdf_extract::OutputError> {
        self.char_count += c.len();
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
