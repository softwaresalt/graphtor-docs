//! Core synchronous transport for the standalone `mcp-probe` diagnostic
//! crate: raw `std::process` / `std::thread` full-duplex byte pumps between
//! an "incoming"/"outgoing" pair (representing the actual Copilot CLI's
//! stdin/stdout from this process's perspective) and a spawned child's
//! stdio, with half-close propagation, a continuous bounded stderr drain,
//! deadline signaling, and a bounded, non-blocking post-write copy-delivery
//! seam.
//!
//! This module never rewrites, reframes, or otherwise interprets protocol
//! bytes -- it forwards them byte-for-byte. It owns no observer, evidence,
//! workspace, config, process-identity, or exact-CLI concern; those seams
//! belong to `056.023-T`, `056.021-T`, `056.022-T`, and `056.001-T`
//! respectively.
//!
//! `056.020-T` deliberately does not wire this module's public API into
//! `main.rs`'s production dispatch for production use -- that composition
//! is `056.022-T`'s job ("sequentially update `main.rs` to compose process
//! spawning and the wrapper onto the 056.020-T transport"). This module is
//! exposed through a small library target (`src/lib.rs`) precisely so this
//! crate's own black-box self-tests, in `tools/mcp-probe/tests/`, can
//! import it directly while separately spawning the actual compiled
//! `mcp-probe` binary (via `env!("CARGO_BIN_EXE_mcp-probe")`) as the
//! fixture "child" process. Self-tests deliberately live in an integration
//! test file rather than a `#[cfg(test)]` unit-test module here: a unit
//! test's own `std::env::current_exe()` would resolve to the `cargo test`
//! harness binary, not this crate's real dispatch in `main.rs`, so the
//! re-exec self-test technique only works from a genuine integration test
//! that reads `CARGO_BIN_EXE_mcp-probe` instead.

use std::io::{self, Read, Write};
use std::process::Child;
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Which direction a forwarded byte copy travelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Bytes read from the incoming (client-facing) reader and written to
    /// the child's stdin.
    ClientToChild,
    /// Bytes read from the child's stdout and written to the outgoing
    /// (client-facing) writer.
    ChildToClient,
}

/// A post-write, bounded, non-blocking copy-delivery hook. Hooks are
/// invoked from a dedicated delivery thread -- never from either pump
/// thread -- so an arbitrarily slow, wedged, or panicking hook can never
/// delay or block the primary byte forwarding.
pub type CopyHook = Arc<dyn Fn(Direction, &[u8]) + Send + Sync + 'static>;

/// Bounded capacity of the internal copy-delivery channel. This is a
/// diagnostic side-channel, not a buffering strategy for the primary
/// forwarded stream: a full channel silently drops the copy currently
/// being delivered (via `try_send`, which never evicts an
/// already-queued item -- see [`deliver_copy`]) rather than ever
/// blocking a pump thread.
const DELIVERY_CHANNEL_CAPACITY: usize = 64;

/// Bounded, best-effort window `run_duplex_pump` waits for its own
/// delivery worker thread to fully drain and exit on its own, once both
/// primary pump directions have already completed (or timed out) and
/// every sender clone it does not itself own has therefore already been
/// dropped. This never delays the PRIMARY byte forwarding (both pump
/// threads are already finished by the time this budget starts) -- it
/// only bounds how long `run_duplex_pump` itself waits before returning,
/// so an arbitrarily slow or wedged hook still cannot hang the caller
/// indefinitely (Copilot review thread H, 2026-09 -- 049-S PR #120,
/// round 2).
const DELIVERY_DRAIN_BUDGET: Duration = Duration::from_millis(250);

/// Default per-read buffer size, in bytes, for both pump directions.
const DEFAULT_PUMP_BUFFER_SIZE: usize = 8192;

/// Tunable pump configuration.
#[derive(Debug, Clone)]
pub struct PumpConfig {
    /// Per-read buffer size in bytes.
    pub buffer_size: usize,
    /// Optional wall-clock deadline for the whole duplex session. When the
    /// deadline elapses before both directions have closed, the pump stops
    /// waiting and reports `timed_out: true` in the returned outcome. It
    /// never forcibly kills the child -- direct `Child`-handle teardown is
    /// owned by `056.022-T`.
    pub deadline: Option<Duration>,
}

impl Default for PumpConfig {
    fn default() -> Self {
        Self {
            buffer_size: DEFAULT_PUMP_BUFFER_SIZE,
            deadline: None,
        }
    }
}

/// Outcome of one full-duplex pump session.
///
/// Four independent, orthogonal boolean outcome flags is over
/// `clippy::pedantic`'s default `struct_excessive_bools` threshold, but
/// each one reports a genuinely separate yes/no fact about this one
/// pump run (which half closed, whether the deadline fired, whether the
/// diagnostic delivery worker fully drained) -- there is no shared state
/// machine or mutually exclusive grouping among them that a two-variant
/// enum would clarify; that refactor would only add indirection here.
#[derive(Debug, Default, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct PumpOutcome {
    /// `true` once the incoming (client) reader reached EOF (or errored)
    /// and the child's stdin half-close was propagated by dropping it.
    pub client_to_child_closed: bool,
    /// `true` once the child's stdout reached EOF (or errored) and no more
    /// bytes will be forwarded to the outgoing (client) writer.
    pub child_to_client_closed: bool,
    /// `true` when the configured deadline elapsed before both directions
    /// closed.
    pub timed_out: bool,
    /// Total bytes forwarded client -> child.
    pub client_to_child_bytes: u64,
    /// Total bytes forwarded child -> client.
    pub child_to_client_bytes: u64,
    /// Total diagnostic copies dropped at this transport's own bounded
    /// delivery channel (across both directions combined), because the
    /// channel was full or the delivery worker had already exited. This
    /// is always `0` when no `copy_hook` was supplied. A caller composing
    /// this transport with a downstream observer (e.g.
    /// `crate::evidence::EvidenceCollector`) should treat a nonzero value
    /// here as its own signal that the observer's summary is incomplete
    /// -- a drop at this outer, transport-level channel happens BEFORE
    /// the hook (and therefore before the observer) ever sees the copy,
    /// so the observer has no way to detect this loss on its own.
    pub transport_copies_dropped: u64,
    /// `true` when the delivery worker thread did not finish draining
    /// its already-enqueued (successfully `try_send`-accepted) copies
    /// within [`DELIVERY_DRAIN_BUDGET`] after both pump directions
    /// completed, so it was detached rather than joined. This is
    /// DISTINCT from `transport_copies_dropped`: a dropped copy never
    /// reached the channel at all (rejected by `try_send`), whereas a
    /// copy counted here for `true` WAS accepted into the channel but
    /// its hook invocation may never complete/observe before this
    /// function returns. Always `false` when no `copy_hook` was
    /// supplied, or when the worker (if any) finished within budget. A
    /// caller composing this transport with a downstream observer must
    /// treat `true` here exactly like a nonzero `transport_copies_dropped`
    /// -- as its own signal that the observer's summary may be
    /// incomplete and must not be reported as cleanly `valid` (Copilot
    /// review thread H, 2026-09 -- 049-S PR #120, round 2).
    pub delivery_drain_incomplete: bool,
}

/// Spawns a background delivery worker that drains a bounded channel and
/// invokes `hook` for every received copy. The returned sender is always
/// used with `try_send`, so a slow or wedged hook can only ever cause
/// dropped diagnostic copies -- never delay the pump threads that hold the
/// sender.
fn spawn_delivery_worker(hook: CopyHook) -> (SyncSender<(Direction, Vec<u8>)>, JoinHandle<()>) {
    let (tx, rx) = sync_channel::<(Direction, Vec<u8>)>(DELIVERY_CHANNEL_CAPACITY);
    let handle = thread::spawn(move || {
        while let Ok((direction, bytes)) = rx.recv() {
            let result =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| hook(direction, &bytes)));
            if let Err(payload) = result {
                // The hook itself panicked. Forwarding is unaffected (this
                // thread is never one of the two pump threads), but silently
                // continuing without any signal would mean every future
                // delivery attempt on this same worker permanently loses its
                // diagnostic copy with zero indication anywhere that this
                // happened -- defeating the purpose of a diagnostic tool.
                // Log once per occurrence and keep draining the channel so a
                // single bad copy does not also lose every subsequent one.
                eprintln!(
                    "mcp-probe: copy-delivery hook panicked, this copy was lost: {}",
                    panic_payload_message(&payload)
                );
            }
        }
    });
    (tx, handle)
}

/// Best-effort extraction of a human-readable message from a caught panic
/// payload, for diagnostic logging only. Falls back to a fixed string for
/// any payload that is not a `&str`/`String` (the two conventional panic
/// message payload types).
fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

/// Non-blocking, best-effort delivery of one forwarded copy. Never blocks
/// the caller: a full channel (slow or wedged hook) silently drops the
/// copy rather than delaying forwarding. Returns `true` when the copy was
/// dropped (channel full or the delivery worker already exited), so
/// callers can accumulate a diagnostic drop count -- see
/// [`PumpOutcome::transport_copies_dropped`].
#[must_use]
fn deliver_copy(tx: &SyncSender<(Direction, Vec<u8>)>, direction: Direction, bytes: &[u8]) -> bool {
    match tx.try_send((direction, bytes.to_vec())) {
        Ok(()) => false,
        Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => true,
    }
}

/// Continuously drains `reader` into `sink`, bounded to
/// `DEFAULT_PUMP_BUFFER_SIZE`-sized reads. Used for the child's stderr,
/// which is drained unconditionally and concurrently so a chatty child can
/// never deadlock the primary duplex forwarding.
fn drain_stderr<R: Read, W: Write>(mut reader: R, mut sink: W) {
    let mut buf = [0_u8; DEFAULT_PUMP_BUFFER_SIZE];
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if sink.write_all(&buf[..n]).is_err() {
                    break;
                }
                let _ = sink.flush();
            }
        }
    }
}

/// Shared read-and-forward loop for one primary pump direction: reads
/// `reader` to EOF (or first error), writing (and flushing) each chunk to
/// `writer`, optionally attempting a non-blocking diagnostic delivery of
/// the same bytes via `tx`. Returns `(total_bytes_forwarded,
/// copies_dropped)`. Extracted once and reused for both `client_to_child`
/// and `child_to_client` in [`run_duplex_pump`] -- the two directions'
/// loops are otherwise identical, and duplicating them risked one being
/// fixed (e.g. for the drop-counting added alongside
/// [`PumpOutcome::transport_copies_dropped`]) without the other.
fn pump_one_direction<R: Read, W: Write>(
    mut reader: R,
    mut writer: W,
    buffer_size: usize,
    direction: Direction,
    tx: Option<&SyncSender<(Direction, Vec<u8>)>>,
) -> (u64, u64) {
    let mut buf = vec![0_u8; buffer_size];
    let mut total = 0_u64;
    let mut dropped = 0_u64;
    loop {
        match reader.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                if writer.write_all(&buf[..n]).is_err() || writer.flush().is_err() {
                    break;
                }
                total += n as u64;
                if let Some(tx) = tx {
                    if deliver_copy(tx, direction, &buf[..n]) {
                        dropped += 1;
                    }
                }
            }
        }
    }
    (total, dropped)
}

/// Runs the core synchronous, full-duplex byte pump between `incoming`
/// (the client-facing reader; production callers pass `io::stdin()`),
/// `outgoing` (the client-facing writer; production callers pass
/// `io::stdout()`), and `child`'s stdio.
///
/// `child` MUST have been spawned with piped `stdin` and `stdout` (piped
/// `stderr` is optional but expected in production). This function takes
/// ownership of those handles via `Child::stdin`/`stdout`/`stderr`
/// `.take()`. It does not spawn or reap `child` itself -- direct
/// `Child`-handle ownership and teardown remain the caller's
/// responsibility (production ownership is `056.022-T`'s contract; this
/// crate's own transport self-tests own bounded, test-only cleanup of
/// their own fixture children).
///
/// Bytes are never rewritten or reframed. `copy_hook`, when provided, is
/// invoked once per forwarded chunk from a dedicated delivery thread,
/// strictly after that chunk's write-and-flush completes on the owning
/// pump thread -- never before, and never on either pump thread itself.
/// Delivery is always attempted via a bounded non-blocking channel: a
/// slow, wedged, or absent hook can drop diagnostic copies but can never
/// alter or delay the forwarded stream.
///
/// # Errors
///
/// Returns an error only if the child's stdio was not piped (missing
/// stdin/stdout handle). Ordinary I/O errors on the pumped streams are
/// treated as stream closure, not as function-level errors, and are
/// reflected in the returned outcome instead.
pub fn run_duplex_pump<R, W>(
    incoming: R,
    outgoing: W,
    child: &mut Child,
    config: &PumpConfig,
    copy_hook: Option<CopyHook>,
) -> io::Result<PumpOutcome>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    let child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "child stdin not piped"))?;
    let child_stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "child stdout not piped"))?;
    let child_stderr = child.stderr.take();

    let delivery = copy_hook.map(spawn_delivery_worker);
    let stdin_tx = delivery.as_ref().map(|(tx, _)| tx.clone());
    let stdout_tx = delivery.as_ref().map(|(tx, _)| tx.clone());

    let buffer_size = config.buffer_size.max(1);
    let deadline = config.deadline;
    let start = Instant::now();

    // Stderr drain: always running, bounded, concurrent, and never gates
    // either primary direction.
    let stderr_handle =
        child_stderr.map(|stderr| thread::spawn(move || drain_stderr(stderr, io::stderr())));

    // client -> child: read `incoming` to EOF, forwarding each chunk to
    // `child_stdin`. EOF (or a read/write error) closes `child_stdin` by
    // dropping it, propagating the half-close to the child.
    let client_to_child = thread::spawn(move || -> (bool, u64, u64) {
        let (total, dropped) = pump_one_direction(
            incoming,
            child_stdin,
            buffer_size,
            Direction::ClientToChild,
            stdin_tx.as_ref(),
        );
        (true, total, dropped)
    });

    // child -> client: read the child's stdout to EOF, forwarding each
    // chunk to `outgoing`.
    let child_to_client = thread::spawn(move || -> (bool, u64, u64) {
        let (total, dropped) = pump_one_direction(
            child_stdout,
            outgoing,
            buffer_size,
            Direction::ChildToClient,
            stdout_tx.as_ref(),
        );
        (true, total, dropped)
    });

    let mut outcome = PumpOutcome::default();
    let mut stdin_pump_done = false;
    let mut stdout_pump_done = false;
    let poll_interval = Duration::from_millis(5);

    loop {
        stdin_pump_done = stdin_pump_done || client_to_child.is_finished();
        stdout_pump_done = stdout_pump_done || child_to_client.is_finished();
        if stdin_pump_done && stdout_pump_done {
            break;
        }
        if let Some(deadline) = deadline {
            if start.elapsed() >= deadline {
                outcome.timed_out = true;
                break;
            }
        }
        thread::sleep(poll_interval);
    }

    if stdin_pump_done {
        if let Some((closed, bytes, dropped)) =
            join_finished_pump_thread(client_to_child, "client_to_child")
        {
            outcome.client_to_child_closed = closed;
            outcome.client_to_child_bytes = bytes;
            outcome.transport_copies_dropped += dropped;
        }
    }
    if stdout_pump_done {
        if let Some((closed, bytes, dropped)) =
            join_finished_pump_thread(child_to_client, "child_to_client")
        {
            outcome.child_to_client_closed = closed;
            outcome.child_to_client_bytes = bytes;
            outcome.transport_copies_dropped += dropped;
        }
    }

    // Drop the delivery sender (this function's own clone; the two pump
    // threads' clones -- `stdin_tx`/`stdout_tx` -- are already dropped
    // above whenever their owning thread was actually joined) so the
    // delivery worker's `rx.recv()` loop can observe channel closure
    // once every sender is gone, then give that worker thread a short,
    // bounded window to drain whatever it had already accepted and exit
    // on its own -- never joined unboundedly, so an arbitrarily slow or
    // wedged hook still cannot delay this function's return past
    // `DELIVERY_DRAIN_BUDGET`. Previously this only dropped the sender
    // and detached the `JoinHandle` unconditionally, with no signal at
    // all when already-enqueued copies were left stranded mid-delivery
    // (Copilot review thread H, 2026-09 -- 049-S PR #120, round 2):
    // that silently produced an evidence summary reported as `valid`
    // despite missing data.
    if let Some((tx, handle)) = delivery {
        drop(tx);
        let drain_deadline = Instant::now() + DELIVERY_DRAIN_BUDGET;
        while !handle.is_finished() && Instant::now() < drain_deadline {
            thread::sleep(poll_interval);
        }
        if handle.is_finished() {
            // Already fully drained (every accepted copy's hook
            // invocation ran to completion, panic-caught or not) --
            // reclaim the thread; this call itself cannot block.
            let _ = handle.join();
        } else {
            // Still not finished after the bounded window -- detach
            // rather than block further, but the caller MUST now treat
            // this exactly like a nonzero `transport_copies_dropped`:
            // some already-accepted copies may never reach the hook.
            outcome.delivery_drain_incomplete = true;
            drop(handle);
        }
    }

    // The primary pumps can both finish (e.g. the child closed stdout
    // and stdin) while the child is still alive and holding stderr
    // open, which would otherwise let the unconditional stderr join
    // below hang past the configured deadline (Copilot review,
    // 2026-09 -- 049-S PR #120). When a deadline is configured and the
    // primary loop did not already time out, give the stderr thread the
    // SAME remaining deadline budget via bounded polling instead of an
    // unconditional blocking join; if it does not finish in time, mark
    // the overall outcome as timed out too so the caller tears the
    // child down (which then forces stderr closed) instead of treating
    // this as a clean completion.
    if !outcome.timed_out {
        if let Some(deadline) = deadline {
            let remaining = deadline.saturating_sub(start.elapsed());
            if !stderr_thread_finished_within(stderr_handle.as_ref(), remaining, poll_interval) {
                outcome.timed_out = true;
            }
        }
    }

    // A timed-out direction's `JoinHandle` is intentionally neither joined
    // nor detached here -- just dropped on return; see
    // `join_stderr_thread`'s doc comment for why the stderr thread below
    // follows the same detach-on-timeout shape.
    join_stderr_thread(stderr_handle, outcome.timed_out);

    Ok(outcome)
}

/// Polls `handle` (if present) for completion within `budget`, without
/// ever blocking past it. Returns `true` immediately when no stderr
/// thread was spawned (nothing to wait for) or once `handle.is_finished()`
/// observes completion; returns `false` if `budget` elapses first. Never
/// consumes or joins `handle` -- the caller still owns that decision via
/// [`join_stderr_thread`].
fn stderr_thread_finished_within(
    handle: Option<&thread::JoinHandle<()>>,
    budget: Duration,
    poll_interval: Duration,
) -> bool {
    let Some(handle) = handle else {
        return true;
    };
    let wait_start = Instant::now();
    loop {
        if handle.is_finished() {
            return true;
        }
        if wait_start.elapsed() >= budget {
            return false;
        }
        thread::sleep(poll_interval);
    }
}

/// Joins a finished primary pump-thread handle, logging (and returning
/// `None`, leaving the caller's `PumpOutcome` fields at their `Default`)
/// if the thread panicked instead of returning normally -- so a panic is
/// never silently indistinguishable from "this direction never observed
/// EOF". The returned tuple is `(closed, bytes_forwarded, copies_dropped)`.
fn join_finished_pump_thread(
    handle: thread::JoinHandle<(bool, u64, u64)>,
    label: &str,
) -> Option<(bool, u64, u64)> {
    match handle.join() {
        Ok(result) => Some(result),
        Err(payload) => {
            eprintln!(
                "mcp-probe: {label} pump thread panicked: {}",
                panic_payload_message(&payload)
            );
            None
        }
    }
}

/// Joins the stderr drain thread's handle unless `timed_out` is set, in
/// which case it is detached (dropped) instead: the stderr pipe only
/// reaches EOF once the child's stderr closes, which for a wedged,
/// not-yet-reaped child never happens until strictly after this function
/// returns, so unconditionally joining here would reintroduce exactly the
/// hang the deadline exists to bound. Callers set `timed_out` for this
/// case whether the primary pumps themselves hit the deadline OR the
/// stderr thread alone outlived the same deadline budget after both
/// primary directions closed (see [`stderr_thread_finished_within`]) --
/// either way a live child with stderr still open must never wedge this
/// join past the caller's requested bound. A join `Err` (the thread panicked)
/// is logged rather than silently discarded.
fn join_stderr_thread(handle: Option<thread::JoinHandle<()>>, timed_out: bool) {
    let Some(handle) = handle else {
        return;
    };
    if timed_out {
        drop(handle);
    } else if let Err(payload) = handle.join() {
        eprintln!(
            "mcp-probe: stderr drain thread panicked: {}",
            panic_payload_message(&payload)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Adversarial-review remediation regression tests (U-6) ────────

    #[test]
    fn deliver_copy_reports_false_when_the_channel_accepts_the_copy() {
        let (tx, _rx) = sync_channel::<(Direction, Vec<u8>)>(1);
        let dropped = deliver_copy(&tx, Direction::ClientToChild, b"hello");
        assert!(!dropped);
    }

    #[test]
    fn deliver_copy_reports_true_and_never_blocks_when_the_channel_is_full() {
        // U-6 / U-8: `deliver_copy` must use `try_send` semantics -- a
        // full channel drops the CURRENT copy (never evicting an
        // already-queued older one) and must never block the caller.
        let (tx, _rx) = sync_channel::<(Direction, Vec<u8>)>(1);
        // Fill the one slot; nothing ever drains `_rx` in this test.
        let first_dropped = deliver_copy(&tx, Direction::ClientToChild, b"first");
        assert!(
            !first_dropped,
            "the first send into an empty slot must succeed"
        );

        let second_dropped = deliver_copy(&tx, Direction::ClientToChild, b"second");
        assert!(
            second_dropped,
            "a full channel must report the copy as dropped, not block"
        );
    }

    #[test]
    fn deliver_copy_reports_true_once_the_receiver_has_disconnected() {
        let (tx, rx) = sync_channel::<(Direction, Vec<u8>)>(4);
        drop(rx);
        let dropped = deliver_copy(&tx, Direction::ChildToClient, b"anything");
        assert!(
            dropped,
            "a disconnected receiver must count as a dropped copy"
        );
    }
}
