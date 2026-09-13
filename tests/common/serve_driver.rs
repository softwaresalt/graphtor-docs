//! Reusable out-of-process `graphtor-docs serve` MCP `initialize` handshake
//! test driver (`056.002-T`).
//!
//! This module is intentionally independent of the standalone
//! `tools/mcp-probe` crate (a different crate boundary, built for a
//! different purpose — one-shot exact-CLI classification against the real
//! Copilot CLI) and independent of any classification produced by that
//! crate's tasks. It spawns the *real* `graphtor-docs` binary, drives a
//! hand-rolled, newline-delimited JSON-RPC session over its stdio (this
//! crate's `rmcp` dependency only enables the `server`/`transport-io`
//! features — there is no bundled client transport to reuse), and reports
//! every outcome as a typed, non-panicking value.
//!
//! Two entry points are exposed:
//!
//! - [`run_initialize_handshake`] — the minimal green-path driver: spawn,
//!   send one `initialize` request, validate the response.
//! - [`run_read_only_server_control`] — a fuller session (`initialize` →
//!   `notifications/initialized` → `tools/list` → `tools/call` for
//!   `get_status`) against a target workspace, with an ALWAYS-forced
//!   `--read-only` posture that a caller can never bypass. This mode is
//!   built for a later, out-of-scope consumer ("T4" / `056.011-T`, not a
//!   member of shipment `049-S`); this task only builds and self-tests the
//!   capability.
//!
//! Both entry points share one underlying primitive, [`ServeSession`]: one
//! spawned child, its stdin held open for the whole session, a background
//! thread continuously draining stdout lines (so a fragmented/partial
//! frame can never be observed — [`BufRead::lines`] only ever yields a
//! complete line), and a background thread continuously (bounded-)draining
//! stderr so a chatty child can never deadlock the driver or be
//! misclassified as a hang. Interleaved notifications/requests that do not
//! match the awaited request id are recorded, never treated as a match.
//! Every code path — timeout, broken pipe, child exit, malformed frame —
//! is a diagnostic value, never a panic.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// Absolute cap on captured stderr bytes per session. This is a *bounded*
/// capture (never grows unbounded), not a ring buffer — once the cap is
/// reached, further bytes are still drained (so the pipe never backs up)
/// but simply discarded, and [`SessionShutdown::stderr_truncated`] is set.
const MAX_STDERR_CAPTURE_BYTES: usize = 262_144;

/// Bound on how long [`ServeSession::shutdown`] waits for the killed
/// child to become reapable after `kill()`, mirroring
/// `mcp_probe::process::KILL_WAIT_BUDGET`'s identical bounded-wait
/// rationale in the sibling `tools/mcp-probe` crate: killing a child is
/// normally followed by it becoming reapable almost immediately, but
/// `Child::wait()` on its own carries no such guarantee against every
/// possible wedged-process scenario, and this driver has no deadline of
/// its own to fall back on -- an unconditional, unbounded `wait()` here
/// could hang `shutdown()` (and therefore every test calling it) forever
/// (Copilot review, 2026-09 -- 049-S PR #120, round 5). This root-workspace
/// test crate cannot reuse `mcp_probe::process`'s private helper directly
/// (different crate boundary — see this module's own top-level doc
/// comment), so the same bounded-poll shape is reimplemented locally.
const SHUTDOWN_REAP_BUDGET: Duration = Duration::from_millis(500);

/// Poll interval used while bounding the wait after `kill()` in
/// [`bounded_wait_after_kill`].
const SHUTDOWN_REAP_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Polls [`Child::try_wait`] on `child` until it reports the child has
/// exited, or `budget` elapses, whichever comes first. Never blocks past
/// `budget`. Returns the child's [`ExitStatus`] if it was observed to
/// have exited within budget; `None` if the budget elapsed while the
/// child was still running, or if `try_wait` itself returned an
/// OS-level error (treated as "could not confirm exit" — the caller
/// already discards the underlying `kill`/`wait` errors on both call
/// sites this helper replaces).
fn bounded_wait_after_kill(child: &mut Child, budget: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + budget;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    return None;
                }
                thread::sleep(SHUTDOWN_REAP_POLL_INTERVAL);
            }
            Err(_) => return None,
        }
    }
}

/// Path to the real `graphtor-docs` binary under test, resolved via the
/// Cargo-injected `CARGO_BIN_EXE_graphtor-docs` environment variable.
#[must_use]
pub fn graphtor_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_graphtor-docs"))
}

// ── Wire-level observation types ────────────────────────────────────────

/// One line observed on the child's stdout during a session, plus its
/// parse outcome. A parse failure is a first-class observation (never a
/// panic) — see [`assert_stdout_protocol_clean`].
#[derive(Debug, Clone)]
pub struct ObservedLine {
    /// The raw line, exactly as read from stdout (newline stripped).
    pub raw: String,
    /// `Ok` with the parsed JSON value, or `Err` with a human-readable
    /// parse-failure message.
    pub parsed: Result<Value, String>,
}

/// Diagnostic-only reason a round-trip step did not produce a matching
/// response. Exit, stderr, broken pipe, and timeout are all first-class,
/// non-panicking outcomes.
#[derive(Debug, Clone)]
pub enum DiagnosticReason {
    /// The binary could not even be spawned.
    SpawnFailed(String),
    /// Writing the request to the child's stdin failed (broken pipe).
    BrokenPipeOnWrite(String),
    /// The session's overall bounded deadline elapsed before a matching
    /// response arrived.
    TimedOut,
    /// The child's stdout reader reached EOF (the child closed stdout,
    /// almost always because it exited) before a matching response
    /// arrived.
    StdoutClosed,
}

/// Bounded, non-blocking stderr capture. Continuously drained by a
/// background thread so a chatty child can never deadlock the driver.
#[derive(Debug, Default)]
struct BoundedCapture {
    buf: Vec<u8>,
    truncated: bool,
}

impl BoundedCapture {
    fn push(&mut self, bytes: &[u8]) {
        if self.truncated {
            return;
        }
        let remaining = MAX_STDERR_CAPTURE_BYTES.saturating_sub(self.buf.len());
        if bytes.len() > remaining {
            self.buf.extend_from_slice(&bytes[..remaining]);
            self.truncated = true;
        } else {
            self.buf.extend_from_slice(bytes);
        }
    }
}

// ── Session primitive ────────────────────────────────────────────────────

/// Outcome of one [`ServeSession::await_response`] step.
#[derive(Debug, Clone)]
pub enum StepOutcome {
    /// A JSON-RPC message whose `id` matched the expected request id, and
    /// which carried a `result` or `error` member, was observed. This is a
    /// *transport-level* match only — protocol-level validation (no
    /// `error`, a negotiated `protocolVersion`, etc.) is a separate step,
    /// see [`validate_initialize_response`].
    Matched(Value),
    /// No matching response arrived before the session's overall deadline,
    /// or stdout closed first.
    Diagnostic(DiagnosticReason),
}

/// Final, owned result of tearing a [`ServeSession`] down.
#[derive(Debug, Clone)]
pub struct SessionShutdown {
    /// The child's exit code, if observed.
    pub exit_code: Option<i32>,
    /// Captured stderr (UTF-8, lossily decoded), bounded by
    /// [`MAX_STDERR_CAPTURE_BYTES`].
    pub stderr: String,
    /// Whether the stderr capture was truncated at the bound above.
    pub stderr_truncated: bool,
    /// Every line observed on stdout across the whole session, in order,
    /// including matched responses, interleaved notifications, and any
    /// malformed line.
    pub observed_lines: Vec<ObservedLine>,
}

/// A live out-of-process `graphtor-docs serve` session: one spawned child,
/// its stdin held open for the full session scope, a background thread
/// continuously draining stdout lines, and a background thread
/// continuously (bounded-)draining stderr. Only this exact owned child is
/// ever killed — never a whole-process-tree claim.
pub struct ServeSession {
    child: Child,
    stdin: Option<ChildStdin>,
    line_rx: mpsc::Receiver<String>,
    stdout_thread: Option<JoinHandle<()>>,
    stderr_capture: Arc<Mutex<BoundedCapture>>,
    stderr_thread: Option<JoinHandle<()>>,
    deadline: Instant,
    observed_lines: Vec<ObservedLine>,
}

impl ServeSession {
    /// Spawns `graphtor-docs serve` (plus `extra_args`) in `cwd`, with
    /// piped stdin/stdout/stderr, and starts the continuous stdout-line and
    /// bounded-stderr drain threads. The overall session deadline is fixed
    /// at spawn time and shared by every subsequent
    /// [`Self::await_response`] call.
    ///
    /// # Errors
    ///
    /// Returns a human-readable message when the binary could not be
    /// spawned, or a spawned child unexpectedly lacks a piped handle.
    pub fn spawn(extra_args: &[String], cwd: &Path, timeout: Duration) -> Result<Self, String> {
        let mut command = Command::new(graphtor_bin());
        command.arg("serve");
        command.args(extra_args);
        command.current_dir(cwd);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|err| format!("failed to spawn graphtor-docs serve: {err}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "spawned child has no stdin handle".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "spawned child has no stdout handle".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "spawned child has no stderr handle".to_string())?;

        let (line_tx, line_rx) = mpsc::channel::<String>();
        let stdout_thread = thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(line) => {
                        if line_tx.send(line).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            // Falling off the loop (EOF or a disconnected receiver) drops
            // `line_tx`; the receiver observes that as
            // `RecvTimeoutError::Disconnected`.
        });

        let stderr_capture = Arc::new(Mutex::new(BoundedCapture::default()));
        let stderr_capture_writer = Arc::clone(&stderr_capture);
        let stderr_thread = thread::spawn(move || {
            let mut reader = stderr;
            let mut chunk = [0_u8; 4096];
            loop {
                match reader.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut guard = stderr_capture_writer
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        guard.push(&chunk[..n]);
                    }
                }
            }
        });

        Ok(Self {
            child,
            stdin: Some(stdin),
            line_rx,
            stdout_thread: Some(stdout_thread),
            stderr_capture,
            stderr_thread: Some(stderr_thread),
            deadline: Instant::now() + timeout,
            observed_lines: Vec::new(),
        })
    }

    /// Writes one newline-delimited JSON-RPC message to the child's
    /// stdin. Stdin is held open across the whole session — this never
    /// closes it.
    ///
    /// # Errors
    ///
    /// Returns a human-readable message on a broken pipe / write failure,
    /// or if the session's stdin has already been closed.
    pub fn send(&mut self, message: &Value) -> Result<(), String> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| "session stdin already closed".to_string())?;
        let mut line = message.to_string();
        line.push('\n');
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|err| format!("write to child stdin failed: {err}"))
    }

    /// Blocks (bounded by the session's overall deadline) until a JSON-RPC
    /// message whose `id` equals `expected_id` and which carries a
    /// `result` or `error` member is observed on stdout. Every observed
    /// line — matched, interleaved, or malformed — is recorded in
    /// session-order. A fragmented/partial line can never satisfy this by
    /// construction: the stdout reader thread only ever yields a complete,
    /// newline-terminated line.
    pub fn await_response(&mut self, expected_id: &Value) -> StepOutcome {
        loop {
            let now = Instant::now();
            if now >= self.deadline {
                return StepOutcome::Diagnostic(DiagnosticReason::TimedOut);
            }
            let remaining = self.deadline - now;
            match self.line_rx.recv_timeout(remaining) {
                Ok(raw) => {
                    let parsed = serde_json::from_str::<Value>(&raw).map_err(|e| e.to_string());
                    let matched_value = match &parsed {
                        Ok(value)
                            if value.get("id") == Some(expected_id)
                                && (value.get("result").is_some()
                                    || value.get("error").is_some()) =>
                        {
                            Some(value.clone())
                        }
                        _ => None,
                    };
                    self.observed_lines.push(ObservedLine { raw, parsed });
                    if let Some(value) = matched_value {
                        return StepOutcome::Matched(value);
                    }
                    // Interleaved notification/request or a malformed
                    // line: recorded above, never satisfies this call.
                }
                Err(RecvTimeoutError::Timeout) => {
                    return StepOutcome::Diagnostic(DiagnosticReason::TimedOut);
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return StepOutcome::Diagnostic(DiagnosticReason::StdoutClosed);
                }
            }
        }
    }

    /// Cancellation-safe teardown: kills and reaps ONLY this exact owned
    /// child (never a whole-process-tree claim), joins the drain threads,
    /// and returns the exit code (if observed), the bounded stderr
    /// capture, and every observed stdout line. Always called exactly
    /// once, after a matched response or any diagnostic outcome —
    /// shutdown follows the response/diagnostic, it never races it.
    pub fn shutdown(mut self) -> SessionShutdown {
        let exit_code = match self.child.try_wait() {
            Ok(Some(status)) => status.code(),
            Ok(None) => {
                let _ = self.child.kill();
                bounded_wait_after_kill(&mut self.child, SHUTDOWN_REAP_BUDGET)
                    .and_then(|status| status.code())
            }
            Err(_) => {
                // Copilot review thread E (PR #120, round 2): an OS
                // error from `try_wait()` proves NOTHING about whether
                // the child is still alive holding its own end of the
                // stdout/stderr pipes open -- it must never be treated
                // as equivalent to an observed exit. Fall back to the
                // exact same defensive kill()+bounded-wait the `Ok(None)`
                // (still-running) branch above already performs, so the
                // drain threads joined below are still guaranteed to
                // see EOF instead of potentially hanging forever on a
                // child that never noticed stdin's EOF on its own.
                // `kill()` on an already-exited child is a harmless
                // no-op (an `Err` is simply discarded, exactly as the
                // sibling branch above does). The wait itself is bounded
                // too: an unconditional, unbounded `wait()` here could
                // hang this whole call forever if the child closes its
                // pipes without ever actually terminating (Copilot
                // review, 2026-09 -- 049-S PR #120, round 5).
                let _ = self.child.kill();
                bounded_wait_after_kill(&mut self.child, SHUTDOWN_REAP_BUDGET)
                    .and_then(|status| status.code())
            }
        };

        // Dropping stdin closes the pipe; the drain threads exit once
        // their respective pipes close (which `kill()` above guarantees
        // even if the child never exits on its own, including on the
        // `try_wait()` OS-error path).
        drop(self.stdin.take());
        if let Some(handle) = self.stdout_thread.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.stderr_thread.take() {
            let _ = handle.join();
        }

        let (stderr, stderr_truncated) = {
            let guard = self
                .stderr_capture
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (
                String::from_utf8_lossy(&guard.buf).into_owned(),
                guard.truncated,
            )
        };

        SessionShutdown {
            exit_code,
            stderr,
            stderr_truncated,
            observed_lines: std::mem::take(&mut self.observed_lines),
        }
    }
}

// ── JSON-RPC message builders ───────────────────────────────────────────
//
// Wire shapes below match exactly what `rmcp` 1.5 itself round-trips (see
// `rmcp::model`'s `InitializeRequestParams`/`ListToolsRequestMethod`
// (`"tools/list"`)/`CallToolRequestParams` and its own
// `test_initial_request_response_serde` wire fixture) — this driver
// hand-rolls the frames rather than depending on an `rmcp` client
// transport (not enabled by this workspace's `rmcp` feature set), but the
// shapes themselves are not guessed.

/// Standard MCP `initialize` request JSON-RPC message.
#[must_use]
pub fn build_initialize_request(id: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "initialize",
        "params": {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": {
                "name": "graphtor-serve-handshake-driver",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

/// The `notifications/initialized` message a client sends after a
/// successful `initialize` round trip. Carries no `id` and expects no
/// response.
#[must_use]
pub fn build_initialized_notification() -> Value {
    json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    })
}

/// `tools/list` request JSON-RPC message.
#[must_use]
pub fn build_tools_list_request(id: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/list"
    })
}

/// `tools/call` request JSON-RPC message for tool `name` with `arguments`.
#[must_use]
pub fn build_tool_call_request(id: &Value, name: &str, arguments: &Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": name,
            "arguments": arguments
        }
    })
}

// ── Initialize response validation ──────────────────────────────────────

/// A successfully validated `initialize` response.
#[derive(Debug, Clone)]
pub struct InitializeSuccess {
    /// The server's negotiated `result.protocolVersion`.
    pub protocol_version: String,
    /// The server's `result.capabilities`, if present (else `Value::Null`).
    pub capabilities: Value,
    /// The server's `result.serverInfo`, if present (else `Value::Null`).
    pub server_info: Value,
}

/// Validates a transport-matched `initialize` response against this task's
/// exact success bar: `jsonrpc: "2.0"`, the correlated `id`, no `error`
/// member, and a negotiated `result.protocolVersion`.
///
/// # Errors
///
/// Returns a human-readable reason when any of those checks fails.
pub fn validate_initialize_response(
    value: &Value,
    expected_id: &Value,
) -> Result<InitializeSuccess, String> {
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(format!("response missing/invalid jsonrpc member: {value}"));
    }
    if value.get("id") != Some(expected_id) {
        return Err(format!("response id does not match request id: {value}"));
    }
    if let Some(error) = value.get("error") {
        return Err(format!("response carried an error member: {error}"));
    }
    let result = value
        .get("result")
        .ok_or_else(|| format!("response has neither result nor error: {value}"))?;
    let protocol_version = result
        .get("protocolVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("result missing protocolVersion: {result}"))?
        .to_string();
    Ok(InitializeSuccess {
        protocol_version,
        capabilities: result.get("capabilities").cloned().unwrap_or(Value::Null),
        server_info: result.get("serverInfo").cloned().unwrap_or(Value::Null),
    })
}

/// Asserts every line observed on stdout during a session parsed as valid
/// JSON — i.e. stdout stayed protocol-clean and nothing else was ever
/// printed to it. Reusable by any caller (e.g. `056.003-T`'s own tests)
/// that wants to prove a change adds no stray stdout bytes.
///
/// # Panics
///
/// Panics (via a descriptive `assert!`) on the first line that failed to
/// parse as JSON.
pub fn assert_stdout_protocol_clean(observed_lines: &[ObservedLine]) {
    for (index, line) in observed_lines.iter().enumerate() {
        assert!(
            line.parsed.is_ok(),
            "stdout carried a non-JSON-RPC line at position {index} during the session \
             (parity-control violation): {:?}",
            line.raw
        );
    }
}

// ── Entry point 1: minimal initialize handshake ─────────────────────────

/// Outcome of [`run_initialize_handshake`].
#[derive(Debug, Clone)]
pub enum HandshakeOutcome {
    /// `initialize` succeeded and passed [`validate_initialize_response`].
    Initialized(InitializeSuccess),
    /// A response was transport-matched but failed protocol validation
    /// (carried an error, missing `protocolVersion`, etc.).
    InvalidResponse {
        /// The raw matched response.
        raw: Value,
        /// Why validation failed.
        reason: String,
    },
    /// No matching response ever arrived.
    NoResponse(DiagnosticReason),
}

/// Full report from [`run_initialize_handshake`], including every observed
/// stdout line (for parity-control assertions via
/// [`assert_stdout_protocol_clean`]) and the captured stderr/exit code.
#[derive(Debug, Clone)]
pub struct HandshakeReport {
    /// The handshake outcome.
    pub outcome: HandshakeOutcome,
    /// Every line observed on stdout during the session, in order.
    pub observed_lines: Vec<ObservedLine>,
    /// Captured stderr (bounded, lossily decoded).
    pub stderr: String,
    /// Whether the stderr capture was truncated.
    pub stderr_truncated: bool,
    /// The child's exit code, if observed.
    pub exit_code: Option<i32>,
}

/// Spawns `graphtor-docs serve` (plus `extra_args`) in `cwd`, sends one
/// valid `initialize` request, and reports the outcome. This driver holds
/// `ChildStdin` open for the full request/response scope, drains stderr
/// concurrently via a bounded capture (never deadlocks even if the child
/// writes beyond one pipe buffer), and enforces the given bounded
/// `timeout` for the whole operation. This entry point always ends with a
/// typed report — never a panic, and never a branch-specific failing
/// assertion (that is downstream, curative-task scope).
#[must_use]
pub fn run_initialize_handshake(
    cwd: &Path,
    extra_args: &[String],
    timeout: Duration,
) -> HandshakeReport {
    let id = json!(1);

    let mut session = match ServeSession::spawn(extra_args, cwd, timeout) {
        Ok(session) => session,
        Err(err) => {
            return HandshakeReport {
                outcome: HandshakeOutcome::NoResponse(DiagnosticReason::SpawnFailed(err)),
                observed_lines: Vec::new(),
                stderr: String::new(),
                stderr_truncated: false,
                exit_code: None,
            };
        }
    };

    let outcome = match session.send(&build_initialize_request(&id)) {
        Ok(()) => match session.await_response(&id) {
            StepOutcome::Matched(value) => match validate_initialize_response(&value, &id) {
                Ok(success) => HandshakeOutcome::Initialized(success),
                Err(reason) => HandshakeOutcome::InvalidResponse { raw: value, reason },
            },
            StepOutcome::Diagnostic(reason) => HandshakeOutcome::NoResponse(reason),
        },
        Err(err) => HandshakeOutcome::NoResponse(DiagnosticReason::BrokenPipeOnWrite(err)),
    };

    let shutdown = session.shutdown();
    HandshakeReport {
        outcome,
        observed_lines: shutdown.observed_lines,
        stderr: shutdown.stderr,
        stderr_truncated: shutdown.stderr_truncated,
        exit_code: shutdown.exit_code,
    }
}

// ── Entry point 2: read-only production-workspace "server control" ─────

/// The literal ban-list of stderr substrings that indicate a write-capable
/// open path or an active background sync task — the SAME log points
/// `open_serve_databases`/the background-sync spawn already emit (see
/// `tests/serve_posture_gating_test.rs`). Observing either during a
/// server-control session is a read-only boundary violation and fails the
/// control closed, regardless of the forced `--read-only` flag below.
/// (`"opened SQLite DataStore"` — the write-mode `DataStore::open_sqlite`
/// log text — is never a substring of either read-only open-path message,
/// `"opened read-only SQLite DataStore"` or `"opened engine-enforced
/// read-only SQLite DataStore"`, so a plain substring match is
/// unambiguous here.)
const WRITE_PATH_STDERR_MARKERS: &[&str] =
    &["opened SQLite DataStore", "background sync task spawned"];

/// Returns `true` if `stderr` shows evidence of a write-capable open path
/// or an active background sync task.
#[must_use]
fn stderr_shows_write_path(stderr: &str) -> bool {
    WRITE_PATH_STDERR_MARKERS
        .iter()
        .any(|marker| stderr.contains(marker))
}

/// A successfully completed read-only server-control session.
#[derive(Debug, Clone)]
pub struct ServerControlSuccess {
    /// The validated `initialize` response.
    pub initialize: InitializeSuccess,
    /// Tool names returned by `tools/list`.
    pub tool_names: Vec<String>,
    /// The text content of the `get_status` tool result.
    pub get_status_text: String,
}

/// A single stage-tagged failure inside a server-control session.
#[derive(Debug, Clone)]
struct ControlStageError {
    stage: &'static str,
    detail: String,
}

/// Outcome of [`run_read_only_server_control`].
#[derive(Debug, Clone)]
pub enum ServerControlOutcome {
    /// `initialize` → `notifications/initialized` → `tools/list` →
    /// `tools/call(get_status)` all completed, and no write-path stderr
    /// marker was ever observed.
    Control(ServerControlSuccess),
    /// The read-only boundary held, but some step of the control session
    /// failed or returned an invalid response — the boundary is fine, the
    /// control session itself did not complete.
    Diagnostic {
        /// Which stage failed (e.g. `"initialize:await"`).
        stage: &'static str,
        /// Human-readable detail.
        detail: String,
    },
    /// The read-only boundary itself was violated: a write-path stderr
    /// marker was observed despite the always-forced `--read-only` flag.
    /// Fails closed regardless of any other outcome.
    BoundaryViolated {
        /// The captured stderr evidence.
        evidence: String,
    },
}

/// Full report from [`run_read_only_server_control`].
#[derive(Debug, Clone)]
pub struct ServerControlReport {
    /// The control-session outcome.
    pub outcome: ServerControlOutcome,
    /// Every line observed on stdout during the session, in order.
    pub observed_lines: Vec<ObservedLine>,
    /// Captured stderr (bounded, lossily decoded).
    pub stderr: String,
    /// Whether the stderr capture was truncated.
    pub stderr_truncated: bool,
    /// The child's exit code, if observed.
    pub exit_code: Option<i32>,
}

/// Runs the read-only production-workspace "server control" session
/// intended for a later, out-of-scope consumer ("T4" / `056.011-T` — not a
/// member of shipment `049-S`). ALWAYS forces `--read-only` on the spawned
/// server; the caller supplies only `cwd` and `timeout`, so the read-only
/// boundary can never be bypassed or contradicted by a caller mistake.
/// This mode targets a `ReadOnly`/auto-discovered posture only — it never
/// requests, and would always override, a `Generation` (write-capable)
/// classification for any target the workspace resolves.
///
/// The boundary is additionally *proven*, not just assumed: the captured
/// stderr is scanned for the exact literal write-path log markers the main
/// crate already emits (see [`stderr_shows_write_path`]); observing one
/// fails the control closed even if every JSON-RPC step otherwise
/// succeeded (a boundary violation always takes precedence over a
/// completed session).
#[must_use]
pub fn run_read_only_server_control(cwd: &Path, timeout: Duration) -> ServerControlReport {
    let extra_args = vec!["--read-only".to_string()];
    let mut session = match ServeSession::spawn(&extra_args, cwd, timeout) {
        Ok(session) => session,
        Err(err) => {
            return ServerControlReport {
                outcome: ServerControlOutcome::Diagnostic {
                    stage: "spawn",
                    detail: err,
                },
                observed_lines: Vec::new(),
                stderr: String::new(),
                stderr_truncated: false,
                exit_code: None,
            };
        }
    };

    let result = run_control_steps(&mut session);
    let shutdown = session.shutdown();

    let outcome = if stderr_shows_write_path(&shutdown.stderr) {
        ServerControlOutcome::BoundaryViolated {
            evidence: shutdown.stderr.clone(),
        }
    } else if shutdown.stderr_truncated {
        // The write-path boundary check above can only scan the bytes
        // actually retained by the bounded stderr capture (see
        // `BoundedCapture`/`MAX_STDERR_CAPTURE_BYTES`); once that cap is
        // hit, any write-path marker emitted after the cutoff is
        // silently unobservable to `stderr_shows_write_path`. This
        // function's own doc contract is that a boundary violation
        // "always fails the control closed", which is only true if the
        // check actually saw the full stream -- so a truncated capture
        // must be reported as indeterminate, never allowed to fall
        // through to a `Control` success.
        ServerControlOutcome::Diagnostic {
            stage: "boundary-check",
            detail: "stderr capture was truncated at the bounded cap; the read-only \
                     boundary claim cannot be verified for the untruncated remainder \
                     of the session"
                .to_string(),
        }
    } else {
        match result {
            Ok(success) => ServerControlOutcome::Control(success),
            Err(ControlStageError { stage, detail }) => {
                ServerControlOutcome::Diagnostic { stage, detail }
            }
        }
    };

    ServerControlReport {
        outcome,
        observed_lines: shutdown.observed_lines,
        stderr: shutdown.stderr,
        stderr_truncated: shutdown.stderr_truncated,
        exit_code: shutdown.exit_code,
    }
}

/// Runs the `initialize` → `notifications/initialized` → `tools/list` →
/// `tools/call(get_status)` step sequence against an already-spawned
/// [`ServeSession`]. Split out from [`run_read_only_server_control`] so
/// each stage can be tagged with `?` via [`ControlStageError`] rather than
/// nested match arms.
fn run_control_steps(
    session: &mut ServeSession,
) -> Result<ServerControlSuccess, ControlStageError> {
    let stage_err = |stage: &'static str, detail: String| ControlStageError { stage, detail };

    let init_id = json!(1);
    session
        .send(&build_initialize_request(&init_id))
        .map_err(|detail| stage_err("initialize:send", detail))?;
    let init_value = match session.await_response(&init_id) {
        StepOutcome::Matched(value) => value,
        StepOutcome::Diagnostic(reason) => {
            return Err(stage_err("initialize:await", format!("{reason:?}")))
        }
    };
    let initialize = validate_initialize_response(&init_value, &init_id)
        .map_err(|detail| stage_err("initialize:validate", detail))?;

    session
        .send(&build_initialized_notification())
        .map_err(|detail| stage_err("initialized:send", detail))?;

    let list_id = json!(2);
    session
        .send(&build_tools_list_request(&list_id))
        .map_err(|detail| stage_err("tools/list:send", detail))?;
    let list_value = match session.await_response(&list_id) {
        StepOutcome::Matched(value) => value,
        StepOutcome::Diagnostic(reason) => {
            return Err(stage_err("tools/list:await", format!("{reason:?}")))
        }
    };
    if let Some(error) = list_value.get("error") {
        return Err(stage_err(
            "tools/list:validate",
            format!("tools/list returned an error: {error}"),
        ));
    }
    let tool_names: Vec<String> = list_value
        .get("result")
        .and_then(|result| result.get("tools"))
        .and_then(Value::as_array)
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool.get("name").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .ok_or_else(|| {
            stage_err(
                "tools/list:validate",
                format!("result.tools missing/malformed: {list_value}"),
            )
        })?;

    let call_id = json!(3);
    session
        .send(&build_tool_call_request(&call_id, "get_status", &json!({})))
        .map_err(|detail| stage_err("tools/call:send", detail))?;
    let call_value = match session.await_response(&call_id) {
        StepOutcome::Matched(value) => value,
        StepOutcome::Diagnostic(reason) => {
            return Err(stage_err("tools/call:await", format!("{reason:?}")))
        }
    };
    if let Some(error) = call_value.get("error") {
        return Err(stage_err(
            "tools/call:validate",
            format!("get_status returned an error: {error}"),
        ));
    }
    let get_status_text = call_value
        .get("result")
        .and_then(|result| result.get("content"))
        .and_then(Value::as_array)
        .and_then(|content| content.first())
        .and_then(|first| first.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            stage_err(
                "tools/call:validate",
                format!("result.content[0].text missing: {call_value}"),
            )
        })?;

    Ok(ServerControlSuccess {
        initialize,
        tool_names,
        get_status_text,
    })
}
