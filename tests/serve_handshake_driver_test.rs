//! Self-tests for the reusable `graphtor-docs serve` `initialize` handshake
//! driver introduced by `056.002-T` (`tests/common/serve_driver.rs`).
//!
//! These tests exercise the driver itself against neutral fixtures — they
//! never assert a branch-specific regression outcome (that curative
//! assertion belongs to a downstream task per the shipment's own exec-plan
//! boundary). Every test here must be green.

mod common;

use std::path::Path;
use std::time::Duration;

use common::serve_driver::{
    assert_stdout_protocol_clean, run_initialize_handshake, run_read_only_server_control,
    DiagnosticReason, HandshakeOutcome, ServerControlOutcome,
};
use graphtor_core::db::DataStore;
use graphtor_core::mcp::list_mcp_tools;

const DRIVER_TIMEOUT: Duration = Duration::from_secs(20);

fn build_v4_fixture(db_path: &Path, root: &Path) {
    let store = DataStore::open_sqlite(db_path, root).expect("open_sqlite for fixture");
    store.ensure_schema().expect("ensure_schema for fixture");
}

fn write_sources_yaml(workspace: &Path, contents: &str) {
    let config_dir = workspace.join(".graphtor").join("config");
    std::fs::create_dir_all(&config_dir).expect("create config dir");
    std::fs::write(config_dir.join("sources.yaml"), contents).expect("write sources.yaml");
}

// ── run_initialize_handshake ────────────────────────────────────────────

#[test]
fn initialize_handshake_succeeds_against_a_readonly_consumption_workspace() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let graphtor_dir = workspace.path().join(".graphtor");
    std::fs::create_dir_all(&graphtor_dir).expect("create .graphtor");
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

    // Parity control: the whole session's stdout must have stayed
    // protocol-clean (nothing but JSON-RPC frames).
    assert_stdout_protocol_clean(&report.observed_lines);
}

#[test]
fn initialize_handshake_reports_a_diagnostic_for_an_empty_workspace() {
    // No `.graphtor/` directory and no `sources.yaml`: `serve` discovers no
    // databases and exits gracefully (see
    // `zero_config_empty_workspace_exits_gracefully_with_no_databases_message`
    // in `tests/serve_posture_gating_test.rs`) before ever responding to
    // any JSON-RPC request. The driver must report a diagnostic, never a
    // panic and never a false match.
    let workspace = tempfile::tempdir().expect("tempdir");

    let report = run_initialize_handshake(workspace.path(), &[], DRIVER_TIMEOUT);

    match &report.outcome {
        HandshakeOutcome::NoResponse(
            DiagnosticReason::StdoutClosed | DiagnosticReason::BrokenPipeOnWrite(_),
        ) => {}
        other => panic!(
            "expected a diagnostic outcome for an empty workspace, got {other:?} (stderr: {}, \
             exit: {:?})",
            report.stderr, report.exit_code
        ),
    }
}

// ── run_read_only_server_control ────────────────────────────────────────

#[test]
fn server_control_forces_read_only_even_with_a_real_source_present() {
    // Mirrors `read_only_flag_forces_readonly_posture_even_with_a_real_source_present`
    // in `tests/serve_posture_gating_test.rs`: a real, resolvable `local`
    // source (which would normally promote its target to `Generation`)
    // plus an unrelated dropped db. `run_read_only_server_control` always
    // forces `--read-only`, so no write-path stderr marker may ever
    // appear, regardless of the resolved source.
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
    let graphtor_dir = workspace.path().join(".graphtor");
    build_v4_fixture(&graphtor_dir.join("dropped-unrelated.db"), workspace.path());

    let report = run_read_only_server_control(workspace.path(), DRIVER_TIMEOUT);

    match &report.outcome {
        ServerControlOutcome::Control(success) => {
            assert!(
                !success.initialize.protocol_version.is_empty(),
                "negotiated protocolVersion must be non-empty"
            );

            let mut expected: Vec<String> = list_mcp_tools()
                .into_iter()
                .map(|tool| tool.name.as_ref().to_string())
                .collect();
            expected.sort();
            let mut actual = success.tool_names.clone();
            actual.sort();
            assert_eq!(
                actual, expected,
                "server-control tools/list must report exactly the real manifest's tool set"
            );

            assert!(
                !success.get_status_text.is_empty(),
                "get_status must return non-empty status text"
            );
        }
        other => panic!(
            "expected a completed read-only server-control session, got {other:?} (stderr: \
             {}, exit: {:?})",
            report.stderr, report.exit_code
        ),
    }

    assert_stdout_protocol_clean(&report.observed_lines);
}

#[test]
fn server_control_never_shows_a_write_path_in_a_pure_readonly_workspace() {
    // A pure consumption workspace (no source config, only a dropped db):
    // proves the boundary check is not vacuously true because the
    // workspace was never write-capable to begin with -- the previous
    // test is the one that actually exercises the forced override.
    let workspace = tempfile::tempdir().expect("tempdir");
    let graphtor_dir = workspace.path().join(".graphtor");
    std::fs::create_dir_all(&graphtor_dir).expect("create .graphtor");
    build_v4_fixture(&graphtor_dir.join("dropped.db"), workspace.path());

    let report = run_read_only_server_control(workspace.path(), DRIVER_TIMEOUT);

    assert!(
        !report.stderr.contains("opened SQLite DataStore"),
        "must never open a write-capable store: {}",
        report.stderr
    );
    assert!(
        !report.stderr.contains("background sync task spawned"),
        "must never spawn a background sync task: {}",
        report.stderr
    );
    match &report.outcome {
        ServerControlOutcome::Control(_) => {}
        other => panic!(
            "expected a completed read-only server-control session, got {other:?} (stderr: \
             {}, exit: {:?})",
            report.stderr, report.exit_code
        ),
    }
}
