//! Tests for `056.003-T`: the typed `cmd_serve` pre-transport exit/error
//! observability seam (`src/workspace/serve_preflight.rs`).
//!
//! Three groups, matching the task's own acceptance criteria:
//!
//! 1. **Exhaustive typed-exit formatter/event mapping** — every reachable
//!    [`ServePreflightExit`](graphtor_core) variant is triggered against a
//!    real fixture, and BOTH the pre-existing `eprintln!` message AND the
//!    new `mcp_serve_preflight_exit` structured `tracing` event are
//!    observed in the real process's stderr, at the documented exit code.
//! 2. **A representative propagated error plus `RUST_LOG=off` loud
//!    stderr** — a malformed source registry propagates through
//!    `cmd_serve` to the top-level fatal renderer. The unconditional
//!    `error: ...` message must survive `RUST_LOG=off`, while the
//!    *additional* `mcp_serve_preflight_error` structured event (which
//!    goes through `tracing`, and so IS legitimately silenced by
//!    `RUST_LOG=off`) must be present when logging is enabled and absent
//!    when it is off — proving the two channels are genuinely
//!    independent, not that the loud message happens to always appear.
//! 3. **The `mcp_serve_ready` event with a real seeded fixture**, using the
//!    `056.002-T` parity-control harness (`assert_stdout_protocol_clean`)
//!    to prove the new diagnostics add no stdout bytes and the
//!    `initialize` wire/protocol is unchanged.
//!
//! None of these tests assert on `ServePreflightExit::NoPrimaryStore`: that
//! variant is documented, structurally-unreachable defence-in-depth (the
//! two earlier `NoDatabasesToServe` gates already guarantee at least one
//! store reaches that point), so it is covered only by the pure unit tests
//! in `src/workspace/serve_preflight.rs` — forcing a real process into that
//! branch would require bypassing the very gates that make it
//! unreachable.

mod common;

use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use common::serve_driver::{
    assert_stdout_protocol_clean, run_initialize_handshake, HandshakeOutcome,
};
use graphtor_core::db::DataStore;

const DRIVER_TIMEOUT: Duration = Duration::from_secs(20);

fn graphtor_bin() -> String {
    env!("CARGO_BIN_EXE_graphtor-docs").to_string()
}

fn write_yaml(dir: &Path, name: &str, content: &str) {
    fs::write(dir.join(name), content).expect("write yaml");
}

/// Strip ANSI CSI escape sequences (`\x1b[...<final-byte>`) from captured
/// output. Mirrors `tests/serve_posture_gating_test.rs`'s own helper: this
/// codebase's `tracing_subscriber::fmt()` setup does not call
/// `.with_ansi(false)`, so `info!`/`error!` log lines (and their individual
/// `key=value` fields) are colourised even when piped to a non-terminal —
/// stripping here keeps compound field assertions (e.g.
/// `mcp_serve_preflight_exit` next to `config_override_not_found`, or
/// `preflight_complete=true`) robust to that pre-existing, unrelated
/// styling behaviour.
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

fn build_v4_fixture(db_path: &Path, root: &Path) {
    let store = DataStore::open_sqlite(db_path, root).expect("open_sqlite for fixture");
    store.ensure_schema().expect("ensure_schema for fixture");
}

fn build_v3_fixture(db_path: &Path, root: &Path) {
    let store = DataStore::open_sqlite(db_path, root).expect("open_sqlite for fixture");
    store.ensure_schema().expect("ensure_schema for fixture");
    store
        .set_schema_version_for_test(3)
        .expect("downgrade schema version to 3 for fixture");
}

// ── Group (a): exhaustive typed-exit formatter/event mapping ───────────

#[test]
fn config_override_not_found_exit_traces_and_preserves_message() {
    let workspace = tempfile::tempdir().expect("tempdir");

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .arg("serve")
        .arg("--config")
        .arg("does-not-exist.yaml")
        .output()
        .expect("run graphtor-docs serve");

    assert_eq!(output.status.code(), Some(2));
    let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr));
    // Preserved, unconditional eprintln! message (mirror, never convert).
    assert!(
        stderr.contains("error: config file 'does-not-exist.yaml' not found"),
        "existing config-override-not-found message must be preserved verbatim: {stderr}"
    );
    // Additional structured trace event.
    assert!(
        stderr.contains("mcp_serve_preflight_exit") && stderr.contains("config_override_not_found"),
        "expected the new structured preflight-exit trace event: {stderr}"
    );
}

#[test]
fn no_databases_to_serve_exit_traces_and_preserves_message() {
    let workspace = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(workspace.path().join(".graphtor")).expect("create .graphtor");

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .arg("serve")
        .output()
        .expect("run graphtor-docs serve");

    assert_eq!(output.status.code(), Some(2));
    let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr));
    assert!(
        stderr.contains("no databases found to serve"),
        "existing no-databases message must be preserved verbatim: {stderr}"
    );
    assert!(
        stderr.contains("mcp_serve_preflight_exit") && stderr.contains("no_databases_to_serve"),
        "expected the new structured preflight-exit trace event: {stderr}"
    );
}

#[test]
fn pre_v4_schema_exit_traces_and_preserves_message() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let graphtor_dir = workspace.path().join(".graphtor");
    fs::create_dir_all(&graphtor_dir).expect("create .graphtor");
    // Auto-discovered dropped db (ReadOnly posture) with a pre-v4 schema —
    // exercises the ReadOnly branch's PreV4Schema call site.
    build_v3_fixture(&graphtor_dir.join("pre-v4-dropped.db"), workspace.path());

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .arg("serve")
        .output()
        .expect("run graphtor-docs serve");

    assert_eq!(output.status.code(), Some(2));
    let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr));
    assert!(
        stderr.contains("has pre-v4 schema")
            && stderr
                .contains("run `graphtor-docs sync` to rebuild the index before starting serve"),
        "existing pre-v4-schema message must be preserved verbatim: {stderr}"
    );
    assert!(
        stderr.contains("mcp_serve_preflight_exit") && stderr.contains("pre_v4_schema"),
        "expected the new structured preflight-exit trace event: {stderr}"
    );
}

#[test]
fn pre_v4_schema_exit_traces_for_the_generation_branch_too() {
    // Same variant, but via the Generation open path (source-config-backed
    // target) rather than the ReadOnly auto-discovery path above -- proves
    // both `open_serve_databases` call sites route through the same
    // `ServePreflightExit::PreV4Schema` variant.
    let workspace = tempfile::tempdir().expect("tempdir");
    let docs_dir = workspace.path().join("docs");
    fs::create_dir_all(&docs_dir).expect("create docs dir");
    fs::write(
        docs_dir.join("guide.md"),
        b"---\ntitle: Guide\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: guide.md\n---\n# Guide\n\nHello world.\n",
    )
    .expect("write guide.md");
    let config_dir = workspace.path().join(".graphtor").join("config");
    fs::create_dir_all(&config_dir).expect("create config dir");
    write_yaml(
        &config_dir,
        "sources.yaml",
        "sources:\n  - type: local\n    id: guide\n    path: docs\n    include:\n      - \"**/*.md\"\n    database: guide.db\n",
    );
    let db_path = workspace.path().join(".graphtor").join("guide.db");
    build_v3_fixture(&db_path, workspace.path());

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .arg("serve")
        .output()
        .expect("run graphtor-docs serve");

    assert_eq!(output.status.code(), Some(2));
    let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr));
    assert!(
        stderr.contains("has pre-v4 schema"),
        "existing pre-v4-schema message must be preserved verbatim: {stderr}"
    );
    assert!(
        stderr.contains("mcp_serve_preflight_exit") && stderr.contains("pre_v4_schema"),
        "expected the new structured preflight-exit trace event: {stderr}"
    );
}

#[test]
fn duplicate_intake_conflict_exit_traces_and_preserves_message() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let config_dir = workspace.path().join(".graphtor").join("config");
    fs::create_dir_all(&config_dir).expect("create config dir");

    let shared = workspace.path().join("shared");
    fs::create_dir_all(&shared).expect("create shared dir");
    fs::write(
        shared.join("readme.md"),
        b"---\ntitle: Shared\nsource: /test/s\ningested_at: 2026-01-01T00:00:00Z\n\
          doc_type: markdown\nsource_path: readme.md\n---\n# Shared\n",
    )
    .expect("write readme");
    let shared_str = shared.to_string_lossy().replace('\\', "/");

    write_yaml(
        &config_dir,
        "alpha.sources.yaml",
        &format!(
            "sources:\n  - type: local\n    id: shared-a\n    path: {shared_str}\n    database: alpha.db\n"
        ),
    );
    write_yaml(
        &config_dir,
        "beta.sources.yaml",
        &format!(
            "sources:\n  - type: local\n    id: shared-b\n    path: {shared_str}\n    database: beta.db\n"
        ),
    );

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .arg("serve")
        .output()
        .expect("run graphtor-docs serve");

    assert_eq!(
        output.status.code(),
        Some(2),
        "serve should exit 2 on duplicate detection before starting the MCP server"
    );
    let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr));
    // The shared `run_duplicate_intake_preflight` helper's own message
    // (unowned by this task) must still be preserved verbatim.
    assert!(
        stderr.contains("cross-database duplicate intakes detected"),
        "existing duplicate-intake message must be preserved verbatim: {stderr}"
    );
    assert!(
        stderr.contains("mcp_serve_preflight_exit") && stderr.contains("duplicate_intake_conflict"),
        "expected the new structured preflight-exit trace event: {stderr}"
    );
}

// ── Group (b): representative propagated error + RUST_LOG=off ─────────

fn malformed_registry_workspace() -> tempfile::TempDir {
    let workspace = tempfile::tempdir().expect("tempdir");
    let config_dir = workspace.path().join(".graphtor").join("config");
    fs::create_dir_all(&config_dir).expect("create config dir");
    write_yaml(
        &config_dir,
        "sources.yaml",
        // Unterminated YAML sequence -- guaranteed parse failure.
        "sources:\n  - type: local\n    id: broken\n    path: [unterminated\n",
    );
    workspace
}

#[test]
fn propagated_registry_error_traces_and_preserves_fatal_message_with_logging_enabled() {
    let workspace = malformed_registry_workspace();

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .arg("serve")
        .output()
        .expect("run graphtor-docs serve");

    assert_ne!(output.status.code(), Some(0), "must exit non-zero");
    let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr));
    assert!(
        stderr.contains("error:") && stderr.contains("source registry is invalid"),
        "top-level fatal renderer must still render the propagated registry error: {stderr}"
    );
    assert!(
        stderr.contains("mcp_serve_preflight_error") && stderr.contains("source_config"),
        "expected the new structured preflight-error trace event with logging enabled: {stderr}"
    );
}

#[test]
fn rust_log_off_silences_the_trace_event_but_never_the_fatal_message() {
    let workspace = malformed_registry_workspace();

    let output = Command::new(graphtor_bin())
        .current_dir(workspace.path())
        .arg("serve")
        .env("RUST_LOG", "off")
        .output()
        .expect("run graphtor-docs serve");

    assert_ne!(output.status.code(), Some(0), "must exit non-zero");
    let stderr = strip_ansi(&String::from_utf8_lossy(&output.stderr));
    // The unconditional top-level fatal renderer bypasses `tracing`
    // entirely, so `RUST_LOG=off` must never silence it.
    assert!(
        stderr.contains("error:") && stderr.contains("source registry is invalid"),
        "RUST_LOG=off must not silence the user-facing fatal error message: {stderr}"
    );
    // The *additional* structured trace event legitimately goes through
    // `tracing` and so IS silenced by RUST_LOG=off -- proving the two
    // channels are genuinely independent (mirror, never convert).
    assert!(
        !stderr.contains("mcp_serve_preflight_error"),
        "RUST_LOG=off should silence the additional structured trace event: {stderr}"
    );
}

// ── Group (c): mcp_serve_ready with a real seeded fixture (parity-safe) ─

#[test]
fn mcp_serve_ready_event_fires_immediately_before_transport_with_stdout_untouched() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let graphtor_dir = workspace.path().join(".graphtor");
    fs::create_dir_all(&graphtor_dir).expect("create .graphtor");
    build_v4_fixture(&graphtor_dir.join("dropped.db"), workspace.path());

    let report = run_initialize_handshake(workspace.path(), &[], DRIVER_TIMEOUT);

    match &report.outcome {
        HandshakeOutcome::Initialized(success) => {
            assert!(
                !success.protocol_version.is_empty(),
                "negotiated protocolVersion must be non-empty"
            );
        }
        other => panic!(
            "expected a successful initialize handshake, got {other:?} (stderr: {}, exit: \
             {:?})",
            report.stderr, report.exit_code
        ),
    }

    // Parity-control assertion (owned by 056.003-T, using the 056.002-T
    // harness): the new preflight diagnostics add no stdout bytes and the
    // initialize wire/protocol is unchanged.
    assert_stdout_protocol_clean(&report.observed_lines);

    // The mcp_serve_ready structured event fired on stderr, immediately
    // before serve_server -- means preflight-complete/about-to-call only.
    let stderr = strip_ansi(&report.stderr);
    assert!(
        stderr.contains("mcp_serve_ready"),
        "expected the mcp_serve_ready structured event: {stderr}"
    );
    assert!(
        stderr.contains("preflight_complete=true"),
        "mcp_serve_ready must report preflight_complete=true: {stderr}"
    );
}
