//! Integration tests: pipeline PDF processing.
//!
//! Verifies that:
//! - PDF files with invalid bytes are counted as errors (not silent skips).
//! - Files with unsupported extensions (e.g. `.txt`) are skipped without error.
//! - A mixed batch of `.md` + invalid `.pdf` processes the markdown and errors
//!   on the PDF.

use std::fs;

use graphtor_core::acquire::plan;
use graphtor_core::config::source::{LocalSource, Source, SourceConfig};
use graphtor_core::db::DataStore;
use graphtor_core::pipeline::{run, PipelineConfig};

fn make_store() -> DataStore {
    let s = DataStore::open_mem().expect("open in-memory store");
    s.ensure_schema().expect("ensure schema");
    s
}

fn default_pipeline_config() -> PipelineConfig {
    PipelineConfig {
        batch_size: 20,
        parallel: false,
    }
}

// ── T015.003: extension dispatch ─────────────────────────────────────────────

#[test]
fn pipeline_pdf_invalid_bytes_counted_as_error() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    fs::write(docs_dir.join("bad.pdf"), b"not a real pdf").expect("write bad.pdf");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "pdf-error-source".to_string(),
            path: docs_dir,
            include: vec![],
            exclude: vec![],
        })],
    };
    let acquisition_plan = plan(&config, &data_root, root).expect("plan should succeed");

    let store = make_store();
    let result = run(&acquisition_plan, &store, None, &default_pipeline_config())
        .expect("pipeline run should succeed even with parse errors");

    assert_eq!(
        result.documents_processed, 0,
        "invalid PDF must not be counted as a processed document"
    );
    assert!(
        !result.errors_encountered.is_empty(),
        "invalid PDF bytes must produce at least one error"
    );
}

#[test]
fn pipeline_unknown_extension_skipped_silently() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    fs::write(docs_dir.join("notes.txt"), b"some text content").expect("write notes.txt");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "unknown-ext-source".to_string(),
            path: docs_dir,
            include: vec![],
            exclude: vec![],
        })],
    };
    let acquisition_plan = plan(&config, &data_root, root).expect("plan should succeed");

    let store = make_store();
    let result = run(&acquisition_plan, &store, None, &default_pipeline_config())
        .expect("pipeline run should succeed");

    assert_eq!(
        result.documents_processed, 0,
        "unsupported extension must not increment documents_processed"
    );
    assert!(
        result.errors_encountered.is_empty(),
        "unsupported extension must not produce errors — it is silently skipped"
    );
}

#[test]
fn pipeline_mixed_batch_md_processed_invalid_pdf_errored() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    fs::write(
        docs_dir.join("guide.md"),
        "# Guide\n\nContent of the guide.\n",
    )
    .expect("write guide.md");
    fs::write(docs_dir.join("ref.pdf"), b"not a real pdf").expect("write ref.pdf");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "mixed-source".to_string(),
            path: docs_dir,
            include: vec![],
            exclude: vec![],
        })],
    };
    let acquisition_plan = plan(&config, &data_root, root).expect("plan should succeed");

    let store = make_store();
    let result = run(&acquisition_plan, &store, None, &default_pipeline_config())
        .expect("pipeline run should succeed");

    assert_eq!(
        result.documents_processed, 1,
        "markdown file must be processed successfully"
    );
    assert!(
        !result.errors_encountered.is_empty(),
        "invalid PDF must produce at least one error"
    );
}
