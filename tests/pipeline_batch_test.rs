//! Integration tests: pipeline batch processing.
//!
//! Verifies that:
//! - `batch_size=2` with 5 files processes all files correctly
//! - `parallel=true` produces identical results to `parallel=false`

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

fn write_five_docs(docs_dir: &std::path::Path) {
    for i in 1..=5 {
        fs::write(
            docs_dir.join(format!("doc{i:02}.md")),
            format!("# Document {i}\n\nContent of document {i}.\n"),
        )
        .unwrap_or_else(|_| panic!("write doc{i:02}.md"));
    }
}

#[test]
fn batch_size_two_processes_all_five_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");
    write_five_docs(&docs_dir);

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "batch-source".to_string(),
            path: docs_dir,
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        })],
    };
    let acquisition_plan = plan(&config, &data_root, root).expect("plan should succeed");

    let store = make_store();
    let pipeline_config = PipelineConfig {
        batch_size: 2,
        parallel: false,
    };

    let result = run(&acquisition_plan, &store, None, &pipeline_config)
        .expect("pipeline run should succeed");

    assert_eq!(
        result.documents_processed, 5,
        "all 5 files should be processed"
    );
    assert!(
        result.errors_encountered.is_empty(),
        "no errors expected: {:?}",
        result.errors_encountered
    );
    assert!(result.total_chunks >= 5, "at least 1 chunk per file");

    let db_chunks = list_chunks_for_source(&store, "batch-source")
        .expect("list_chunks_for_source should succeed");
    assert_eq!(db_chunks.len(), result.total_chunks);
}

#[test]
fn parallel_true_produces_same_result_as_sequential() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");
    write_five_docs(&docs_dir);

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "parallel-source".to_string(),
            path: docs_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        })],
    };

    // Sequential run on a fresh store.
    let seq_store = make_store();
    let seq_plan = plan(&config, &data_root, root).expect("plan (seq)");
    let seq_result = run(
        &seq_plan,
        &seq_store,
        None,
        &PipelineConfig {
            batch_size: 20,
            parallel: false,
        },
    )
    .expect("sequential run");

    // Parallel run on a second fresh store.
    let par_store = make_store();
    let par_plan = plan(&config, &data_root, root).expect("plan (par)");
    let par_result = run(
        &par_plan,
        &par_store,
        None,
        &PipelineConfig {
            batch_size: 20,
            parallel: true,
        },
    )
    .expect("parallel run");

    assert_eq!(
        seq_result.documents_processed, par_result.documents_processed,
        "document count must be identical"
    );
    assert_eq!(
        seq_result.total_chunks, par_result.total_chunks,
        "chunk count must be identical"
    );
    assert_eq!(
        seq_result.errors_encountered.len(),
        par_result.errors_encountered.len(),
        "error count must be identical"
    );
}
