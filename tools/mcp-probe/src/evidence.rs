//! Copy-only, read-only observation, JSON-RPC `initialize` correlation,
//! and redacted evidence-summary capture for the standalone `mcp-probe`
//! diagnostic crate (`056.023-T`), composed onto the `056.020-T`
//! transport's post-write, bounded, non-blocking copy-delivery hook
//! ([`crate::transport::CopyHook`]) and executed INSIDE the `056.022-T`
//! wrapper process (see [`crate::process::run_wrapper`]).
//!
//! # Ordering and isolation guarantee
//!
//! [`crate::transport::run_duplex_pump`] already writes-and-flushes each
//! forwarded chunk on its owning pump thread BEFORE ever delivering a
//! copy to any hook, and it invokes hooks from a single dedicated
//! delivery thread that is never one of the two pump threads. This
//! module's [`EvidenceCollector::hook`] is deliberately as cheap as
//! possible (an owned byte-vec clone plus one non-blocking channel send)
//! so it can never itself become the slow part of that already-isolated
//! seam; the real newline-reassembly / JSON-RPC parsing / correlation
//! work happens on a SEPARATE dedicated thread owned by this module,
//! decoupled from transport's delivery thread by this module's OWN
//! bounded channel. This means an arbitrarily slow or even fully wedged
//! correlator can only ever fill up this module's own channel -- it can
//! never delay, reorder, or block byte forwarding in either direction,
//! and it never takes a cross-direction lock that the pump could
//! contend on.
//!
//! # Failure and saturation semantics
//!
//! On channel saturation (this module's own bounded channel is full) or
//! an internal correlator failure (a caught panic while processing one
//! copy), the returned [`EvidenceSummary`] is atomically marked
//! `valid: false` with a human-readable reason. Forwarding itself is
//! completely unaffected either way -- this seam has no way to slow,
//! alter, or block the wire.
//!
//! # No raw-frame persistence
//!
//! Raw frame bytes never leave wrapper memory. Only a redacted,
//! structured [`EvidenceSummary`] -- carrying the `initialize`
//! correlation (with redacted `params`/`result` copies), lightweight
//! per-frame metadata (kind / method / id / byte length / a non-secure
//! content digest), and a validity flag -- is ever written to the
//! wrapper-owned `--evidence-output` file, via [`write_evidence_output`].
//! Every JSON value this module parses or builds uses `serde_json`
//! directly (`Value` / the `json!` macro) rather than `#[derive(Serialize)]`,
//! so this task's only new dependency is the standalone `serde_json`
//! crate -- no `serde` derive dependency is introduced.

use crate::transport::{CopyHook, Direction};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::io::Write as _;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Default bounded capacity of this module's own copy-delivery channel,
/// fully separate from `056.020-T` transport's internal delivery
/// channel. A dedicated channel (rather than doing correlation work
/// directly inside the hook on transport's shared delivery thread) is
/// what makes saturation observable and testable at all: transport's own
/// internal channel silently discards on overflow with no signal to any
/// hook, so a correlator sharing that channel could never detect or
/// report a drop.
const DEFAULT_EVIDENCE_CHANNEL_CAPACITY: usize = 256;

/// How often the dedicated correlator thread checks the shutdown flag
/// between blocking waits for the next copy.
const WORKER_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Bounded, best-effort grace period `finalize` waits before signaling
/// shutdown. `056.020-T`'s transport deliberately never joins its own
/// delivery thread (so a slow hook can never delay pump completion),
/// which means a handful of already-in-flight trailing copies MAY still
/// be delivered to this module's hook for a brief window after
/// `run_duplex_pump` itself has already returned. This is NOT a
/// synchronization guarantee -- it is a short, documented window to let
/// that small, already-bounded backlog (transport's own delivery channel
/// capacity) drain through a fast hook before this collector stops
/// listening. Evidence completeness is always best-effort; forwarding
/// correctness never depends on it.
const FINALIZE_GRACE_PERIOD: Duration = Duration::from_millis(50);

/// Case-insensitive substrings marking an argv/env/JSON key as
/// secret-bearing for redaction purposes. Intentionally conservative
/// (over-redact rather than under-redact) since this is diagnostic
/// evidence, not a wire-protocol concern.
const SENSITIVE_KEY_SUBSTRINGS: &[&str] = &[
    "token",
    "secret",
    "password",
    "passwd",
    "credential",
    "authorization",
    "apikey",
    "api_key",
    "access_key",
    "private_key",
    "cookie",
];

/// Placeholder substituted for any redacted value.
pub const REDACTED_PLACEHOLDER: &str = "<redacted>";

fn is_sensitive_key(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SENSITIVE_KEY_SUBSTRINGS
        .iter()
        .any(|needle| lower.contains(needle))
}

/// Redacts `--name=value`-style argv entries whose `name` matches a
/// sensitive key substring, replacing only the value half. Bare flags,
/// positional arguments, and space-separated `--name value` pairs (where
/// the value is a distinct argv entry) are left unchanged -- this task
/// covers the common inline-assignment form only, which is the form
/// `056.022-T`'s own wrapper argv contract actually uses
/// (`--inner-arg=...` style is NOT currently used by the wrapper's own
/// parser, but downstream inner-process argv forwarded verbatim through
/// `--inner-arg` may use it).
#[must_use]
pub fn redact_argv(argv: &[String]) -> Vec<String> {
    argv.iter()
        .map(|arg| match arg.split_once('=') {
            Some((name, _value)) if is_sensitive_key(name) => {
                format!("{name}={REDACTED_PLACEHOLDER}")
            }
            _ => arg.clone(),
        })
        .collect()
}

/// Redacts environment-variable values whose key matches a sensitive key
/// substring. Returns a `BTreeMap` (rather than the input's original map
/// type) so a persisted summary is always deterministically ordered.
#[must_use]
pub fn redact_env(env: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    env.iter()
        .map(|(key, value)| {
            if is_sensitive_key(key) {
                (key.clone(), REDACTED_PLACEHOLDER.to_string())
            } else {
                (key.clone(), value.clone())
            }
        })
        .collect()
}

/// Recursively redacts a parsed JSON value in place: any object member
/// whose key matches a sensitive key substring has its value replaced
/// with [`REDACTED_PLACEHOLDER`] regardless of its original type; arrays
/// and nested objects are walked recursively. Never adds, removes, or
/// reorders keys -- only ever replaces sensitive values.
pub fn redact_json_value(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, entry) in map.iter_mut() {
                if is_sensitive_key(key) {
                    *entry = serde_json::Value::String(REDACTED_PLACEHOLDER.to_string());
                } else {
                    redact_json_value(entry);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items.iter_mut() {
                redact_json_value(item);
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
}

/// Classification of one observed, newline-delimited JSON-RPC frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// Has both `method` and `id`.
    Request,
    /// Has `id` and (`result` or `error`), no `method`.
    Response,
    /// Has `method`, no `id`.
    Notification,
    /// Not valid UTF-8, not valid JSON, or none of the shapes above.
    Unparseable,
}

/// Lightweight, non-secret metadata recorded for one observed frame.
/// Deliberately excludes the frame's full body -- only the
/// `initialize` request/response pair gets a (redacted) body copy, via
/// [`InitializeCorrelation`], since that is this module's one
/// substantive correlation responsibility; every other frame is recorded
/// as metadata only, never persisted verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameEvent {
    pub direction: Direction,
    /// Monotonically increasing sequence number across both directions,
    /// assigned in the order lines were reassembled (not necessarily wall
    /// -clock order between directions, but stable and unique).
    pub sequence: u64,
    pub kind: FrameKind,
    pub method: Option<String>,
    pub id: Option<serde_json::Value>,
    pub byte_len: usize,
    /// A std-only, non-cryptographic 64-bit content digest
    /// (`DefaultHasher`/SipHash-1-3) of the raw line bytes, formatted as
    /// lowercase hex -- sufficient for diagnostic identity comparison
    /// between observed frames. This is NOT a security digest and must
    /// never be represented as one.
    pub digest_hex: String,
}

/// Redacted correlation of the exact `initialize` request id to a
/// `jsonrpc: "2.0"` non-error `result.protocolVersion`.
#[derive(Debug, Clone, PartialEq)]
pub struct InitializeCorrelation {
    pub request_id: serde_json::Value,
    pub protocol_version: String,
    /// Redacted copy of the `initialize` request's `params`, if present.
    pub redacted_request_params: Option<serde_json::Value>,
    /// Redacted copy of the correlated response's `result` (the same
    /// value `protocol_version` was extracted from).
    pub redacted_result: Option<serde_json::Value>,
}

/// The complete, redacted, persistable evidence summary for one wrapper
/// run. Never carries a raw frame body beyond the `initialize`
/// correlation's redacted copies.
// The `Evidence` prefix is deliberate and clearer than a bare `Summary`
// for a type re-exported from the crate root's public API surface.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceSummary {
    pub run_nonce: String,
    pub valid: bool,
    pub invalid_reason: Option<String>,
    pub initialize_correlation: Option<InitializeCorrelation>,
    pub events: Vec<FrameEvent>,
}

/// Incrementally reassembles a byte stream into newline-delimited lines.
/// Any bytes after the final unterminated fragment remain buffered for
/// the next call -- this is exactly what lets fragmented/partial frames
/// (a single JSON-RPC line split across multiple delivered copies) be
/// handled correctly.
#[derive(Default)]
struct LineReassembler {
    pending: Vec<u8>,
}

impl LineReassembler {
    /// Appends `chunk` and returns any newly completed lines, in order,
    /// with the trailing `\n` (and a preceding `\r`, if present) removed.
    fn push(&mut self, chunk: &[u8]) -> Vec<Vec<u8>> {
        self.pending.extend_from_slice(chunk);
        let mut lines = Vec::new();
        while let Some(pos) = self.pending.iter().position(|&byte| byte == b'\n') {
            let mut line: Vec<u8> = self.pending.drain(..=pos).collect();
            line.pop(); // trailing '\n'
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            lines.push(line);
        }
        lines
    }
}

fn content_digest_hex(bytes: &[u8]) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Mutable state owned exclusively by this module's dedicated correlator
/// thread (see [`EvidenceCollector::new_with_capacity`]); never shared
/// or locked by the pump threads or transport's own delivery thread.
struct CollectorState {
    valid: bool,
    invalid_reason: Option<String>,
    sequence: u64,
    client_to_child_lines: LineReassembler,
    child_to_client_lines: LineReassembler,
    pending_initialize_request_id: Option<serde_json::Value>,
    pending_initialize_params: Option<serde_json::Value>,
    initialize_correlation: Option<InitializeCorrelation>,
    events: Vec<FrameEvent>,
}

impl CollectorState {
    fn new() -> Self {
        Self {
            valid: true,
            invalid_reason: None,
            sequence: 0,
            client_to_child_lines: LineReassembler::default(),
            child_to_client_lines: LineReassembler::default(),
            pending_initialize_request_id: None,
            pending_initialize_params: None,
            initialize_correlation: None,
            events: Vec::new(),
        }
    }

    fn mark_invalid(&mut self, reason: &str) {
        self.valid = false;
        self.invalid_reason
            .get_or_insert_with(|| reason.to_string());
    }
}

/// Processes one already-reassembled, newline-delimited line observed in
/// `direction`: classifies it, records a [`FrameEvent`], and -- for the
/// `initialize` request/response pair only -- captures a redacted body
/// copy for [`InitializeCorrelation`]. Never affects wire bytes; this
/// function only ever reads `line`, it never persists it verbatim.
fn process_line(state: &mut CollectorState, direction: Direction, line: &[u8]) {
    state.sequence += 1;
    let sequence = state.sequence;
    let byte_len = line.len();
    let digest_hex = content_digest_hex(line);

    let parsed = std::str::from_utf8(line)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());

    let Some(value) = parsed else {
        state.events.push(FrameEvent {
            direction,
            sequence,
            kind: FrameKind::Unparseable,
            method: None,
            id: None,
            byte_len,
            digest_hex,
        });
        return;
    };

    let method = value
        .get("method")
        .and_then(|method| method.as_str())
        .map(str::to_owned);
    let id = value.get("id").cloned();
    let has_result_or_error = value.get("result").is_some() || value.get("error").is_some();

    let kind = if method.is_some() && id.is_some() {
        FrameKind::Request
    } else if method.is_some() {
        FrameKind::Notification
    } else if id.is_some() && has_result_or_error {
        FrameKind::Response
    } else {
        FrameKind::Unparseable
    };

    if kind == FrameKind::Request && method.as_deref() == Some("initialize") {
        state.pending_initialize_request_id.clone_from(&id);
        state.pending_initialize_params = value.get("params").cloned().map(|mut params| {
            redact_json_value(&mut params);
            params
        });
    }

    if kind == FrameKind::Response && state.initialize_correlation.is_none() {
        if let (Some(expected_id), Some(observed_id)) =
            (state.pending_initialize_request_id.as_ref(), id.as_ref())
        {
            let jsonrpc_ok = value.get("jsonrpc").and_then(|v| v.as_str()) == Some("2.0");
            let is_error = value.get("error").is_some();
            if expected_id == observed_id && jsonrpc_ok && !is_error {
                if let Some(protocol_version) = value
                    .get("result")
                    .and_then(|result| result.get("protocolVersion"))
                    .and_then(|version| version.as_str())
                {
                    let mut redacted_result = value.get("result").cloned();
                    if let Some(result) = redacted_result.as_mut() {
                        redact_json_value(result);
                    }
                    state.initialize_correlation = Some(InitializeCorrelation {
                        request_id: expected_id.clone(),
                        protocol_version: protocol_version.to_string(),
                        redacted_request_params: state.pending_initialize_params.clone(),
                        redacted_result,
                    });
                }
            }
        }
    }

    state.events.push(FrameEvent {
        direction,
        sequence,
        kind,
        method,
        id,
        byte_len,
        digest_hex,
    });
}

/// In-wrapper, copy-only observer: reassembles and correlates the
/// `initialize` handshake over a `056.020-T` transport's copy-delivery
/// hook, using its own dedicated bounded channel and correlator thread
/// (see module docs for why). Construct one per wrapper run
/// ([`EvidenceCollector::new`]), attach [`EvidenceCollector::hook`] to
/// [`crate::transport::run_duplex_pump`]'s `copy_hook` parameter, and
/// call [`EvidenceCollector::finalize`] once the pump has returned.
// The `Evidence` prefix is deliberate and clearer than a bare `Collector`
// for a type re-exported from the crate root's public API surface.
#[allow(clippy::module_name_repetitions)]
pub struct EvidenceCollector {
    tx: SyncSender<(Direction, Vec<u8>)>,
    state: Arc<Mutex<CollectorState>>,
    run_nonce: String,
    shutdown: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl EvidenceCollector {
    /// Constructs a collector with the default channel capacity.
    #[must_use]
    pub fn new(run_nonce: impl Into<String>) -> Arc<Self> {
        Self::new_with_capacity(run_nonce, DEFAULT_EVIDENCE_CHANNEL_CAPACITY)
    }

    /// Constructs a collector with an explicit bounded channel capacity.
    /// A smaller capacity makes saturation (and thus the `valid: false`
    /// path) easier to reach deterministically-in-practice under a burst
    /// of copies, which this crate's own self-tests use; production
    /// callers should generally prefer [`Self::new`].
    ///
    /// Never panics on a poisoned internal `Mutex`: the dedicated
    /// correlator thread spawned here recovers via
    /// `PoisonError::into_inner` on every lock, so even a caught panic
    /// while processing one copy (see the `catch_unwind` below) leaves
    /// the mutex fully usable for every subsequent copy and for
    /// [`Self::finalize`].
    #[must_use]
    pub fn new_with_capacity(run_nonce: impl Into<String>, capacity: usize) -> Arc<Self> {
        let (tx, rx) = sync_channel::<(Direction, Vec<u8>)>(capacity.max(1));
        let state = Arc::new(Mutex::new(CollectorState::new()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let worker_state = Arc::clone(&state);
        let worker_shutdown = Arc::clone(&shutdown);
        let worker = thread::spawn(move || loop {
            match rx.recv_timeout(WORKER_POLL_INTERVAL) {
                Ok((direction, bytes)) => {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let mut guard = worker_state
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        let lines = match direction {
                            Direction::ClientToChild => guard.client_to_child_lines.push(&bytes),
                            Direction::ChildToClient => guard.child_to_client_lines.push(&bytes),
                        };
                        for line in &lines {
                            process_line(&mut guard, direction, line);
                        }
                    }));
                    if result.is_err() {
                        // A poisoned mutex must still be recovered from
                        // here, exactly as the closure above does --
                        // otherwise this exact recovery path (the whole
                        // reason catch_unwind exists) would itself
                        // silently no-op forever after the very first
                        // panic, and every later `.lock()` on this same
                        // mutex (including finalize()'s) would then
                        // panic too, escalating one caught, isolated
                        // correlator panic into an unhandled panic that
                        // aborts the entire wrapper process.
                        let mut guard = worker_state
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        guard.mark_invalid(
                            "observer failure: correlator panicked while processing a copy",
                        );
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if worker_shutdown.load(Ordering::Acquire) {
                        break;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        });

        Arc::new(Self {
            tx,
            state,
            run_nonce: run_nonce.into(),
            shutdown,
            worker: Mutex::new(Some(worker)),
        })
    }

    /// Returns a cheap, non-blocking [`CopyHook`] suitable for
    /// `run_duplex_pump`'s `copy_hook` parameter. The hook itself never
    /// parses or correlates -- it only forwards an owned copy (already
    /// cloned by transport before any hook is invoked) into this
    /// collector's own bounded channel via a non-blocking send. On
    /// saturation, the summary is atomically marked invalid; forwarding
    /// is completely unaffected either way.
    #[must_use]
    pub fn hook(self: &Arc<Self>) -> CopyHook {
        let collector = Arc::clone(self);
        Arc::new(move |direction: Direction, bytes: &[u8]| {
            if collector.tx.try_send((direction, bytes.to_vec())).is_err() {
                let mut guard = collector
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                guard.mark_invalid("evidence channel saturated: one or more copies were dropped");
            }
        })
    }

    /// Finalizes collection and returns the resulting [`EvidenceSummary`].
    /// Waits a short, bounded, best-effort grace period (see
    /// [`FINALIZE_GRACE_PERIOD`]) for any already-in-flight trailing copy
    /// to arrive, signals the dedicated correlator thread to stop, and
    /// joins it (bounded by [`WORKER_POLL_INTERVAL`]). Safe to call at
    /// most once per collector in production.
    ///
    /// Never panics on a poisoned internal `Mutex`: every lock on this
    /// collector's state (here and in [`Self::new_with_capacity`]'s
    /// correlator thread and [`Self::hook`]) recovers via
    /// `PoisonError::into_inner` rather than `.expect(...)`, so a single
    /// caught correlator panic can never escalate into an unhandled
    /// panic here on the caller's own thread.
    #[must_use]
    pub fn finalize(&self) -> EvidenceSummary {
        thread::sleep(FINALIZE_GRACE_PERIOD);
        self.shutdown.store(true, Ordering::Release);
        let worker_handle = self
            .worker
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(handle) = worker_handle {
            let _ = handle.join();
        }
        let guard = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        EvidenceSummary {
            run_nonce: self.run_nonce.clone(),
            valid: guard.valid,
            invalid_reason: guard.invalid_reason.clone(),
            initialize_correlation: guard.initialize_correlation.clone(),
            events: guard.events.clone(),
        }
    }
}

fn frame_kind_str(kind: FrameKind) -> &'static str {
    match kind {
        FrameKind::Request => "request",
        FrameKind::Response => "response",
        FrameKind::Notification => "notification",
        FrameKind::Unparseable => "unparseable",
    }
}

fn direction_str(direction: Direction) -> &'static str {
    match direction {
        Direction::ClientToChild => "client_to_child",
        Direction::ChildToClient => "child_to_client",
    }
}

fn frame_event_to_json(event: &FrameEvent) -> serde_json::Value {
    serde_json::json!({
        "direction": direction_str(event.direction),
        "sequence": event.sequence,
        "kind": frame_kind_str(event.kind),
        "method": event.method,
        "id": event.id,
        "byte_len": event.byte_len,
        "digest_hex": event.digest_hex,
    })
}

fn initialize_correlation_to_json(correlation: &InitializeCorrelation) -> serde_json::Value {
    serde_json::json!({
        "request_id": correlation.request_id,
        "protocol_version": correlation.protocol_version,
        "redacted_request_params": correlation.redacted_request_params,
        "redacted_result": correlation.redacted_result,
    })
}

/// Builds the exact JSON value persisted to `--evidence-output`. Built
/// directly as a `serde_json::Value` (via the `json!` macro) rather than
/// `#[derive(Serialize)]`, so this task needs no `serde` derive
/// dependency beyond the standalone `serde_json` crate already in use
/// for parsing.
// The `evidence_summary` prefix is deliberate and clearer than a bare
// `to_json` for a function re-exported from the crate root's public API
// surface, matching this module's own `EvidenceSummary` type name.
#[allow(clippy::module_name_repetitions)]
#[must_use]
pub fn evidence_summary_to_json(summary: &EvidenceSummary) -> serde_json::Value {
    serde_json::json!({
        "run_nonce": summary.run_nonce,
        "valid": summary.valid,
        "invalid_reason": summary.invalid_reason,
        "initialize_correlation": summary
            .initialize_correlation
            .as_ref()
            .map(initialize_correlation_to_json),
        "events": summary.events.iter().map(frame_event_to_json).collect::<Vec<_>>(),
        "digest_note": "digest_hex values are a std-only, non-cryptographic 64-bit \
            content digest (DefaultHasher/SipHash-1-3) for diagnostic identity \
            comparison only -- not a security digest",
    })
}

/// Atomically writes the redacted evidence summary to `path`: serializes
/// to a temporary file in the same directory, then renames it into place
/// (an atomic replace on both Windows and Unix for same-volume renames).
/// Never writes raw frame bytes -- only [`evidence_summary_to_json`]'s
/// redacted, structured output.
///
/// # Errors
///
/// Returns an error if the summary cannot be serialized, the temporary
/// file cannot be created/written/flushed, or the final rename fails.
pub fn write_evidence_output(summary: &EvidenceSummary, path: &Path) -> std::io::Result<()> {
    let value = evidence_summary_to_json(summary);
    let bytes = serde_json::to_vec_pretty(&value)
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("evidence-output.json");
    let tmp_name = format!(".{file_name}.tmp-{}", std::process::id());
    let tmp_path = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(tmp_name),
        _ => std::path::PathBuf::from(tmp_name),
    };

    {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(&bytes)?;
        file.flush()?;
    }
    std::fs::rename(&tmp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Directly proves the poison-recovery contract every lock in this
    /// module now relies on (`.lock().unwrap_or_else(PoisonError::into_inner)`
    /// instead of `.lock().expect(...)` / `if let Ok(guard) = ...lock()`):
    /// a panic while holding the lock on another thread poisons it, and a
    /// subsequent lock on THIS thread must still succeed and observe the
    /// state left behind by the panicking thread, rather than being
    /// permanently unusable. Before this fix, the analogous
    /// `if let Ok(guard) = mutex.lock() { ... }` recovery pattern used in
    /// `EvidenceCollector`'s correlator/hook would silently and
    /// permanently skip its body forever after the first panic, and
    /// `finalize()`'s own `.lock().expect(...)` would itself panic --
    /// escalating one caught, isolated panic into an unhandled process
    /// crash. This test exercises the exact same std `Mutex`
    /// poison-then-recover mechanism those call sites depend on, using a
    /// `CollectorState` (this module's own real guarded type) as the
    /// payload rather than a placeholder type, so a future refactor that
    /// reverts any of those call sites back to `.expect(...)` would show
    /// up as this test's own reasoning becoming stale, not as a silent
    /// behavioral gap.
    #[test]
    fn a_poisoned_mutex_recovers_via_poison_error_into_inner_and_observes_prior_state() {
        let state = Arc::new(Mutex::new(CollectorState::new()));

        let panicking_state = Arc::clone(&state);
        let joined = thread::spawn(move || {
            let mut guard = panicking_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.mark_invalid("deliberate: proving poison recovery, not a real failure");
            panic!("deliberate: poison this mutex while still holding the lock");
        })
        .join();
        assert!(
            joined.is_err(),
            "the spawned thread must actually have panicked for this test to prove anything"
        );

        // The exact recovery pattern used throughout this module: this
        // must NOT panic, and must NOT silently skip -- it must return a
        // fully usable guard.
        let recovered_guard = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(
            !recovered_guard.valid,
            "the guard obtained after poison-recovery must observe the mutation the \
             panicking thread made just before it panicked, proving this is a real \
             recovered guard over the same underlying state, not a fresh/default one"
        );
        assert_eq!(
            recovered_guard.invalid_reason.as_deref(),
            Some("deliberate: proving poison recovery, not a real failure")
        );

        // The mutex must remain fully usable for subsequent, unrelated
        // locks too -- not just the one immediately after the panic.
        drop(recovered_guard);
        let again = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(
            !again.valid,
            "state must remain readable on a second, later lock too"
        );
    }
}
