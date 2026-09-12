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
/// forwarded stream: a full channel silently drops the oldest-pending
/// delivery attempt rather than ever blocking a pump thread.
const DELIVERY_CHANNEL_CAPACITY: usize = 64;

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
#[derive(Debug, Default, Clone)]
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
            hook(direction, &bytes);
        }
    });
    (tx, handle)
}

/// Non-blocking, best-effort delivery of one forwarded copy. Never blocks
/// the caller: a full channel (slow or wedged hook) silently drops the
/// copy rather than delaying forwarding.
fn deliver_copy(tx: &SyncSender<(Direction, Vec<u8>)>, direction: Direction, bytes: &[u8]) {
    match tx.try_send((direction, bytes.to_vec())) {
        Ok(()) | Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {}
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
    let client_to_child = thread::spawn(move || -> (bool, u64) {
        let mut reader = incoming;
        let mut writer = child_stdin;
        let mut buf = vec![0_u8; buffer_size];
        let mut total = 0_u64;
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if writer.write_all(&buf[..n]).is_err() || writer.flush().is_err() {
                        break;
                    }
                    total += n as u64;
                    if let Some(tx) = &stdin_tx {
                        deliver_copy(tx, Direction::ClientToChild, &buf[..n]);
                    }
                }
            }
        }
        drop(writer);
        (true, total)
    });

    // child -> client: read the child's stdout to EOF, forwarding each
    // chunk to `outgoing`.
    let child_to_client = thread::spawn(move || -> (bool, u64) {
        let mut reader = child_stdout;
        let mut writer = outgoing;
        let mut buf = vec![0_u8; buffer_size];
        let mut total = 0_u64;
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if writer.write_all(&buf[..n]).is_err() || writer.flush().is_err() {
                        break;
                    }
                    total += n as u64;
                    if let Some(tx) = &stdout_tx {
                        deliver_copy(tx, Direction::ChildToClient, &buf[..n]);
                    }
                }
            }
        }
        (true, total)
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
        if let Ok((closed, bytes)) = client_to_child.join() {
            outcome.client_to_child_closed = closed;
            outcome.client_to_child_bytes = bytes;
        }
    }
    if stdout_pump_done {
        if let Ok((closed, bytes)) = child_to_client.join() {
            outcome.child_to_client_closed = closed;
            outcome.child_to_client_bytes = bytes;
        }
    }

    // Drop delivery senders so any background delivery worker drains and
    // exits; deliberately never joined here -- an arbitrarily slow hook
    // must never delay pump completion, which is exactly the property
    // this seam guarantees.
    drop(delivery);

    // The stderr drain thread only reaches EOF once the child's stderr
    // pipe closes, which (for a wedged child) never happens until the
    // caller reaps the child -- strictly after this function returns.
    // Unconditionally joining it here would re-introduce exactly the
    // hang the deadline exists to bound: a wedged child would deadlock
    // this whole function against its own not-yet-joined stderr thread.
    // Only join when the pump did not time out, i.e. both directions
    // already closed on their own and the child's stdio (stderr
    // included) is expected to have closed alongside them; on a timeout,
    // detach the drain thread instead so a still-alive, unreaped child
    // can never delay pump completion.
    if let Some(handle) = stderr_handle {
        if outcome.timed_out {
            drop(handle);
        } else {
            let _ = handle.join();
        }
    }

    Ok(outcome)
}
