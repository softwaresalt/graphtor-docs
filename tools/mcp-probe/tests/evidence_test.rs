//! Black-box self-tests for `mcp_probe::evidence` (`056.023-T`).
//!
//! Lives as an integration test for the same reason as
//! `transport_test.rs`/`process_test.rs`: fixture children are spawned via
//! `env!("CARGO_BIN_EXE_mcp-probe")`, which only resolves correctly outside
//! the `cargo test` harness binary.
//!
//! `transport_test.rs` already covers, at the transport level: no-hook
//! baseline, a *slow* hook's isolation, deadline behaviour without a hook,
//! half-close, and stderr draining. It does NOT cover a *panicking* hook,
//! JSON-RPC frame reassembly/correlation, redaction, or no-raw-frame
//! persistence -- this file is the new coverage `056.023-T` is responsible
//! for, plus one true end-to-end check that `056.022-T`'s `run_wrapper` is
//! actually wired to this module in production (`process.rs`), not just
//! that the module works in isolation.

use mcp_probe::evidence::{
    evidence_summary_to_json, redact_argv, redact_env, redact_json_value, write_evidence_output,
    EvidenceCollector, FrameKind, REDACTED_PLACEHOLDER,
};
use mcp_probe::process::{run_wrapper, SysinfoProcessObserver, WrapperArgs, WrapperConfig};
use mcp_probe::transport::{run_duplex_pump, CopyHook, Direction, PumpConfig};
use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn probe_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mcp-probe")
}

/// Bounded, test-only cleanup for a self-test fixture child -- same
/// pattern as `transport_test.rs`'s `TestChildGuard`, duplicated here
/// because each integration test file is its own separate compiled crate
/// and cannot share code without a `tests/common/` module.
struct TestChildGuard(Child);

impl Drop for TestChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn_echo_child() -> TestChildGuard {
    let child = Command::new(probe_bin())
        .arg("__echo")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn echo fixture child");
    TestChildGuard(child)
}

#[derive(Clone, Default)]
struct CapturingWriter(Arc<Mutex<Vec<u8>>>);

impl CapturingWriter {
    fn snapshot(&self) -> Vec<u8> {
        self.0.lock().expect("lock capturing writer").clone()
    }
}

impl Write for CapturingWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .expect("lock capturing writer")
            .extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A unique path under the OS temp directory for this test file's own
/// `--evidence-output` targets, so no test here ever writes into the
/// crate's working tree.
fn unique_temp_path(label: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "mcp-probe-evidence-test-{label}-{}-{n}.json",
        std::process::id()
    ))
}

fn json_line(value: &serde_json::Value) -> Vec<u8> {
    let mut bytes = value.to_string().into_bytes();
    bytes.push(b'\n');
    bytes
}

// --- Pure redaction helpers ---

#[test]
fn redact_argv_redacts_only_sensitive_inline_assignments() {
    let argv: Vec<String> = [
        "--token=abc123",
        "--api_key=xyz",
        "--Authorization=Bearer xyz",
        "--host=example.com",
        "--verbose",
        "positional-arg",
        "--name",
        "value",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();

    let redacted = redact_argv(&argv);

    assert_eq!(redacted[0], format!("--token={REDACTED_PLACEHOLDER}"));
    assert_eq!(redacted[1], format!("--api_key={REDACTED_PLACEHOLDER}"));
    assert_eq!(
        redacted[2],
        format!("--Authorization={REDACTED_PLACEHOLDER}"),
        "key case must be preserved; only the value half is replaced"
    );
    assert_eq!(
        redacted[3], "--host=example.com",
        "non-sensitive key unchanged"
    );
    assert_eq!(redacted[4], "--verbose", "bare flag (no '=') unchanged");
    assert_eq!(redacted[5], "positional-arg");
    assert_eq!(
        redacted[6], "--name",
        "space-separated form's flag half is untouched"
    );
    assert_eq!(
        redacted[7], "value",
        "space-separated form's value half is a distinct argv entry and is left alone"
    );
}

#[test]
fn redact_env_redacts_only_sensitive_keys_and_returns_sorted_map() {
    let mut env = BTreeMap::new();
    env.insert("GITHUB_TOKEN".to_string(), "ghp_supersecret".to_string());
    env.insert("PASSWORD".to_string(), "hunter2".to_string());
    env.insert("HOME".to_string(), "/home/probe".to_string());
    env.insert("PATH".to_string(), "/usr/bin".to_string());

    let redacted = redact_env(&env);

    assert_eq!(redacted["GITHUB_TOKEN"], REDACTED_PLACEHOLDER);
    assert_eq!(redacted["PASSWORD"], REDACTED_PLACEHOLDER);
    assert_eq!(redacted["HOME"], "/home/probe");
    assert_eq!(redacted["PATH"], "/usr/bin");

    let keys: Vec<&String> = redacted.keys().collect();
    let mut sorted_keys = keys.clone();
    sorted_keys.sort();
    assert_eq!(keys, sorted_keys, "BTreeMap output is always key-sorted");
}

#[test]
fn redact_json_value_recursively_redacts_nested_sensitive_keys_only() {
    let mut value = serde_json::json!({
        "user": "alice",
        "auth_info": {
            "password": "hunter2",
            "note": "keep-me",
        },
        "items": [
            {"secret": "abc"},
            {"public": "ok"},
        ],
    });

    redact_json_value(&mut value);

    assert_eq!(
        value["user"], "alice",
        "non-sensitive top-level key untouched"
    );
    assert_eq!(value["auth_info"]["password"], REDACTED_PLACEHOLDER);
    assert_eq!(
        value["auth_info"]["note"], "keep-me",
        "sibling non-sensitive key inside a nested object is untouched"
    );
    assert_eq!(value["items"][0]["secret"], REDACTED_PLACEHOLDER);
    assert_eq!(value["items"][1]["public"], "ok");
}

// --- Correlation via direct hook calls (no real child needed) ---

#[test]
fn initialize_request_response_correlated_across_fragmented_and_interleaved_frames() {
    let collector = EvidenceCollector::new("correlation-test-nonce");
    let hook = collector.hook();

    let request = json_line(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "initialize",
        "params": {"token": "SEKRIT-1", "clientInfo": {"name": "probe"}},
    }));
    let notification = json_line(&serde_json::json!({
        "jsonrpc": "2.0",
        "method": "notify/log",
        "params": {"message": "unrelated notification"},
    }));
    let response = json_line(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 7,
        "result": {"protocolVersion": "2024-11-05", "serverInfo": {"apiKey": "SEKRIT-2"}},
    }));
    let garbage = b"not-json-at-all\n".to_vec();

    // Fragment the request across two hook calls (a realistic mid-line
    // split), then deliver an unrelated notification as its own separate
    // line on the same direction -- proving `LineReassembler` handles a
    // partial line split across separate delivered copies correctly.
    let (req_a, req_b) = request.split_at(request.len() / 2);
    hook(Direction::ClientToChild, req_a);
    hook(Direction::ClientToChild, req_b);
    hook(Direction::ClientToChild, &notification);

    // Interleave a non-JSON garbage line on the *other* direction before
    // fragmenting the response across three hook calls -- proving frames
    // from independently-reassembled directions can be freely interleaved
    // relative to each other without disturbing correlation.
    hook(Direction::ChildToClient, &garbage);
    let third = response.len() / 3;
    hook(Direction::ChildToClient, &response[..third]);
    hook(Direction::ChildToClient, &response[third..2 * third]);
    hook(Direction::ChildToClient, &response[2 * third..]);

    let summary = collector.finalize();

    assert!(summary.valid, "no saturation/failure occurred: {summary:?}");
    assert_eq!(
        summary.events.len(),
        4,
        "request+notification+response+garbage"
    );

    let correlation = summary
        .initialize_correlation
        .expect("initialize request/response must correlate despite fragmentation/interleaving");
    assert_eq!(correlation.request_id, serde_json::json!(7));
    assert_eq!(correlation.protocol_version, "2024-11-05");
    assert_eq!(
        correlation
            .redacted_request_params
            .as_ref()
            .and_then(|p| p.get("token"))
            .and_then(|v| v.as_str()),
        Some(REDACTED_PLACEHOLDER),
        "the raw initialize params' sensitive field must be redacted"
    );
    assert_eq!(
        correlation
            .redacted_request_params
            .as_ref()
            .and_then(|p| p.get("clientInfo"))
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str()),
        Some("probe"),
        "non-sensitive nested fields survive redaction unchanged"
    );
    assert_eq!(
        correlation
            .redacted_result
            .as_ref()
            .and_then(|r| r.get("serverInfo"))
            .and_then(|v| v.get("apiKey"))
            .and_then(|v| v.as_str()),
        Some(REDACTED_PLACEHOLDER)
    );

    let kinds: Vec<FrameKind> = summary.events.iter().map(|event| event.kind).collect();
    assert_eq!(
        kinds,
        vec![
            FrameKind::Request,
            FrameKind::Notification,
            FrameKind::Unparseable,
            FrameKind::Response,
        ],
        "events preserve arrival order and are classified correctly across both directions"
    );
    // Sequence numbers are monotonic across both directions.
    for pair in summary.events.windows(2) {
        assert!(pair[0].sequence < pair[1].sequence);
    }
}

#[test]
fn a_response_with_a_different_id_never_correlates_as_initialize() {
    let collector = EvidenceCollector::new("mismatched-id-nonce");
    let hook = collector.hook();

    hook(
        Direction::ClientToChild,
        &json_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {},
        })),
    );
    hook(
        Direction::ChildToClient,
        &json_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "result": {"protocolVersion": "9.9.9"},
        })),
    );

    let summary = collector.finalize();
    assert!(summary.valid);
    assert!(
        summary.initialize_correlation.is_none(),
        "a response id that does not match the pending initialize request id must never correlate"
    );
}

// --- Saturation: bounded channel overflow marks the summary invalid ---

#[test]
fn channel_saturation_marks_summary_invalid_without_affecting_real_forwarding() {
    let collector = EvidenceCollector::new_with_capacity("saturation-nonce", 1);
    let hook = collector.hook();

    // Tight loop, no sleeps: the dedicated correlator thread cannot
    // possibly keep up, so `try_send` must fail at least once against a
    // capacity-1 channel.
    for i in 0..500_u32 {
        hook(Direction::ClientToChild, format!("line-{i}\n").as_bytes());
    }

    // Attach the SAME (already-saturating) hook to a real duplex pump run
    // and prove forwarding is still byte-for-byte correct regardless.
    let mut guard = spawn_echo_child();
    let payload: Vec<u8> = (0_u32..20_000).map(|i| (i % 251) as u8).collect();
    let outcome = run_duplex_pump(
        Cursor::new(payload.clone()),
        CapturingWriter::default(),
        &mut guard.0,
        &PumpConfig::default(),
        Some(hook),
    )
    .expect("duplex pump run with a saturating evidence hook attached");
    assert!(outcome.client_to_child_closed);
    assert!(outcome.child_to_client_closed);

    let summary = collector.finalize();
    assert!(
        !summary.valid,
        "a capacity-1 channel under a 500-call hot loop must saturate"
    );
    assert!(summary
        .invalid_reason
        .as_deref()
        .unwrap_or_default()
        .contains("saturated"));
}

// --- Panicking hook: general transport-level isolation (new vs. transport_test.rs) ---

#[test]
fn a_panicking_copy_hook_never_affects_duplex_forwarding_or_exit_code() {
    let mut guard = spawn_echo_child();
    let payload = b"forwarded despite a hook that panics on every single call\n".to_vec();
    let outgoing = CapturingWriter::default();

    let hook: CopyHook = Arc::new(|_direction: Direction, _bytes: &[u8]| {
        panic!("deliberate: proving hook panics can never affect forwarding");
    });

    let outcome = run_duplex_pump(
        Cursor::new(payload.clone()),
        outgoing.clone(),
        &mut guard.0,
        &PumpConfig::default(),
        Some(hook),
    )
    .expect("duplex pump run with a panicking hook attached");

    assert_eq!(outgoing.snapshot(), payload);
    assert!(outcome.client_to_child_closed);
    assert!(outcome.child_to_client_closed);
}

// --- No raw-frame persistence: only the redacted summary ever reaches JSON/disk ---

#[test]
fn no_raw_frame_persistence_only_redacted_summary_reaches_json_and_disk() {
    let collector = EvidenceCollector::new("no-raw-frame-nonce");
    let hook = collector.hook();

    hook(
        Direction::ClientToChild,
        &json_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {"token": "INIT-SECRET-PARAM"},
        })),
    );
    // A non-initialize frame carrying an embedded marker string under a
    // non-sensitive key -- since this frame is not the initialize
    // request/response pair, this module must record metadata only
    // (kind/method/id/byte_len/digest), never a body copy.
    hook(
        Direction::ClientToChild,
        &json_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notify/log",
            "params": {"message": "RAW_FRAME_MARKER_MUST_NEVER_LEAK"},
        })),
    );
    hook(
        Direction::ChildToClient,
        &json_line(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {"protocolVersion": "1.2.3", "apiKey": "RESULT-SECRET-KEY"},
        })),
    );

    let summary = collector.finalize();
    assert!(summary.valid);

    let json_value = evidence_summary_to_json(&summary);
    let in_memory_text = json_value.to_string();

    assert!(
        !in_memory_text.contains("RAW_FRAME_MARKER_MUST_NEVER_LEAK"),
        "a non-initialize frame body must never be persisted, even redacted-in-place"
    );
    assert!(!in_memory_text.contains("INIT-SECRET-PARAM"));
    assert!(!in_memory_text.contains("RESULT-SECRET-KEY"));
    assert!(
        in_memory_text.contains("1.2.3"),
        "the correlated (non-secret) protocol version must still be present"
    );

    let path = unique_temp_path("no-raw-frame");
    write_evidence_output(&summary, &path).expect("write evidence output");
    let on_disk_text = std::fs::read_to_string(&path).expect("read written evidence file");
    let _ = std::fs::remove_file(&path);

    assert!(!on_disk_text.contains("RAW_FRAME_MARKER_MUST_NEVER_LEAK"));
    assert!(!on_disk_text.contains("INIT-SECRET-PARAM"));
    assert!(!on_disk_text.contains("RESULT-SECRET-KEY"));
    let parsed: serde_json::Value =
        serde_json::from_str(&on_disk_text).expect("written evidence file must be valid JSON");
    assert_eq!(parsed["valid"], true);
    assert_eq!(
        parsed["initialize_correlation"]["protocol_version"],
        "1.2.3"
    );
}

// --- End-to-end: proves 056.022-T's run_wrapper is actually wired to this module ---

#[test]
fn run_wrapper_end_to_end_writes_valid_evidence_output_for_an_echo_child() {
    let observer = SysinfoProcessObserver::new();
    let evidence_output = unique_temp_path("run-wrapper-echo");
    let config = WrapperConfig {
        args: WrapperArgs {
            inner_exe: probe_bin().to_string(),
            inner_args: vec!["__echo".to_string()],
            evidence_output: evidence_output.to_string_lossy().into_owned(),
            run_nonce: "e2e-echo-nonce".to_string(),
        },
        pump_deadline: None,
    };

    let request = json_line(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 42,
        "method": "initialize",
        "params": {},
    }));

    let outcome = run_wrapper(
        Cursor::new(request),
        CapturingWriter::default(),
        &config,
        &observer,
    )
    .expect("wrapper run over __echo with real evidence wiring");

    assert!(
        outcome.evidence_valid,
        "no saturation/failure expected for a single small frame"
    );
    assert!(
        outcome.evidence_write_error.is_none(),
        "evidence write must succeed: {:?}",
        outcome.evidence_write_error
    );

    let on_disk_text =
        std::fs::read_to_string(&evidence_output).expect("run_wrapper must write evidence_output");
    let _ = std::fs::remove_file(&evidence_output);
    let parsed: serde_json::Value =
        serde_json::from_str(&on_disk_text).expect("written evidence file must be valid JSON");
    assert_eq!(parsed["run_nonce"], "e2e-echo-nonce");
    assert_eq!(parsed["valid"], true);
    assert!(
        parsed["events"]
            .as_array()
            .is_some_and(|events| !events.is_empty()),
        "an echoed initialize request must produce at least one recorded event"
    );
    assert!(
        parsed["initialize_correlation"].is_null(),
        "__echo mirrors the request verbatim; it can never produce a real \
         initialize response, so no correlation should form"
    );
}

#[test]
fn run_wrapper_end_to_end_still_writes_evidence_output_when_the_inner_child_is_torn_down_on_deadline(
) {
    let observer = SysinfoProcessObserver::new();
    let evidence_output = unique_temp_path("run-wrapper-block");
    let config = WrapperConfig {
        args: WrapperArgs {
            inner_exe: probe_bin().to_string(),
            inner_args: vec!["__block".to_string()],
            evidence_output: evidence_output.to_string_lossy().into_owned(),
            run_nonce: "e2e-block-nonce".to_string(),
        },
        pump_deadline: Some(Duration::from_millis(150)),
    };

    let request = json_line(&serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {},
    }));

    let outcome = run_wrapper(
        Cursor::new(request),
        CapturingWriter::default(),
        &config,
        &observer,
    )
    .expect("wrapper run over a wedged __block inner child");

    assert!(outcome.pump.timed_out);
    assert_eq!(
        outcome.inner_exit_code, None,
        "a torn-down wedged child reports no preserved exit code"
    );
    assert!(
        outcome.evidence_write_error.is_none(),
        "evidence must still be written even on the deadline teardown path: {:?}",
        outcome.evidence_write_error
    );

    let on_disk_text = std::fs::read_to_string(&evidence_output)
        .expect("run_wrapper must write evidence_output even after a deadline teardown");
    let _ = std::fs::remove_file(&evidence_output);
    let parsed: serde_json::Value =
        serde_json::from_str(&on_disk_text).expect("written evidence file must be valid JSON");
    assert_eq!(parsed["run_nonce"], "e2e-block-nonce");
    assert!(
        parsed["initialize_correlation"].is_null(),
        "a wedged child that never reads stdin can never produce a response"
    );
}
