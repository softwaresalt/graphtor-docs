//! Black-box self-tests for `mcp_probe::transport::run_duplex_pump`.
//!
//! Lives as an integration test (not a `#[cfg(test)]` unit-test module
//! inside `src/transport.rs`) because the self-test fixture technique
//! relies on `env!("CARGO_BIN_EXE_mcp-probe")` to locate the actual
//! compiled `mcp-probe` binary. A unit test's own
//! `std::env::current_exe()` resolves to the `cargo test` harness binary
//! for this crate, not the real dispatch in `main.rs`, so re-exec-based
//! self-tests only work from here.
//!
//! Every helper child spawned below is a hidden, in-crate, platform-portable
//! self-test mode of the probe's own binary (`__echo` / `__block`, wired in
//! `main.rs`) -- never an external OS-specific helper. Every fixture child
//! is reaped through its owned direct `std::process::Child` handle on every
//! outcome (success, assertion failure, or panic/unwind) via the `Drop`-based
//! `TestChildGuard` below, so no self-test in this crate ever leaks a helper
//! process.

use mcp_probe::transport::{run_duplex_pump, CopyHook, Direction, PumpConfig};
use std::io::{Cursor, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Bounded, test-only cleanup for a self-test fixture child: reaps the
/// owned direct `Child` handle on every outcome (success, assertion
/// failure, or panic/unwind) via `Drop`, so no self-test ever leaks a
/// helper process. Scoped strictly to this crate's own transport
/// self-test fixtures -- never a general process-management API.
struct TestChildGuard(Child);

impl Drop for TestChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        // Bounded: `wait()` on an already-killed child returns promptly
        // once the OS reaps the exit status; there is no separate timeout
        // primitive in `std`, but `kill` guarantees termination is already
        // in flight.
        let _ = self.0.wait();
    }
}

fn probe_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mcp-probe")
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

fn spawn_echo_stderr_spam_child(lines: usize) -> TestChildGuard {
    let child = Command::new(probe_bin())
        .arg("__echo")
        .env("MCP_PROBE_TEST_STDERR_SPAM_LINES", lines.to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn stderr-spam echo fixture child");
    TestChildGuard(child)
}

fn spawn_block_child() -> TestChildGuard {
    let child = Command::new(probe_bin())
        .arg("__block")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn block fixture child");
    TestChildGuard(child)
}

/// A `Write` sink that accumulates bytes behind an `Arc<Mutex<_>>` so the
/// test thread can inspect them after the pump completes.
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

#[test]
fn duplex_forwarding_and_half_close_propagate() {
    let mut guard = spawn_echo_child();
    let payload = b"hello mcp-probe transport\n".to_vec();
    let incoming = Cursor::new(payload.clone());
    let outgoing = CapturingWriter::default();
    let outcome = run_duplex_pump(
        incoming,
        outgoing.clone(),
        &mut guard.0,
        &PumpConfig::default(),
        None,
    )
    .expect("duplex pump run");

    assert!(outcome.client_to_child_closed);
    assert!(outcome.child_to_client_closed);
    assert!(!outcome.timed_out);
    assert_eq!(outgoing.snapshot(), payload);
    assert_eq!(outcome.client_to_child_bytes, payload.len() as u64);
    assert_eq!(outcome.child_to_client_bytes, payload.len() as u64);
}

#[test]
fn bounded_buffers_forward_large_payload_under_load() {
    let mut guard = spawn_echo_child();
    // Larger than one pump buffer chunk so multiple read/write cycles are
    // exercised in both directions.
    let payload: Vec<u8> = (0_u32..200_000).map(|i| (i % 251) as u8).collect();
    let incoming = Cursor::new(payload.clone());
    let outgoing = CapturingWriter::default();
    let start = Instant::now();
    let outcome = run_duplex_pump(
        incoming,
        outgoing.clone(),
        &mut guard.0,
        &PumpConfig::default(),
        None,
    )
    .expect("duplex pump run");

    assert_eq!(outgoing.snapshot(), payload);
    assert!(outcome.client_to_child_closed);
    assert!(outcome.child_to_client_closed);
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "large-payload pump should complete quickly, took {:?}",
        start.elapsed()
    );
}

#[test]
fn stderr_is_drained_concurrently_without_blocking_stdout_forwarding() {
    let mut guard = spawn_echo_stderr_spam_child(2_000);
    let payload = b"payload survives stderr spam\n".to_vec();
    let incoming = Cursor::new(payload.clone());
    let outgoing = CapturingWriter::default();
    let start = Instant::now();
    let outcome = run_duplex_pump(
        incoming,
        outgoing.clone(),
        &mut guard.0,
        &PumpConfig::default(),
        None,
    )
    .expect("duplex pump run");

    assert_eq!(outgoing.snapshot(), payload);
    assert!(outcome.child_to_client_closed);
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "stderr spam must never deadlock stdout forwarding, took {:?}",
        start.elapsed()
    );
}

#[test]
fn deadline_signals_without_hanging_on_a_wedged_child() {
    let mut guard = spawn_block_child();
    let incoming = Cursor::new(Vec::<u8>::new());
    let outgoing = CapturingWriter::default();
    let config = PumpConfig {
        buffer_size: 4096,
        deadline: Some(Duration::from_millis(150)),
    };
    let start = Instant::now();
    let outcome =
        run_duplex_pump(incoming, outgoing, &mut guard.0, &config, None).expect("duplex pump run");

    assert!(outcome.timed_out);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "deadline must bound the wait even against a wedged child, took {:?}",
        start.elapsed()
    );
}

#[test]
fn absent_delivery_hook_forwards_stream_unchanged() {
    let mut guard = spawn_echo_child();
    let payload = b"no hook attached\n".to_vec();
    let incoming = Cursor::new(payload.clone());
    let outgoing = CapturingWriter::default();
    let outcome = run_duplex_pump(
        incoming,
        outgoing.clone(),
        &mut guard.0,
        &PumpConfig::default(),
        None,
    )
    .expect("duplex pump run");

    assert_eq!(outgoing.snapshot(), payload);
    assert!(outcome.client_to_child_closed);
    assert!(outcome.child_to_client_closed);
}

#[test]
fn slow_delivery_hook_never_delays_or_alters_forwarded_stream() {
    let mut guard = spawn_echo_child();
    let payload: Vec<u8> = (0_u32..50_000).map(|i| (i % 200) as u8).collect();
    let incoming = Cursor::new(payload.clone());
    let outgoing = CapturingWriter::default();

    let hook_invocations = Arc::new(AtomicUsize::new(0));
    let hook_invocations_for_closure = hook_invocations.clone();
    let hook: CopyHook = Arc::new(move |_direction: Direction, _bytes: &[u8]| {
        hook_invocations_for_closure.fetch_add(1, Ordering::SeqCst);
        // Deliberately slow: proves the bounded, non-blocking delivery
        // channel -- not hook-side good behaviour -- is what keeps the
        // primary pump threads unblocked.
        std::thread::sleep(Duration::from_millis(50));
    });

    let start = Instant::now();
    let outcome = run_duplex_pump(
        incoming,
        outgoing.clone(),
        &mut guard.0,
        &PumpConfig::default(),
        Some(hook),
    )
    .expect("duplex pump run");

    // The forwarded stream must be byte-for-byte unchanged regardless of
    // how slow the diagnostic hook is.
    assert_eq!(outgoing.snapshot(), payload);
    assert!(outcome.client_to_child_closed);
    assert!(outcome.child_to_client_closed);
    // A hook sleeping 50ms per call could not possibly keep up with tens
    // of forwarded chunks; the pump must still finish quickly because
    // delivery is best-effort and non-blocking.
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "a slow hook must never delay the pump, took {:?}",
        start.elapsed()
    );
    // Give the detached delivery worker a moment to process whatever it
    // accepted, then confirm it was invoked at least once (proving the
    // seam is live) without ever having been awaited by the pump.
    std::thread::sleep(Duration::from_millis(200));
    assert!(hook_invocations.load(Ordering::SeqCst) >= 1);
}
