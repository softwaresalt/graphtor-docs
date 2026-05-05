//! Integration tests: PDF document parsing.
//!
//! These tests verify the public contract of [`graphtor_core::parse::parse_pdf_document`]:
//! - Invalid or empty bytes produce a [`graphtor_core::error::GraphtorError::Parse`] error.

use graphtor_core::parse::parse_pdf_document;

// ── T015.002: error cases ────────────────────────────────────────────────────

#[test]
fn parse_pdf_empty_bytes_returns_error() {
    let result = parse_pdf_document(b"", "empty.pdf");
    assert!(
        result.is_err(),
        "empty byte slice must be rejected as an invalid PDF"
    );
}

#[test]
fn parse_pdf_invalid_bytes_returns_error() {
    let result = parse_pdf_document(b"this is not a pdf document at all", "invalid.pdf");
    assert!(result.is_err(), "non-PDF bytes must return a Parse error");
}

#[test]
fn parse_pdf_binary_garbage_returns_error() {
    let garbage: Vec<u8> = (0_u8..=255).collect();
    let result = parse_pdf_document(&garbage, "garbage.pdf");
    assert!(result.is_err(), "binary garbage must return a Parse error");
}

// ── T027.001: real-PDF integration test ─────────────────────────────────────

/// End-to-end test using a real minimal PDF fixture with a heading (18pt) and
/// body text (10pt). Exercises the full `parse_pdf_document` pipeline without
/// mocking `OutputDev` calls.
///
/// Fixture: `tests/fixtures/sample_heading.pdf` — a 763-byte hand-crafted
/// minimal valid PDF with two font sizes. The heading text is "Introduction"
/// at 18pt; the body text is "Body text." at 10pt.
#[test]
fn parse_pdf_heading_aware_real_pdf() {
    let fixture_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_heading.pdf");

    let bytes = std::fs::read(&fixture_path)
        .unwrap_or_else(|e| panic!("could not read fixture {}: {e}", fixture_path.display()));

    let result = parse_pdf_document(&bytes, "sample_heading.pdf");

    let doc = result.expect("parse_pdf_document must succeed on a valid PDF");

    assert!(
        !doc.chunks.is_empty(),
        "real PDF must produce at least 1 chunk, got {}",
        doc.chunks.len()
    );

    // At least one chunk must have a heading hierarchy entry containing the
    // known heading text "Introduction" from the fixture.  Checking for the
    // actual heading text (not just non-empty hierarchy) guards against the
    // uniform-font fallback, which also produces non-empty hierarchies
    // (e.g., ["Page 1"]) but would not contain the detected heading name.
    let has_heading = doc.chunks.iter().any(|c| {
        c.heading_hierarchy
            .iter()
            .any(|h| h.contains("Introduction"))
    });
    assert!(
        has_heading,
        "at least one chunk must have a heading_hierarchy entry containing \
         'Introduction' (the 18pt heading in the fixture); \
         chunks: {:#?}",
        doc.chunks
            .iter()
            .map(|c| &c.heading_hierarchy)
            .collect::<Vec<_>>()
    );
}
