//! Integration tests: pipeline resilience — continue-on-failure semantics.
//!
//! Verifies that a file containing invalid UTF-8 bytes:
//! - accumulates a `FileError` entry rather than aborting the run
//! - allows other files in the same batch to be processed successfully
//! - does not cause `pipeline::run` to return `Err`

use std::fs;
use std::io::Write;

use graphtor_core::acquire::plan;
use graphtor_core::config::source::{LocalSource, Source, SourceConfig};
use graphtor_core::db::{list_chunks_for_source, DataStore};
use graphtor_core::pipeline::{run, PipelineConfig};

fn make_store() -> DataStore {
    let s = DataStore::open_mem().expect("open in-memory store");
    s.ensure_schema().expect("ensure schema");
    s
}

#[test]
fn pipeline_skips_invalid_utf8_and_processes_valid_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    // Two valid markdown files.
    fs::write(docs_dir.join("valid_a.md"), "# Valid A\n\nContent of A.\n")
        .expect("write valid_a.md");
    fs::write(docs_dir.join("valid_b.md"), "# Valid B\n\nContent of B.\n")
        .expect("write valid_b.md");

    // One file with invalid UTF-8 bytes — will fail `read_to_string`.
    let mut bad_file = fs::File::create(docs_dir.join("bad.md")).expect("create bad.md");
    bad_file
        .write_all(b"\xFF\xFE invalid utf-8")
        .expect("write bad bytes");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "resilience-source".to_string(),
            path: docs_dir,
            include: vec![],
            exclude: vec![],
        })],
    };
    let acquisition_plan = plan(&config, &data_root, root).expect("plan should succeed");

    let store = make_store();
    let pipeline_config = PipelineConfig::default();

    // The run must not return Err — bad file is accumulated, not fatal.
    let result = run(&acquisition_plan, &store, None, &pipeline_config)
        .expect("pipeline run should succeed despite bad file");

    // Exactly one error for the bad file.
    assert_eq!(
        result.errors_encountered.len(),
        1,
        "expected exactly 1 per-file error, got: {:?}",
        result.errors_encountered
    );

    let err = &result.errors_encountered[0];
    let err_path_str = err.path.to_string_lossy();
    assert!(
        err_path_str.contains("bad.md"),
        "error path should reference bad.md, got: {err_path_str}",
    );

    // Two valid files should be fully processed.
    assert_eq!(
        result.documents_processed, 2,
        "expected 2 documents processed"
    );

    // Chunks for the 2 valid documents should be in the DB.
    let db_chunks = list_chunks_for_source(&store, "resilience-source")
        .expect("list_chunks_for_source should succeed");
    assert!(
        !db_chunks.is_empty(),
        "valid documents should produce DB chunks"
    );
}
