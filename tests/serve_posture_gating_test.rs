//! Characterization + regression tests for P1-T3 (050.001-T): per-database
//! posture threaded through `serve` open + sync gating.
//!
//! `serve` is spawned as a real subprocess with piped stdin that is closed
//! immediately: the MCP stdio transport then reports "connection closed"
//! while starting up, which is a deterministic, fast way to observe the
//! full startup sequence (posture resolution, database opens, and the
//! sync-spawn decision) via stderr without needing to drive a full MCP
//! JSON-RPC session.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use graphtor_core::db::DataStore;

fn graphtor_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"))
}

/// Build a populated, fully-checkpointed v4 fixture database at `db_path`.
fn build_v4_fixture(db_path: &Path, root: &Path) {
    let store = DataStore::open_sqlite(db_path, root).expect("open_sqlite for fixture");
    store.ensure_schema().expect("ensure_schema for fixture");
}

/// Build a pre-v4 (schema version 3) fixture database at `db_path`.
fn build_v3_fixture(db_path: &Path, root: &Path) {
    let store = DataStore::open_sqlite(db_path, root).expect("open_sqlite for fixture");
    store.ensure_schema().expect("ensure_schema for fixture");
    store
        .set_schema_version_for_test(3)
        .expect("downgrade schema version to 3 for fixture");
}

/// Strip ANSI CSI escape sequences (`\x1b[...<final-byte>`) from captured
/// output. This codebase's `tracing_subscriber::fmt()` setup does not call
/// `.with_ansi(false)`, so `info!` log lines are colourised even when piped
/// to a non-terminal (unlike the plain `eprintln!` progress lines other
/// integration tests already assert on) — stripping here keeps field
/// assertions (`generation_count=0`, message text, etc.) robust to that
/// pre-existing, unrelated styling behaviour.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next(); // consume '['
            for next in chars.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&next) {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// Run `graphtor-docs serve` in `workspace` with `stdin` connected to a null
/// device (immediate EOF), so the MCP transport reports a deterministic
/// "connection closed" error shortly after completing its startup sequence.
/// Returns the captured stderr (ANSI-stripped) and exit code.
fn run_serve_with_closed_stdin(workspace: &Path) -> (String, Option<i32>) {
    let output = Command::new(graphtor_bin())
        .current_dir(workspace)
        .arg("serve")
        .stdin(Stdio::null())
        .output()
        .expect("run graphtor-docs serve");

    (
        strip_ansi(&String::from_utf8_lossy(&output.stderr)),
        output.status.code(),
    )
}

fn write_sources_yaml(workspace: &Path, contents: &str) {
    let config_dir = workspace.join(".graphtor").join("config");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    std::fs::write(config_dir.join("sources.yaml"), contents).expect("write sources.yaml");
}

// ── (a) posture gating happy-path / mixed ───────────────────────────────

#[test]
fn pure_consumption_workspace_opens_only_engine_readonly_and_skips_sync() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let graphtor_dir = workspace.path().join(".graphtor");
    std::fs::create_dir_all(&graphtor_dir).expect("create .graphtor");
    let dropped = graphtor_dir.join("dropped.db");
    build_v4_fixture(&dropped, workspace.path());

    let (stderr, code) = run_serve_with_closed_stdin(workspace.path());

    assert_eq!(
        code,
        Some(2),
        "closed-stdin startup exits via the transport error: {stderr}"
    );
    assert!(
        stderr.contains("resolved serve posture") && stderr.contains("generation_count=0"),
        "expected zero Generation databases: {stderr}"
    );
    assert!(
        stderr.contains("opened engine-enforced read-only SQLite DataStore"),
        "consumption workspace must open the engine-enforced read-only primitive: {stderr}"
    );
    assert!(
        !stderr.contains("opened SQLite DataStore"),
        "consumption workspace must never open a read-write store: {stderr}"
    );
    assert!(
        stderr.contains("no generation sources resolved; background sync skipped"),
        "consumption workspace must never spawn a background sync task: {stderr}"
    );
    assert!(
        !stderr.contains("background sync task spawned"),
        "consumption workspace must never spawn a background sync task: {stderr}"
    );
}

#[test]
fn mixed_workspace_gates_generation_and_readonly_independently() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let docs_dir = workspace.path().join("docs");
    std::fs::create_dir_all(&docs_dir).expect("create docs dir");
    std::fs::write(
        docs_dir.join("guide.md"),
        b"---\ntitle: Guide\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: guide.md\n---\n# Guide\n\nHello world.\n",
    )
    .expect("write guide.md");
    write_sources_yaml(
        workspace.path(),
        "sources:\n  - type: local\n    id: guide\n    path: docs\n    include:\n      - \"**/*.md\"\n",
    );
    // An unrelated, co-resident dropped db that no configured source targets.
    let graphtor_dir = workspace.path().join(".graphtor");
    let dropped = graphtor_dir.join("dropped-unrelated.db");
    build_v4_fixture(&dropped, workspace.path());

    let (stderr, code) = run_serve_with_closed_stdin(workspace.path());

    assert_eq!(
        code,
        Some(2),
        "closed-stdin startup exits via the transport error: {stderr}"
    );
    assert!(
        stderr.contains("generation_count=1") && stderr.contains("readonly_count=1"),
        "expected exactly one Generation db and one ReadOnly db: {stderr}"
    );
    assert!(
        stderr.contains("opened SQLite DataStore"),
        "the source-backed target must still open read-write: {stderr}"
    );
    assert!(
        stderr.contains("opened engine-enforced read-only SQLite DataStore"),
        "the co-resident dropped db must open via the engine-enforced read-only primitive: {stderr}"
    );
    assert!(
        stderr.contains("background sync task spawned"),
        "a mixed workspace with a real source must still spawn background sync: {stderr}"
    );
}

// ── (b) fail-safe / empty workspace ─────────────────────────────────────

#[test]
fn zero_config_empty_workspace_exits_gracefully_with_no_databases_message() {
    let workspace = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(workspace.path().join(".graphtor")).expect("create .graphtor");

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .arg("serve")
        .output()
        .expect("run graphtor-docs serve");

    assert_eq!(
        output.status.code(),
        Some(2),
        "empty workspace should exit gracefully, not hang or panic"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no databases found to serve"),
        "expected the graceful no-databases message, not a panic or unreachable!(): {stderr}"
    );
    assert!(
        !stderr.contains("panicked"),
        "must never panic on an empty workspace: {stderr}"
    );
}

#[test]
fn valid_but_empty_sources_yaml_with_no_db_file_exits_gracefully() {
    // A `sources.yaml` that parses successfully but declares zero sources,
    // AND no database file exists yet anywhere: `discover_db_files`'s
    // fallback-to-`base_db_path` default must not be treated as a phantom
    // database to open — nothing was ever dropped and nothing resolves.
    let workspace = tempfile::tempdir().expect("tempdir");
    write_sources_yaml(workspace.path(), "sources: []\n");

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .arg("serve")
        .output()
        .expect("run graphtor-docs serve");

    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no databases found to serve"),
        "an empty source list with no existing db file must not fall back to a phantom \
         default database: {stderr}"
    );
}

// ── (c) observability ───────────────────────────────────────────────────

#[test]
fn startup_log_reports_resolved_posture_and_discovered_count() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let graphtor_dir = workspace.path().join(".graphtor");
    std::fs::create_dir_all(&graphtor_dir).expect("create .graphtor");
    build_v4_fixture(&graphtor_dir.join("a.db"), workspace.path());
    build_v4_fixture(&graphtor_dir.join("b.db"), workspace.path());

    let (stderr, code) = run_serve_with_closed_stdin(workspace.path());

    assert_eq!(
        code,
        Some(2),
        "closed-stdin startup exits via the transport error: {stderr}"
    );
    assert!(
        stderr.contains("resolved serve posture")
            && stderr.contains("discovered_count=2")
            && stderr.contains("generation_count=0")
            && stderr.contains("readonly_count=2"),
        "startup log must positively report the discovered-db count and resolved posture \
         breakdown: {stderr}"
    );
}

// ── P1-T4: v4 gate parity for an auto-discovered read-only db ───────────

#[test]
fn auto_discovered_pre_v4_db_is_refused_via_the_same_v4_gate() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let graphtor_dir = workspace.path().join(".graphtor");
    std::fs::create_dir_all(&graphtor_dir).expect("create .graphtor");
    // A dropped, auto-discovered db with pre-v4 schema — no source config
    // and no explicit --db-path, so this is discovered purely via the
    // `.graphtor/` root scan and classified ReadOnly.
    build_v3_fixture(&graphtor_dir.join("pre-v4-dropped.db"), workspace.path());

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .arg("serve")
        .output()
        .expect("run graphtor-docs serve");

    assert_eq!(
        output.status.code(),
        Some(2),
        "serve should fail at the v4 migration gate for an auto-discovered pre-v4 db"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("pre-v4 schema"),
        "serve should reach the same v4 gate message for an auto-discovered db as for an \
         explicit --db-path target: {stderr}"
    );
    assert!(
        stderr.contains("run `graphtor-docs sync` to rebuild the index before starting serve"),
        "serve should explain how to clear the migration gate: {stderr}"
    );
}
