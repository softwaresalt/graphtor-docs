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
