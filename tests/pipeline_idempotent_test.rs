//! Integration tests: pipeline idempotency.
//!
//! Verifies that running the pipeline twice on the same inputs:
//! - does not double the chunk count in the database (upsert semantics)
//! - produces the same `documents_processed` and `total_chunks` on both runs
//! - chunk IDs are deterministic (SHA-256 of content + `source_path`)

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

#[test]
fn pipeline_run_twice_does_not_double_chunk_count() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    let idmd = "---\ntitle: Idempotent Doc\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: idempotent.md\n---\n# Idempotent Doc\n\nThis content must not be duplicated.\n\n## Section\n\nMore content.\n";
    fs::write(docs_dir.join("idempotent.md"), idmd.as_bytes()).expect("write idempotent.md");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "idempotent-source".to_string(),
            path: docs_dir,
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        })],
    };

    // Single shared store — both runs hit the same CozoDB instance.
    let store = make_store();
    let pipeline_config = PipelineConfig::default();

    let plan1 = plan(&config, &data_root, root).expect("plan for first run");
    let result1 = run(&plan1, &store, None, &pipeline_config).expect("first pipeline run");

    assert_eq!(result1.documents_processed, 1);
    assert!(result1.errors_encountered.is_empty());

    let chunks_after_first =
        list_chunks_for_source(&store, "idempotent-source").expect("list after first run");
    let count_after_first = chunks_after_first.len();
    assert!(count_after_first >= 1, "at least one chunk after first run");

    // Second run — same store, same files.
    let plan2 = plan(&config, &data_root, root).expect("plan for second run");
    let result2 = run(&plan2, &store, None, &pipeline_config).expect("second pipeline run");

    assert_eq!(
        result2.documents_processed, result1.documents_processed,
        "second run should process same number of docs"
    );
    assert_eq!(
        result2.total_chunks, result1.total_chunks,
        "second run should report same chunk count"
    );

    let chunks_after_second =
        list_chunks_for_source(&store, "idempotent-source").expect("list after second run");
    let count_after_second = chunks_after_second.len();

    assert_eq!(
        count_after_second, count_after_first,
        "chunk count in DB must not double after second run (upsert idempotency)"
    );
}

#[test]
fn chunk_ids_are_deterministic_across_runs() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    fs::write(
        docs_dir.join("stable.md"),
        "# Stable\n\nFixed content for ID stability.\n",
    )
    .expect("write stable.md");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "stable-source".to_string(),
            path: docs_dir,
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        })],
    };
    let pipeline_config = PipelineConfig::default();

    // Run 1.
    let store1 = make_store();
    let plan1 = plan(&config, &data_root, root).expect("plan run 1");
    run(&plan1, &store1, None, &pipeline_config).expect("run 1");
    let chunks1 = list_chunks_for_source(&store1, "stable-source").expect("chunks run 1");
    let mut ids1: Vec<String> = chunks1.iter().map(|c| c.chunk_id.clone()).collect();
    ids1.sort();

    // Run 2 on a separate store with the same source content.
    let store2 = make_store();
    let plan2 = plan(&config, &data_root, root).expect("plan run 2");
    run(&plan2, &store2, None, &pipeline_config).expect("run 2");
    let chunks2 = list_chunks_for_source(&store2, "stable-source").expect("chunks run 2");
    let mut ids2: Vec<String> = chunks2.iter().map(|c| c.chunk_id.clone()).collect();
    ids2.sort();

    assert_eq!(
        ids1, ids2,
        "chunk IDs must be deterministic (SHA-256 of content + source_path)"
    );
}
