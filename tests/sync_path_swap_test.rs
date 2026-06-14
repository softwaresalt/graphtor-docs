//! Regression test: two files that swap `source_path` values in one cycle.
//!
//! Scenario:
//!   Cycle 1 → file-a.md = "alpha/doc.md", file-b.md = "beta/doc.md"  (ingested OK)
//!   Cycle 2 → file-a.md changes to "beta/doc.md", file-b.md changes to "alpha/doc.md"
//!
//! Without the fix, the second reingest (file-b.md old=beta) would call
//! `delete_file_data(..., "beta/doc.md")` which clobbers the chunks that were
//! just loaded by the first reingest (file-a.md new=beta).
//!
//! With the fix, the pre-scan detects the swap and rejects **both** files
//! fail-closed: `metrics.errors == 2`, no chunks are loaded for either path,
//! and neither file advances in sync state.

use std::fs;

use graphtor_core::config::source::LocalSource;
use graphtor_core::db::ensure_schema;
use graphtor_core::sync::sync_source;
use graphtor_core::{DataStore, Source};

/// Build a docline-conformant markdown string.
fn docline_md(source_path: &str, title: &str, content: &str) -> String {
    format!(
        "---\ntitle: {title}\nsource: /test/source\ningested_at: \
         2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: {source_path}\n---\n{content}"
    )
}

fn make_source(id: &str, dir: &std::path::Path) -> Source {
    Source::Local(LocalSource {
        id: id.to_string(),
        path: dir.to_path_buf(),
        include: vec![],
        exclude: vec![],
        formats: vec!["md".to_string()],
        database: None,
    })
}

/// Force sync state to treat both files as modified by zeroing their stored mtimes.
///
/// This is necessary to guarantee the files are seen as "modified" on the next
/// sync cycle regardless of filesystem mtime resolution.
fn zero_stored_mtimes(
    state_path: &std::path::Path,
    root: &std::path::Path,
    source_id: &str,
    fs_rels: &[&str],
) {
    let mut state =
        graphtor_core::sync::state::SyncState::load(state_path, root).expect("load state");
    let src = state.source_mut(source_id);
    for fs_rel in fs_rels {
        src.file_mtimes.insert((*fs_rel).to_string(), 0);
    }
    state
        .save(state_path, root)
        .expect("save state after zeroing mtimes");
}

// ── T-SWAP-01: two files swap source_path values ─────────────────────────────

/// Verifies that two modified files whose `source_path` values swap in a single
/// cycle are both rejected fail-closed (2 errors, 0 synced).
///
/// After the cycle, the DB must contain **all** the chunks from cycle 1 (both
/// files' data is intact — no clobbering occurred).
#[test]
#[allow(clippy::similar_names)]
fn source_path_swap_both_rejected_original_data_preserved() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs = root.join("docs");
    fs::create_dir_all(&docs).expect("create docs dir");

    // Cycle 1: file-a → "alpha/doc.md", file-b → "beta/doc.md"
    let md_a_v1 = docline_md("alpha/doc.md", "Alpha Doc", "# Alpha\n\nContent A.\n");
    let md_b_v1 = docline_md("beta/doc.md", "Beta Doc", "# Beta\n\nContent B.\n");
    fs::write(docs.join("file-a.md"), md_a_v1.as_bytes()).expect("write file-a v1");
    fs::write(docs.join("file-b.md"), md_b_v1.as_bytes()).expect("write file-b v1");

    let state_path = root.join("sync_state.json");
    let store = DataStore::open_mem().expect("open mem store");
    ensure_schema(&store).expect("ensure schema");

    let source = make_source("swap-test", &docs);

    let m1 = sync_source(&store, &source, &docs, &state_path, root, None, None)
        .expect("cycle 1 sync must succeed");
    assert_eq!(m1.files_synced, 2, "cycle 1 must ingest both files: {m1:?}");
    assert_eq!(m1.errors, 0, "cycle 1 must be clean: {m1:?}");

    let chunks_c1 = graphtor_core::db::list_chunks_for_source(&store, "swap-test")
        .expect("list chunks after cycle 1");
    assert!(
        chunks_c1.len() >= 2,
        "cycle 1 must produce at least 2 chunks (one per file)"
    );

    // Cycle 2: swap the source_path values.
    // file-a now claims "beta/doc.md" (previously owned by file-b)
    // file-b now claims "alpha/doc.md" (previously owned by file-a)
    let md_a_v2 = docline_md(
        "beta/doc.md",
        "Alpha to Beta",
        "# Alpha to Beta\n\nContent A v2.\n",
    );
    let md_b_v2 = docline_md(
        "alpha/doc.md",
        "Beta to Alpha",
        "# Beta to Alpha\n\nContent B v2.\n",
    );
    fs::write(docs.join("file-a.md"), md_a_v2.as_bytes()).expect("write file-a v2");
    fs::write(docs.join("file-b.md"), md_b_v2.as_bytes()).expect("write file-b v2");

    // Zero stored mtimes so the sync cycle definitely sees both files as modified,
    // regardless of filesystem timestamp resolution.
    zero_stored_mtimes(&state_path, root, "swap-test", &["file-a.md", "file-b.md"]);

    let m2 = sync_source(&store, &source, &docs, &state_path, root, None, None)
        .expect("cycle 2 sync must not return a fatal error");

    // Both files must be rejected (fail-closed: 2 errors, 0 synced).
    assert_eq!(
        m2.errors, 2,
        "both swapped files must be rejected; metrics: {m2:?}"
    );
    assert_eq!(
        m2.files_synced, 0,
        "no files must be synced on a swap: {m2:?}"
    );

    // The DB must still contain exactly the cycle-1 chunks (no clobbering).
    let chunks_c2 = graphtor_core::db::list_chunks_for_source(&store, "swap-test")
        .expect("list chunks after cycle 2");
    assert_eq!(
        chunks_c2.len(),
        chunks_c1.len(),
        "cycle-1 chunks must be fully preserved after rejected swap; \
         before={}, after={}",
        chunks_c1.len(),
        chunks_c2.len()
    );
}

// ── T-SWAP-02: single-direction steal is also rejected ───────────────────────

/// A file that changes its `source_path` to the OLD path of another changed file
/// (without the reverse) must also be rejected.
///
/// Scenario:
///   Cycle 1: file-a="original.md", file-b="other.md"
///   Cycle 2: file-b changes to "original.md" (claims file-a's old path)
///            file-a also changes content (new `source_path` = "new-original.md")
///
/// In this case file-b's new path matches file-a's OLD path.  Reingest of
/// file-b would delete file-a's stale records (old path), but ALSO the newly
/// loaded file-a records (via the new-path cleanup step in `reingest_file`).
/// The fix rejects file-b fail-closed.
#[test]
#[allow(clippy::similar_names)]
fn source_path_steal_of_changed_file_old_path_is_rejected() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let docs = root.join("docs");
    fs::create_dir_all(&docs).expect("create docs dir");

    // Cycle 1.
    let md_a = docline_md("original.md", "Original", "# Original\n\nContent.\n");
    let md_b = docline_md("other.md", "Other", "# Other\n\nContent.\n");
    fs::write(docs.join("file-a.md"), md_a.as_bytes()).expect("write file-a");
    fs::write(docs.join("file-b.md"), md_b.as_bytes()).expect("write file-b");

    let state_path = root.join("sync_state.json");
    let store = DataStore::open_mem().expect("open mem store");
    ensure_schema(&store).expect("ensure schema");

    let source = make_source("steal-test", &docs);
    let m1 = sync_source(&store, &source, &docs, &state_path, root, None, None)
        .expect("cycle 1 must succeed");
    assert_eq!(m1.errors, 0, "cycle 1 must be clean: {m1:?}");

    // Cycle 2: file-a renames to "new-original.md"; file-b steals "original.md".
    let md_a_v2 = docline_md("new-original.md", "New Original", "# New\n\nContent.\n");
    let md_b_v2 = docline_md("original.md", "Stolen", "# Stolen\n\nContent.\n");
    fs::write(docs.join("file-a.md"), md_a_v2.as_bytes()).expect("write file-a v2");
    fs::write(docs.join("file-b.md"), md_b_v2.as_bytes()).expect("write file-b v2");

    // Force both files to appear as modified.
    zero_stored_mtimes(&state_path, root, "steal-test", &["file-a.md", "file-b.md"]);

    let m2 = sync_source(&store, &source, &docs, &state_path, root, None, None)
        .expect("cycle 2 must not be fatal");

    // file-b must be rejected because its new path == file-a's old path.
    // file-a may succeed (its new path "new-original.md" is unique).
    assert!(
        m2.errors >= 1,
        "file-b with stolen path must produce at least 1 error: {m2:?}"
    );

    // No chunks must be stored under "original.md" for file-b's new content
    // (the steal must not succeed).
    let chunks = graphtor_core::db::list_chunks_for_source(&store, "steal-test")
        .expect("list chunks after cycle 2");
    let stolen_chunks: Vec<_> = chunks.iter().filter(|c| c.path == "original.md").collect();
    // If any chunks exist under "original.md" they should be from the ORIGINAL
    // file-a ingest (cycle 1), not from file-b's stolen-path reingest.
    // The safest assertion: no chunk under "original.md" should have the
    // "Stolen" content from file-b's new document.
    for c in &stolen_chunks {
        assert!(
            !c.content.contains("Stolen"),
            "stolen-path content must not appear in DB: {c:?}"
        );
    }
}
