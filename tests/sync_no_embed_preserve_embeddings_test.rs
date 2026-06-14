//! Regression test: `--no-embed` incremental reingest must not drop embeddings
//! for chunks whose content (and therefore `chunk_id`) remains unchanged.
//!
//! # Scenario
//!
//! A file is ingested with an embedding stored for each chunk.  The file is
//! then touched (same content, same `chunk_id`s) and re-ingested with
//! `model = None` (simulating `--no-embed`).  The embeddings that existed
//! before the re-ingest must still be present afterwards.
//!
//! # Why this matters
//!
//! `reingest_file` calls `delete_file_data` which removes the old chunks
//! (and their stored embeddings) before re-inserting them.  When `model` is
//! `None`, no new embeddings are computed.  Without the fix the chunks are
//! re-inserted with `null` embeddings, permanently erasing the previously
//! computed vectors.

use std::fs;

use graphtor_core::config::source::LocalSource;
use graphtor_core::db::ensure_schema;
use graphtor_core::db::vectors::get_vector;
use graphtor_core::sync::{reingest_file_with_old_contract_path, sync_source};
use graphtor_core::{DataStore, Source};

/// Build a docline-conformant markdown string.
fn docline_md(source_path: &str, title: &str, content: &str) -> String {
    format!(
        "---\ntitle: {title}\nsource: /test/source\ningested_at: \
         2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: {source_path}\n---\n{content}"
    )
}

// ── T-NOEMBED-01: reingest_file with no model preserves existing embeddings ──

/// Directly exercises `reingest_file`:
/// 1. Ingest a file (no model) and manually inject a fake embedding for its chunk.
/// 2. Re-ingest the same file with `model = None`.
/// 3. Assert the fake embedding is still present.
#[test]
fn reingest_file_no_model_preserves_existing_embeddings() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let source_dir = root.join("docs");
    fs::create_dir_all(&source_dir).expect("create source dir");

    let md = docline_md("guide.md", "Guide", "# Guide\n\nStable content.\n");
    let file_path = source_dir.join("guide.md");
    fs::write(&file_path, md.as_bytes()).expect("write guide.md");

    let store = DataStore::open_mem().expect("open in-memory store");
    ensure_schema(&store).expect("ensure schema");
    let source_id = "embed-preserve-test";

    // First ingest: no model.
    let (n1, contract_path) = reingest_file_with_old_contract_path(
        &store,
        source_id,
        &file_path,
        &source_dir,
        root,
        None,
        None,
    )
    .expect("first reingest must succeed");
    assert!(n1 > 0, "first reingest must produce at least one chunk");

    // Manually inject a fake embedding for every chunk from the first ingest.
    let chunks = graphtor_core::db::list_chunks_for_source(&store, source_id).expect("list chunks");
    assert!(
        !chunks.is_empty(),
        "chunks must be stored after first ingest"
    );

    let fake_embedding: Vec<f32> = (0..384_u16).map(|i| f32::from(i) / 384.0).collect();
    for chunk in &chunks {
        graphtor_core::db::vectors::upsert_vector(&store, &chunk.chunk_id, &fake_embedding)
            .expect("inject fake embedding");
    }

    // Verify embeddings are stored before the second ingest.
    for chunk in &chunks {
        let vec = get_vector(&store, &chunk.chunk_id).expect("get vector before reingest");
        assert!(
            vec.is_some(),
            "embedding must exist before second reingest: {}",
            chunk.chunk_id
        );
    }

    // Second ingest: same file, no model.
    let (n2, contract_path2) = reingest_file_with_old_contract_path(
        &store,
        source_id,
        &file_path,
        &source_dir,
        root,
        Some(&contract_path),
        None, // ← --no-embed
    )
    .expect("second reingest must succeed");

    assert_eq!(n1, n2, "chunk count must be stable across reingests");
    assert_eq!(
        contract_path, contract_path2,
        "contract path must be stable"
    );

    // All embeddings must still be present after --no-embed reingest.
    let chunks_after =
        graphtor_core::db::list_chunks_for_source(&store, source_id).expect("list chunks after");
    assert_eq!(
        chunks_after.len(),
        n1,
        "chunk count in DB must match ingest result"
    );

    for chunk in &chunks_after {
        let vec = get_vector(&store, &chunk.chunk_id).expect("get vector after reingest");
        assert!(
            vec.is_some(),
            "--no-embed reingest must preserve existing embeddings for unchanged chunks; \
             embedding is None for chunk_id={}",
            chunk.chunk_id
        );
        // Verify the embedding value is the same fake value we stored.
        let stored = vec.expect("already asserted Some above");
        assert_eq!(
            stored.len(),
            384,
            "embedding dimension must be 384: chunk_id={}",
            chunk.chunk_id
        );
        // Compare a few values to confirm it's our fake embedding.
        assert!(
            (stored[0] - fake_embedding[0]).abs() < f32::EPSILON,
            "embedding values must match the injected fake: chunk_id={}",
            chunk.chunk_id
        );
    }
}

// ── T-NOEMBED-02: sync_source with no model preserves embeddings ──────────────

/// End-to-end scenario through `sync_source`:
/// 1. Sync a file with `model = None`.
/// 2. Manually inject embeddings for all chunks.
/// 3. Re-sync the same file (forced via zeroed stored mtime) with `model = None`.
/// 4. Assert embeddings are still present.
#[test]
fn sync_source_no_model_preserves_existing_embeddings() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let source_dir = root.join("docs");
    fs::create_dir_all(&source_dir).expect("create source dir");

    let md = docline_md(
        "stable/guide.md",
        "Stable Guide",
        "# Stable Guide\n\nParagraph one.\n\nParagraph two.\n",
    );
    fs::write(source_dir.join("guide.md"), md.as_bytes()).expect("write guide.md");

    let state_path = root.join("sync_state.json");
    let store = DataStore::open_mem().expect("open in-memory store");
    ensure_schema(&store).expect("ensure schema");
    let source_id = "noembed-sync-test";

    let source = Source::Local(LocalSource {
        id: source_id.to_string(),
        path: source_dir.clone(),
        include: vec![],
        exclude: vec![],
        formats: vec!["md".to_string()],
        database: None,
    });

    // First sync.
    let m1 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
        .expect("first sync must succeed");
    assert_eq!(m1.files_synced, 1, "first sync: {m1:?}");
    assert_eq!(m1.errors, 0, "first sync: {m1:?}");

    // Inject fake embeddings.
    let chunks = graphtor_core::db::list_chunks_for_source(&store, source_id).expect("list chunks");
    assert!(
        !chunks.is_empty(),
        "at least one chunk must exist after first sync"
    );
    let fake_embedding: Vec<f32> = (0..384_u16).map(|i| f32::from(i).sin()).collect();
    for chunk in &chunks {
        graphtor_core::db::vectors::upsert_vector(&store, &chunk.chunk_id, &fake_embedding)
            .expect("inject embedding");
    }

    // Force re-ingest by zeroing stored mtime.
    {
        let mut state =
            graphtor_core::sync::state::SyncState::load(&state_path, root).expect("load state");
        let src = state.source_mut(source_id);
        src.file_mtimes.insert("guide.md".to_string(), 0);
        state.save(&state_path, root).expect("save state");
    }

    // Second sync: no model.
    let m2 = sync_source(&store, &source, &source_dir, &state_path, root, None, None)
        .expect("second sync must succeed");
    assert_eq!(m2.files_synced, 1, "second sync must re-ingest: {m2:?}");
    assert_eq!(m2.errors, 0, "second sync: {m2:?}");

    // All embeddings must survive the --no-embed reingest.
    let chunks_after =
        graphtor_core::db::list_chunks_for_source(&store, source_id).expect("list chunks after");
    for chunk in &chunks_after {
        let vec = get_vector(&store, &chunk.chunk_id).expect("get vector after second sync");
        assert!(
            vec.is_some(),
            "--no-embed sync must preserve embeddings for unchanged chunks; \
             None for chunk_id={}",
            chunk.chunk_id
        );
    }
}
