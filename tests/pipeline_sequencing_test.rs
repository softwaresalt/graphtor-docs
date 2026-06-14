//! Integration tests: pipeline sequencing — acquire → parse → embed → load.
//!
//! Verifies that a basic pipeline run with three markdown documents:
//! - processes all documents successfully
//! - writes chunks to the database
//! - records no per-file errors

use std::fs;

use graphtor_core::acquire::plan;
use graphtor_core::config::source::{LocalSource, Source, SourceConfig};
use graphtor_core::db::{list_chunks_for_source, DataStore};
use graphtor_core::pipeline::{run, PipelineConfig};

fn make_store() -> DataStore {
    let s = DataStore::open_mem().expect("open in-memory store");
    s.ensure_schema().expect("ensure schema");
    s
}

fn make_config(local_path: std::path::PathBuf) -> SourceConfig {
    SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "test-source".to_string(),
            path: local_path,
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        })],
    }
}

#[test]
fn pipeline_processes_three_docs_and_populates_db() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    let alpha = "---\ntitle: Alpha\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: alpha.md\n---\n# Alpha\n\nFirst document.\n\n## Details\n\nSome details here.\n";
    let beta = "---\ntitle: Beta\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: beta.md\n---\n# Beta\n\nSecond document.\n\n## Overview\n\nAn overview.\n";
    let gamma = "---\ntitle: Gamma\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: gamma.md\n---\n# Gamma\n\nThird document.\n\n## Notes\n\nSome notes.\n";
    fs::write(docs_dir.join("alpha.md"), alpha.as_bytes()).expect("write alpha.md");
    fs::write(docs_dir.join("beta.md"), beta.as_bytes()).expect("write beta.md");
    fs::write(docs_dir.join("gamma.md"), gamma.as_bytes()).expect("write gamma.md");

    let data_root = root.join("data");
    let config = make_config(docs_dir);
    let acquisition_plan = plan(&config, &data_root, root).expect("plan should succeed");

    let store = make_store();
    let pipeline_config = PipelineConfig::default();

    let result = run(&acquisition_plan, &store, None, &pipeline_config)
        .expect("pipeline run should succeed");

    // All three files should be processed without errors.
    assert_eq!(
        result.documents_processed, 3,
        "all 3 docs should be processed"
    );
    assert!(
        result.errors_encountered.is_empty(),
        "expected no errors, got: {:?}",
        result.errors_encountered
    );

    // Each file has 2 chunks (intro + H2 section), so 6 total minimum.
    assert!(
        result.total_chunks >= 6,
        "expected at least 6 chunks, got {}",
        result.total_chunks
    );

    // Verify chunks are actually in the database.
    let db_chunks = list_chunks_for_source(&store, "test-source")
        .expect("list_chunks_for_source should succeed");
    assert_eq!(
        db_chunks.len(),
        result.total_chunks,
        "DB chunk count should match reported total_chunks"
    );
    assert!(
        !db_chunks.is_empty(),
        "at least one chunk should be in the database"
    );
}
