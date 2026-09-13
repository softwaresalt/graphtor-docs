//! Black-box self-test for `mcp_probe::exact_cli::run_exact_cli`'s
//! Gate-1 spawn-failure fail-closed behavior (Copilot review, 2026-09 --
//! 049-S PR #120).
//!
//! Lives as an integration test (not a `#[cfg(test)]` unit-test module)
//! for the same `std::env::current_exe()`-inside-`cargo-test`-harness
//! reason documented in `process_test.rs`/`transport_test.rs`/
//! `workspace_test.rs`: `run_exact_cli` resolves its own absolute path
//! via `std::env::current_exe()` to embed as the diagnostic wrapper
//! command, and only a real binary (not an in-process unit test) can
//! stand in for that.

use mcp_probe::exact_cli::{run_exact_cli, ExactCliArgs};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// A unique, freshly created directory under the OS temp directory to
/// stand in as a fake "repository root" for one test, mirroring
/// `workspace_test.rs`'s `fresh_fake_repo_root` hygiene.
fn fresh_fake_repo_root(label: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mcp-probe-exact-cli-test-{label}-{}-{n}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("create fake repo root");
    root
}

/// A real, spawnable, near-instantly-exiting stand-in for the exact
/// target Copilot CLI: this crate's own compiled binary (via the same
/// `CARGO_BIN_EXE_mcp-probe` pattern already used by
/// `process_test.rs`/`transport_test.rs`/`evidence_test.rs`). Any
/// argument list this test drives it with (`--version`, or Gate 1's own
/// `-C <dir> mcp get <entry> --json`) is an unrecognized subcommand to
/// `main.rs`'s own dispatch, so it exits quickly with a non-zero code
/// without ever hanging. Because the file genuinely exists and is
/// readable, `identify_copilot` always succeeds against it (no
/// `identify_error`), letting a test isolate an `identify_inner_exe`
/// failure as the sole identity-check trigger.
fn spawnable_fast_exiting_exe() -> String {
    env!("CARGO_BIN_EXE_mcp-probe").to_string()
}

#[test]
fn run_exact_cli_fails_closed_with_a_blocked_terminal_when_gate1_cannot_spawn_the_exact_cli() {
    let repo_root = fresh_fake_repo_root("gate1-spawn-failure");
    let unlaunchable_copilot_exe = repo_root
        .join("definitely-does-not-exist-copilot-cli.exe")
        .to_string_lossy()
        .into_owned();

    let args = ExactCliArgs {
        copilot_exe: unlaunchable_copilot_exe,
        stable_copilot_exe: None,
        repo_root: repo_root.to_string_lossy().into_owned(),
        inner_exe: "definitely-does-not-exist-inner-server".to_string(),
        inner_args: Vec::new(),
        entry_name: "graphtor-docs".to_string(),
        run_nonce: Some("exact-cli-test-gate1-spawn-failure".to_string()),
        leg_deadline: Duration::from_secs(2),
        gate1_deadline: Duration::from_secs(2),
        prompt: "diagnostic probe prompt".to_string(),
        sentinel_value: None,
    };

    let outcome = run_exact_cli(&args).expect(
        "run_exact_cli only errors on workspace-creation failure, which is not exercised here",
    );

    assert!(
        outcome.gate1.spawn_error.is_some(),
        "Gate 1 must record why the exact CLI could not be spawned"
    );
    assert!(
        !outcome.gate1.passed,
        "an unlaunchable exact CLI can never pass Gate 1"
    );
    assert!(
        !outcome.h3_b_candidate,
        "a Gate 1 spawn failure must NOT be reported as an H3-B-candidate -- isolation was \
         never proven either way, so this must never be conflated with a genuine \
         ancestor-config-merge detection"
    );
    assert_eq!(
        outcome.terminal, "blocked",
        "a Gate 1 spawn failure is a genuine, explicit non-H3-B evidence-capture blocker"
    );
    assert!(
        outcome.passes.is_empty(),
        "no causal control/treatment pass may run when Gate 1 itself could not be proven"
    );

    // Copilot review thread 2 (PR #120): the inner `--inner-exe` identity
    // must still be recorded even when Gate 1 itself is blocked -- it is
    // computed unconditionally, before Gate 1 runs, so a nonexistent
    // inner exe path must surface a read `identify_error`, never a spawn
    // attempt (`version_output` is always `None` for the inner exe; see
    // `identify_inner_exe`'s own doc comment for why it is never spawned).
    assert_eq!(outcome.inner_identity.exe_path, args.inner_exe);
    assert!(outcome.inner_identity.version_output.is_none());
    assert!(outcome.inner_identity.identify_error.is_some());
    assert_eq!(outcome.inner_identity.content_hash_hex, "");
    assert_eq!(outcome.inner_identity.content_len, 0);

    let _ = std::fs::remove_dir_all(&repo_root);
}

#[test]
fn run_exact_cli_records_inner_identity_by_content_hash_when_the_inner_exe_file_exists() {
    let repo_root = fresh_fake_repo_root("inner-identity-real-file");
    let unlaunchable_copilot_exe = repo_root
        .join("definitely-does-not-exist-copilot-cli.exe")
        .to_string_lossy()
        .into_owned();

    // A real, readable file -- but never spawned -- stands in for the
    // inner MCP server binary so `identify_inner_exe`'s file-hash path
    // (as opposed to its read-error path, covered by the sibling test
    // above) is exercised.
    let inner_exe_path = repo_root.join("fake-inner-server.bin");
    std::fs::write(&inner_exe_path, b"fake inner mcp server bytes")
        .expect("write fake inner exe fixture");

    let args = ExactCliArgs {
        copilot_exe: unlaunchable_copilot_exe,
        stable_copilot_exe: None,
        repo_root: repo_root.to_string_lossy().into_owned(),
        inner_exe: inner_exe_path.to_string_lossy().into_owned(),
        inner_args: Vec::new(),
        entry_name: "graphtor-docs".to_string(),
        run_nonce: Some("exact-cli-test-inner-identity-real-file".to_string()),
        leg_deadline: Duration::from_secs(2),
        gate1_deadline: Duration::from_secs(2),
        prompt: "diagnostic probe prompt".to_string(),
        sentinel_value: None,
    };

    let outcome = run_exact_cli(&args).expect(
        "run_exact_cli only errors on workspace-creation failure, which is not exercised here",
    );

    assert!(
        outcome.inner_identity.identify_error.is_none(),
        "a readable inner exe file must never surface a read error"
    );
    assert!(
        outcome.inner_identity.version_output.is_none(),
        "the inner exe must never be spawned for --version (undefined/unsafe for an \
         arbitrary MCP server binary), so version_output is always None"
    );
    assert_eq!(outcome.inner_identity.content_len, 27);
    assert!(
        !outcome.inner_identity.content_hash_hex.is_empty(),
        "a readable inner exe file must produce a non-empty content hash"
    );

    let _ = std::fs::remove_dir_all(&repo_root);
}

#[test]
fn run_exact_cli_persists_the_result_json_under_the_probe_workspace() {
    use mcp_probe::exact_cli::persist_outcome_json;

    let repo_root = fresh_fake_repo_root("persist-result-json");
    let unlaunchable_copilot_exe = repo_root
        .join("definitely-does-not-exist-copilot-cli.exe")
        .to_string_lossy()
        .into_owned();

    let args = ExactCliArgs {
        copilot_exe: unlaunchable_copilot_exe,
        stable_copilot_exe: None,
        repo_root: repo_root.to_string_lossy().into_owned(),
        inner_exe: "definitely-does-not-exist-inner-server".to_string(),
        inner_args: Vec::new(),
        entry_name: "graphtor-docs".to_string(),
        run_nonce: Some("exact-cli-test-persist-result-json".to_string()),
        leg_deadline: Duration::from_secs(2),
        gate1_deadline: Duration::from_secs(2),
        prompt: "diagnostic probe prompt".to_string(),
        sentinel_value: None,
    };

    let outcome = run_exact_cli(&args).expect(
        "run_exact_cli only errors on workspace-creation failure, which is not exercised here",
    );

    // Copilot review thread 5 (PR #120): the final classification JSON
    // must be persisted under the probe workspace, not only printed to
    // stdout by the caller, matching the doc-comment contract this task
    // already claimed.
    let written_path =
        persist_outcome_json(&outcome).expect("persisting the result JSON must succeed");
    assert!(written_path.starts_with(&outcome.workspace_root));
    assert!(written_path.is_file());

    let persisted_text = std::fs::read_to_string(&written_path).expect("read persisted JSON");
    let persisted_json: serde_json::Value =
        serde_json::from_str(&persisted_text).expect("persisted content must be valid JSON");
    assert_eq!(persisted_json["run_nonce"], outcome.run_nonce);
    assert_eq!(persisted_json["terminal"], outcome.terminal);

    let _ = std::fs::remove_dir_all(&repo_root);
}

#[test]
fn run_exact_cli_fails_closed_when_the_inner_executable_identity_cannot_be_read() {
    // Copilot review thread B (PR #120, round 2): recording an identity
    // failure is not the same as ENFORCING it. Use a real, spawnable
    // stand-in for `--copilot-exe` (so `identify_copilot` succeeds and
    // cannot itself trigger the identity fail-closed path) paired with a
    // deliberately nonexistent `--inner-exe`, and assert the run still
    // fails closed on the inner-executable identity, regardless of
    // whatever Gate 1 itself observes.
    let repo_root = fresh_fake_repo_root("inner-identity-fail-closed");
    let spawnable_copilot_exe = spawnable_fast_exiting_exe();

    let args = ExactCliArgs {
        copilot_exe: spawnable_copilot_exe,
        stable_copilot_exe: None,
        repo_root: repo_root.to_string_lossy().into_owned(),
        inner_exe: "definitely-does-not-exist-inner-server".to_string(),
        inner_args: Vec::new(),
        entry_name: "graphtor-docs".to_string(),
        run_nonce: Some("exact-cli-test-inner-identity-fail-closed".to_string()),
        leg_deadline: Duration::from_secs(5),
        gate1_deadline: Duration::from_secs(5),
        prompt: "diagnostic probe prompt".to_string(),
        sentinel_value: None,
    };

    let outcome = run_exact_cli(&args).expect(
        "run_exact_cli only errors on workspace-creation failure, which is not exercised here",
    );

    assert!(
        outcome.affected_identity.identify_error.is_none(),
        "the spawnable stand-in exe is a real, readable file, so its own identity capture \
         must succeed -- proving this test isolates the INNER identity failure, not a \
         copilot-identity failure"
    );
    assert!(
        outcome.inner_identity.identify_error.is_some(),
        "the nonexistent inner exe must surface a read error"
    );
    assert_eq!(
        outcome.terminal, "blocked",
        "an unproved inner-executable identity must fail closed regardless of Gate 1's own \
         result (056.001-T: fail closed when ... same-inner-executable ... parity is unproved)"
    );
    assert!(!outcome.h3_b_candidate);
    assert!(outcome.passes.is_empty());
    assert!(
        outcome.ordered_cause_classification[0].contains("inner executable's identity"),
        "the blocked reason must cite the inner-executable identity failure specifically, \
         got: {:?}",
        outcome.ordered_cause_classification
    );

    let _ = std::fs::remove_dir_all(&repo_root);
}

#[test]
fn run_exact_cli_does_not_classify_a_gate1_timeout_as_an_h3_b_candidate() {
    // Copilot review thread G (PR #120, round 2): a Gate 1 timeout (or
    // malformed/empty successful output) proves NOTHING about whether
    // the exact CLI read or merged the ancestor config -- it must fail
    // closed as "blocked", never be classified as the causal
    // `H3-B-candidate` reserved for a positively OBSERVED ancestor
    // merge.
    let repo_root = fresh_fake_repo_root("gate1-timeout-not-h3-b");
    let spawnable_copilot_exe = spawnable_fast_exiting_exe();

    let args = ExactCliArgs {
        copilot_exe: spawnable_copilot_exe,
        stable_copilot_exe: None,
        repo_root: repo_root.to_string_lossy().into_owned(),
        inner_exe: "definitely-does-not-exist-inner-server".to_string(),
        inner_args: Vec::new(),
        entry_name: "graphtor-docs".to_string(),
        run_nonce: Some("exact-cli-test-gate1-timeout-not-h3-b".to_string()),
        leg_deadline: Duration::from_secs(5),
        gate1_deadline: Duration::from_secs(5),
        prompt: "diagnostic probe prompt".to_string(),
        sentinel_value: None,
    };

    let outcome = run_exact_cli(&args).expect(
        "run_exact_cli only errors on workspace-creation failure, which is not exercised here",
    );

    // The spawnable stand-in exits quickly on an unrecognized subcommand
    // (never a JSON `mcp get --json` response), so Gate 1 itself fails
    // with a parse error here -- but this test's fixed point is that the
    // OVERALL outcome is never H3-B-candidate for ANY unproved Gate 1
    // shape (spawn failure, timeout, non-zero exit, or parse error);
    // the inner-identity failure above additionally guarantees a
    // "blocked" terminal regardless of Gate 1's own classification.
    assert!(!outcome.gate1.passed);
    assert!(
        !outcome.h3_b_candidate,
        "Gate 1 never positively observed an ancestor-merge sourcePath here, so this must \
         never be reported as an H3-B-candidate"
    );
    assert_eq!(outcome.terminal, "blocked");

    let _ = std::fs::remove_dir_all(&repo_root);
}
