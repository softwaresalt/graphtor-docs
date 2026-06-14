//! Regression tests: duplicate `source_path` within a single source is rejected.
//!
//! Two (or more) files in the same source that declare the same `source_path`
//! in their docline v1 frontmatter would cause delete-before-insert clobbering
//! on future sync cycles — the second reingest deletes the first file's chunks.
//!
//! The pipeline must detect such collisions before loading any chunks and emit
//! a `FileError` for every conflicting file (fail-closed).  No chunks from any
//! conflicting file may be present in the database after a pipeline run.

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

/// Build a docline-conformant markdown string.
fn docline_md(source_path: &str, title: &str, content: &str) -> String {
    format!(
        "---\ntitle: {title}\nsource: /test/source\ningested_at: \
         2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: {source_path}\n---\n{content}"
    )
}

// ── T-DSP-01: two files with the same source_path ────────────────────────────

/// Two files in the same source claiming an identical `source_path` must both
/// be emitted as errors and neither file's chunks may appear in the database.
#[test]
fn two_files_with_same_source_path_both_errored_no_chunks_loaded() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    // Both files claim the same canonical identity.
    let shared_path = "canonical/guide.md";
    fs::write(
        docs_dir.join("file-a.md"),
        docline_md(shared_path, "Guide A", "# Guide A\n\nContent A.\n").as_bytes(),
    )
    .expect("write file-a");
    fs::write(
        docs_dir.join("file-b.md"),
        docline_md(shared_path, "Guide B", "# Guide B\n\nContent B.\n").as_bytes(),
    )
    .expect("write file-b");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "dup-source".to_string(),
            path: docs_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        })],
    };

    let store = make_store();
    let acquisition_plan = plan(&config, &data_root, root).expect("plan");
    let result = run(&acquisition_plan, &store, None, &PipelineConfig::default())
        .expect("pipeline must not return a fatal error");

    // Both conflicting files must appear as errors.
    assert_eq!(
        result.errors_encountered.len(),
        2,
        "both files with duplicate source_path must be errored; \
         errors: {:?}",
        result.errors_encountered
    );

    // Every error message must mention the duplicate source_path.
    for fe in &result.errors_encountered {
        assert!(
            fe.error.contains("duplicate source_path") || fe.error.contains("canonical identity"),
            "error message should reference the duplicate: {:?}",
            fe.error
        );
    }

    // No chunks may be loaded for the conflicting source_path.
    let db_chunks = list_chunks_for_source(&store, "dup-source").expect("list chunks");
    assert!(
        db_chunks.is_empty(),
        "no chunks must be stored when source_path collision is detected; \
         found {} chunks: {:?}",
        db_chunks.len(),
        db_chunks
            .iter()
            .map(|c| c.path.as_str())
            .collect::<Vec<_>>()
    );

    // documents_processed must be 0 (neither file was loaded successfully).
    assert_eq!(
        result.documents_processed, 0,
        "neither conflicting document should be counted as processed"
    );
}

// ── T-DSP-02: duplicate pair + a clean file in same source ───────────────────

/// When a source has two conflicting files AND one clean file (unique `source_path`),
/// only the conflicting pair is rejected; the clean file is processed normally.
#[test]
fn clean_file_succeeds_alongside_duplicate_pair() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    // Conflicting pair.
    let shared_path = "api/reference.md";
    fs::write(
        docs_dir.join("ref-v1.md"),
        docline_md(shared_path, "Ref V1", "# Ref V1\n\nContent.\n").as_bytes(),
    )
    .expect("write ref-v1");
    fs::write(
        docs_dir.join("ref-v2.md"),
        docline_md(shared_path, "Ref V2", "# Ref V2\n\nContent.\n").as_bytes(),
    )
    .expect("write ref-v2");

    // Clean file with a unique source_path.
    fs::write(
        docs_dir.join("guide.md"),
        docline_md("guide.md", "Guide", "# Guide\n\nContent.\n").as_bytes(),
    )
    .expect("write guide");

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "mixed-source".to_string(),
            path: docs_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        })],
    };

    let store = make_store();
    let acquisition_plan = plan(&config, &data_root, root).expect("plan");
    let result = run(&acquisition_plan, &store, None, &PipelineConfig::default())
        .expect("pipeline must not return a fatal error");

    // Exactly 2 errors: one per conflicting file.
    assert_eq!(
        result.errors_encountered.len(),
        2,
        "only the two conflicting files should be errored; \
         errors: {:?}",
        result.errors_encountered
    );

    // Clean file must be processed successfully.
    assert_eq!(
        result.documents_processed, 1,
        "the non-conflicting file must be processed; result: {result:?}"
    );

    // Chunks from the clean file must be in the DB.
    let db_chunks = list_chunks_for_source(&store, "mixed-source").expect("list chunks");
    assert!(
        !db_chunks.is_empty(),
        "clean file must produce at least one chunk"
    );

    // No chunks must be stored under the conflicting source_path.
    let conflict_chunks: Vec<_> = db_chunks.iter().filter(|c| c.path == shared_path).collect();
    assert!(
        conflict_chunks.is_empty(),
        "no chunks must be stored for the conflicting source_path '{shared_path}'; \
         found: {conflict_chunks:?}"
    );
}

// ── T-DSP-03: three-way collision ────────────────────────────────────────────

/// Three files all claiming the same `source_path` must all be errored.
#[test]
fn three_files_with_same_source_path_all_errored() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    let shared_path = "docs/overview.md";
    for (name, title) in [("a.md", "A"), ("b.md", "B"), ("c.md", "C")] {
        fs::write(
            docs_dir.join(name),
            docline_md(shared_path, title, &format!("# {title}\n\nContent.\n")).as_bytes(),
        )
        .unwrap_or_else(|_| panic!("write {name}"));
    }

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "three-way-source".to_string(),
            path: docs_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        })],
    };

    let store = make_store();
    let acquisition_plan = plan(&config, &data_root, root).expect("plan");
    let result = run(&acquisition_plan, &store, None, &PipelineConfig::default())
        .expect("pipeline must not return a fatal error");

    assert_eq!(
        result.errors_encountered.len(),
        3,
        "all three conflicting files must be errored; errors: {:?}",
        result.errors_encountered
    );
    assert_eq!(
        result.documents_processed, 0,
        "no documents should be processed when all files conflict"
    );

    let db_chunks = list_chunks_for_source(&store, "three-way-source").expect("list chunks");
    assert!(
        db_chunks.is_empty(),
        "database must be empty after three-way source_path collision"
    );
}

// ── T-DSP-04: unique source_paths produce no errors ──────────────────────────

/// Baseline regression: files with distinct `source_paths` are not affected by
/// the duplicate detection logic.
#[test]
fn unique_source_paths_are_not_affected() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");

    for i in 1_usize..=3 {
        fs::write(
            docs_dir.join(format!("doc{i}.md")),
            docline_md(
                &format!("docs/doc{i}.md"),
                &format!("Doc {i}"),
                &format!("# Doc {i}\n\nContent {i}.\n"),
            )
            .as_bytes(),
        )
        .unwrap_or_else(|_| panic!("write doc{i}.md"));
    }

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "unique-source".to_string(),
            path: docs_dir.clone(),
            include: vec![],
            exclude: vec![],
            formats: vec![],
            database: None,
        })],
    };

    let store = make_store();
    let acquisition_plan = plan(&config, &data_root, root).expect("plan");
    let result =
        run(&acquisition_plan, &store, None, &PipelineConfig::default()).expect("pipeline run");

    assert!(
        result.errors_encountered.is_empty(),
        "unique source_paths must produce no errors; errors: {:?}",
        result.errors_encountered
    );
    assert_eq!(
        result.documents_processed, 3,
        "all three unique files must be processed"
    );
}
