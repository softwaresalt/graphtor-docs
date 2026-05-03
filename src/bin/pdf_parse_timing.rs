use std::fs;
use std::time::Instant;

fn main() {
    let pdf_path = "./tmp/azure-cosmos-db.pdf";

    if !std::path::Path::new(pdf_path).exists() {
        eprintln!("PDF not found: {pdf_path}");
        std::process::exit(1);
    }

    let file_size = fs::metadata(pdf_path).map(|m| m.len()).unwrap_or(0);

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
                #[allow(clippy::cast_precision_loss)]
                let avg_ms = (elapsed.as_secs_f64() * 1000.0) / doc.chunks.len() as f64;
                println!("\nAverage time per chunk: {avg_ms:.4} ms");
            }
            Err(e) => eprintln!("Parse error: {e}"),
        }
    } else {
        eprintln!("Could not read file");
    }
}
