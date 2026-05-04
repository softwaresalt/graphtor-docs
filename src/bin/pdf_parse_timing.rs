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
        eprintln!("Usage: pdf_parse_timing [path/to/file.pdf]");
        std::process::exit(1);
    }

    let file_size = fs::metadata(pdf_path).map_or(0, |m| m.len());

    println!("Testing PDF parsing: {pdf_path}");
    println!("File size: {} MB\n", file_size / (1024 * 1024));

    let start = Instant::now();

    // Parse the PDF
    if let Ok(bytes) = fs::read(pdf_path) {
        match graphtor_core::parse::parse_pdf_document(&bytes, pdf_path) {
            Ok(doc) => {
                let elapsed = start.elapsed();
                println!("=== PDF PARSING RESULTS ===");
                println!("Parse time: {:.2} seconds", elapsed.as_secs_f64());
                println!("Total chunks: {}", doc.chunks.len());
                println!("Title: {:?}", doc.title);
                if doc.chunks.is_empty() {
                    println!("\nNo chunks produced.");
                } else {
                    #[allow(clippy::cast_precision_loss)]
                    let avg_ms = (elapsed.as_secs_f64() * 1000.0) / doc.chunks.len() as f64;
                    println!("\nAverage time per chunk: {avg_ms:.4} ms");
                }
            }
            Err(e) => eprintln!("Parse error: {e}"),
        }
    } else {
        eprintln!("Could not read file");
    }
}
