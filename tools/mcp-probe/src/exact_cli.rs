//! `056.001-T`: one-shot, investigate-first exact-CLI differential serve
//! probe and cause ordering.
//!
//! This module owns the `exact-cli` subcommand's whole runner. It composes,
//! never reimplements:
//!   - the `056.020-T` transport (`crate::transport::run_duplex_pump`) for
//!     bounded, deadline-governed byte capture of the exact Copilot CLI's
//!     own stdout;
//!   - the `056.022-T` process guard (`crate::process::ChildGuard`) for
//!     direct-`Child`-handle spawn/teardown of the exact Copilot CLI
//!     process itself (never a whole-process-tree claim);
//!   - the `056.022-T` wrapper (`crate::process`'s `wrapper` subcommand,
//!     invoked indirectly -- Copilot spawns it per the generated
//!     `.mcp.json`) and the `056.023-T` evidence seam, whose redacted
//!     `--evidence-output` summary this runner only *reads*, never
//!     recomputes;
//!   - the `056.021-T` isolated `logs/probe/<nonce>` workspace
//!     (`crate::workspace::create_probe_workspace`) for the control,
//!     treatment, and nested ancestor/child fixtures.
//!
//! # Why a bespoke bounded-capture helper is not a pump reimplementation
//!
//! Every child this runner spawns (Gate 1's `mcp get`/`mcp list`, and each
//! causal-pass leg's exact Copilot CLI invocation) is captured the exact
//! same way: `run_duplex_pump` is driven with an immediately-EOF
//! `incoming` side (`std::io::empty()`, since none of these auxiliary
//! processes are read interactively over their own stdin -- Gate 1 needs
//! no stdin at all, and causal-pass legs are driven entirely via `-p`) and
//! an in-memory `outgoing` capture sink. This reuses the exact same
//! deadline/timeout and stderr-drain machinery `056.020-T` already built
//! and `056.022-T`'s `run_wrapper` already exercises for the *inner*
//! child; this module applies it one layer up, to the exact Copilot CLI
//! child itself, per this task's own acceptance criteria ("apply the
//! deadline through 056.020-T and the direct-`Child`-handle teardown
//! through 056.022-T"). No new pump, teardown, wrapper, or evidence-capture
//! logic is introduced here.
//!
//! # Gate 1 (ancestor config-isolation) needs no live session
//!
//! Empirically, `copilot mcp get <name> --json` / `copilot mcp list --json`
//! perform pure static config resolution (walking the nearest-`.mcp.json`
//! -wins ancestor chain and reporting the resolved `sourcePath` for the
//! requested entry) with **no** live MCP server connection attempt and
//! **no** AI-model call -- so Gate 1 is fully provable safely and cheaply
//! via `mcp get`, before spending any real model call on the causal
//! control/treatment pass(es).
//!
//! # Causal passes require a real `-p` invocation
//!
//! The real Copilot CLI's non-interactive mode refuses to start at all
//! without a prompt ("No prompt provided. Run in an interactive terminal
//! or provide a prompt with -p or via standard in."), and MCP servers are
//! connected eagerly at session bootstrap -- before any model turn is
//! dispatched. This runner therefore issues a short, explicitly
//! tool-declining, action-declining diagnostic prompt via `-p`
//! `--allow-all-tools` (required for non-interactive mode), bounding the
//! whole session with a wall-clock deadline and a direct-`Child`-handle
//! kill on timeout so the observation window is tight around the MCP
//! bootstrap/connect phase rather than any extended model turn.
//!
//! # Env-inheritance sentinel observation (for `056.006-T` selection)
//!
//! The `056.022-T` wrapper's own `sentinel_inherited` observation
//! (`crate::process::WrapperOutcome`) is process-local: `main.rs`'s
//! `wrapper` subcommand dispatch never persists it anywhere this runner
//! -- a separate, arm's-length process -- can read, and the
//! `056.023-T` evidence-output schema does not carry it either (by
//! design: that schema is JSON-RPC frame/`initialize` correlation only).
//! Rather than reaching into either of those owned modules to add a new
//! persisted field (which this task's own acceptance criteria forbids:
//! "never reimplement ... evidence capture"), this runner independently,
//! and only ever in a read-only/observational capacity, watches for a
//! direct child of the exact Copilot CLI process whose own executable
//! path matches this exact binary (the wrapper is always
//! `mcp-probe wrapper`, i.e. a re-invocation of this same compiled
//! binary) and inspects that child's OS-reported process environment
//! (`sysinfo::Process::environ`) for the exact sentinel `KEY=VALUE`
//! entry, on a background polling thread bounded to the same leg
//! deadline. This never touches `crate::process`/`crate::evidence`, never
//! kills or otherwise mutates the observed process (only [`ChildGuard`]
//! ever does that, and only for the directly-owned Copilot child), and
//! is always reported as a best-effort, honestly-labeled observation --
//! a "not observed" result is explicitly never treated as proof of
//! non-inheritance (the wrapper may simply have already exited, or this
//! platform's process-environment introspection may itself be
//! unavailable/restricted).

use crate::process::{ChildGuard, ENV_INHERITANCE_SENTINEL_VAR};
use crate::transport::{run_duplex_pump, PumpConfig};
use crate::workspace::{create_probe_workspace, McpServerEntrySpec, ProbeWorkspace};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sysinfo::{Pid, ProcessRefreshKind, System, UpdateKind};

/// Literal manual-equivalent invocation recorded alongside the exact
/// Copilot identity. There is no CLI-flag equivalent of the interactive
/// `/mcp show <name>` slash command (confirmed against the real `copilot
/// mcp --help` surface: the `mcp` subcommand group is static config
/// display only), so this runner records the exact string a human
/// verifier -- or `T4` -- should type to cross-check identity
/// interactively, rather than attempting to fabricate an automated
/// equivalent that does not exist.
pub const MANUAL_VERIFICATION_INVOCATION: &str = "/mcp show graphtor-docs";

const DEFAULT_ENTRY_NAME: &str = "graphtor-docs";
const DEFAULT_LEG_DEADLINE_SECS: u64 = 45;
const DEFAULT_GATE1_DEADLINE_SECS: u64 = 15;
/// Absolute cap on bytes captured in memory from a child's stdout via
/// [`CaptureSink`] (mirrors `tests/common/serve_driver.rs`'s
/// `MAX_STDERR_CAPTURE_BYTES` bound). This is a *bounded* capture, never a
/// ring buffer -- once the cap is reached, further bytes are silently
/// discarded (never causing the pipe to back up) and the capture is
/// flagged truncated rather than allowed to grow without limit for the
/// entire deadline window.
const MAX_CAPTURE_BYTES: usize = 262_144;
const DEFAULT_PROMPT: &str = "Automated MCP connectivity diagnostic for the 056-F regression \
investigation. Do not call any tools, do not perform any action, and do not answer any \
question. Reply with exactly: DIAGNOSTIC_OK";

/// Parsed `exact-cli` argv contract.
// The `ExactCli` prefix is deliberate and clearer than a bare `Args` for a
// type re-exported from the crate root's public API surface.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone)]
pub struct ExactCliArgs {
    pub copilot_exe: String,
    pub stable_copilot_exe: Option<String>,
    pub repo_root: String,
    pub inner_exe: String,
    pub inner_args: Vec<String>,
    pub entry_name: String,
    pub run_nonce: Option<String>,
    pub leg_deadline: Duration,
    pub gate1_deadline: Duration,
    pub prompt: String,
    pub sentinel_value: Option<String>,
}

fn next_value<I: Iterator<Item = String>>(args: &mut I, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_u64_flag(flag: &str, raw: &str) -> Result<u64, String> {
    raw.parse::<u64>()
        .map_err(|err| format!("{flag}: invalid integer '{raw}': {err}"))
}

/// Parses the `exact-cli` subcommand's argv contract. Unrecognized flags
/// are a hard error (mirrors `crate::process::parse_wrapper_args`'s own
/// fail-closed style).
///
/// # Errors
///
/// Returns a human-readable message when a flag is unknown, missing its
/// required value, has an unparsable value, or a required flag
/// (`--copilot-exe`, `--repo-root`, `--inner-exe`) was never supplied.
pub fn parse_exact_cli_args(args: impl Iterator<Item = String>) -> Result<ExactCliArgs, String> {
    let mut copilot_exe = None;
    let mut stable_copilot_exe = None;
    let mut repo_root = None;
    let mut inner_exe = None;
    let mut inner_args = Vec::new();
    let mut entry_name = None;
    let mut run_nonce = None;
    let mut leg_deadline_secs = None;
    let mut gate1_deadline_secs = None;
    let mut prompt = None;
    let mut sentinel_value = None;

    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--copilot-exe" => copilot_exe = Some(next_value(&mut args, "--copilot-exe")?),
            "--stable-copilot-exe" => {
                stable_copilot_exe = Some(next_value(&mut args, "--stable-copilot-exe")?);
            }
            "--repo-root" => repo_root = Some(next_value(&mut args, "--repo-root")?),
            "--inner-exe" => inner_exe = Some(next_value(&mut args, "--inner-exe")?),
            "--inner-arg" => inner_args.push(next_value(&mut args, "--inner-arg")?),
            "--entry-name" => entry_name = Some(next_value(&mut args, "--entry-name")?),
            "--run-nonce" => run_nonce = Some(next_value(&mut args, "--run-nonce")?),
            "--leg-deadline-secs" => {
                leg_deadline_secs = Some(next_value(&mut args, "--leg-deadline-secs")?);
            }
            "--gate1-deadline-secs" => {
                gate1_deadline_secs = Some(next_value(&mut args, "--gate1-deadline-secs")?);
            }
            "--prompt" => prompt = Some(next_value(&mut args, "--prompt")?),
            "--sentinel-value" => sentinel_value = Some(next_value(&mut args, "--sentinel-value")?),
            other => return Err(format!("exact-cli: unknown argument '{other}'")),
        }
    }

    let leg_deadline = match leg_deadline_secs {
        Some(raw) => Duration::from_secs(parse_u64_flag("--leg-deadline-secs", &raw)?),
        None => Duration::from_secs(DEFAULT_LEG_DEADLINE_SECS),
    };
    let gate1_deadline = match gate1_deadline_secs {
        Some(raw) => Duration::from_secs(parse_u64_flag("--gate1-deadline-secs", &raw)?),
        None => Duration::from_secs(DEFAULT_GATE1_DEADLINE_SECS),
    };

    Ok(ExactCliArgs {
        copilot_exe: copilot_exe.ok_or("--copilot-exe is required")?,
        stable_copilot_exe,
        repo_root: repo_root.ok_or("--repo-root is required")?,
        inner_exe: inner_exe.ok_or("--inner-exe is required")?,
        inner_args,
        entry_name: entry_name.unwrap_or_else(|| DEFAULT_ENTRY_NAME.to_string()),
        run_nonce,
        leg_deadline,
        gate1_deadline,
        prompt: prompt.unwrap_or_else(|| DEFAULT_PROMPT.to_string()),
        sentinel_value,
    })
}

fn generate_run_nonce() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("exact-{nanos}-{}", std::process::id())
}

/// Non-cryptographic FNV-1a 64-bit content digest. Deliberately NOT a
/// security digest -- chosen only because, unlike relying on an unstated
/// std hashing guarantee, FNV-1a's algorithm is fixed and fully
/// deterministic across processes, platforms, and time, which is exactly
/// what "this identity must match T4" requires (a digest that must still
/// compare equal in a wholly separate process invocation, possibly much
/// later).
#[must_use]
pub fn fnv1a_64(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01B3;
    let mut hash = OFFSET_BASIS;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

/// Recorded identity of one exact target Copilot executable: its path,
/// best-effort `--version` output, and a diagnostic (non-cryptographic)
/// content digest/length. `identify_error` is populated, never panics,
/// when the executable cannot be read from disk, OR when the
/// `--version` probe itself failed to spawn, timed out, was truncated,
/// or exited with a non-zero/missing status ([`version_probe_failure_reason`]).
#[derive(Debug, Clone)]
pub struct CopilotIdentity {
    pub exe_path: String,
    pub version_output: Option<String>,
    pub content_hash_hex: String,
    pub content_len: u64,
    pub identify_error: Option<String>,
}

/// Turns a `--version` probe's raw outcome into a fail-closed
/// `identify_error` reason, or `None` when the probe genuinely succeeded
/// (a clean, non-timed-out, zero-exit run with a fully captured
/// stdout). A readable-but-unlaunchable, wedged, crashing, or
/// truncated-output CLI must never be treated as having a proven exact
/// identity just because its executable bytes were still readable from
/// disk -- `identify_copilot` folds this into `identify_error` exactly
/// like a `fs::read` failure, so `identity_failure_message` fails closed
/// on it too (Copilot review, 2026-09 -- 049-S PR #120, round 3).
fn version_probe_failure_reason(
    timed_out: bool,
    exit_code: Option<i32>,
    stdout_truncated: bool,
    spawn_error: Option<&str>,
) -> Option<String> {
    if let Some(err) = spawn_error {
        return Some(format!("version probe failed to spawn: {err}"));
    }
    if timed_out {
        return Some("version probe timed out before exiting".to_string());
    }
    match exit_code {
        None => Some("version probe exited without an observable status".to_string()),
        Some(code) if code != 0 => Some(format!("version probe exited with nonzero status {code}")),
        Some(_) if stdout_truncated => {
            Some("version probe output was truncated before it could be fully captured".to_string())
        }
        Some(_) => None,
    }
}

/// Invokes `exe_path --version` through the same deadline-governed
/// `ChildGuard`/pump-and-reap composition [`run_leg`] uses for the exact
/// Copilot CLI child, rather than a raw blocking `Command::output()` call.
/// A wedged or intentionally hanging `--version` invocation is killed at
/// `deadline` instead of blocking the entire probe run indefinitely.
///
/// Only stdout is captured into `version_output` (bounded, via
/// [`CaptureSink`]); stderr is drained -- to this process's own stderr,
/// same as every other child this crate spawns through
/// [`run_duplex_pump`] -- rather than combined into the returned string,
/// since the shared pump-and-reap primitive has no capture path for it.
/// This preserves stderr's diagnostic visibility without pulling
/// stderr-capture plumbing into the pump primitive shared by every other
/// call site.
///
/// A readable-but-unlaunchable or wedged CLI must never be reported as
/// having a proven identity merely because its bytes could still be
/// read from disk: [`version_probe_failure_reason`] turns a version
/// probe spawn error, timeout, truncation, or non-zero/missing exit
/// status into `identify_error` so [`identity_failure_message`] fails
/// closed on it exactly as it already does for an unreadable executable
/// (Copilot review, 2026-09 -- 049-S PR #120, round 3).
fn identify_copilot(exe_path: &str, deadline: Duration) -> CopilotIdentity {
    let mut command = Command::new(exe_path);
    command.arg("--version");
    let (stdout_bytes, timed_out, exit_code, stdout_truncated, spawn_error) =
        spawn_and_capture(command, deadline);
    let probe_failure = version_probe_failure_reason(
        timed_out,
        exit_code,
        stdout_truncated,
        spawn_error.as_deref(),
    );
    let version_output = if timed_out || exit_code.is_none() {
        None
    } else {
        let text = String::from_utf8_lossy(&stdout_bytes).trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    };

    match fs::read(exe_path) {
        Ok(bytes) => CopilotIdentity {
            exe_path: exe_path.to_string(),
            version_output,
            content_hash_hex: format!("{:016x}", fnv1a_64(&bytes)),
            content_len: bytes.len() as u64,
            identify_error: probe_failure,
        },
        Err(err) => CopilotIdentity {
            exe_path: exe_path.to_string(),
            version_output,
            content_hash_hex: String::new(),
            content_len: 0,
            identify_error: Some(err.to_string()),
        },
    }
}

/// Records identity (path, content hash, content length) for the
/// production `--inner-exe` this run's wrapper handoff encodes, WITHOUT
/// ever spawning it: unlike [`identify_copilot`]'s Copilot CLI targets,
/// the inner executable is an arbitrary MCP server binary with no
/// guaranteed `--version` contract, and invoking it outside the wrapper
/// handoff this task already owns would be a new, unauthorized execution
/// with unknown side effects (it might not even be a short-lived CLI).
/// `version_output` is therefore always `None` here -- this is
/// read-only, file-hash identity proof, the same proof
/// [`identify_copilot`] already provides via `fs::read` for the Copilot
/// targets, just without the extra `--version` invocation step (Copilot
/// review, 2026-09 -- 049-S PR #120; 056.001-T's own acceptance
/// criteria: "Record and hash the exact inner executable path").
fn identify_inner_exe(exe_path: &str) -> CopilotIdentity {
    match fs::read(exe_path) {
        Ok(bytes) => CopilotIdentity {
            exe_path: exe_path.to_string(),
            version_output: None,
            content_hash_hex: format!("{:016x}", fnv1a_64(&bytes)),
            content_len: bytes.len() as u64,
            identify_error: None,
        },
        Err(err) => CopilotIdentity {
            exe_path: exe_path.to_string(),
            version_output: None,
            content_hash_hex: String::new(),
            content_len: 0,
            identify_error: Some(err.to_string()),
        },
    }
}

/// Bounded buffer backing [`CaptureSink`]: never grows past
/// [`MAX_CAPTURE_BYTES`]. Once the cap is reached, further bytes are
/// discarded (the write still reports success to its caller -- a `Write`
/// impl reporting the byte count it was asked to accept, never the count
/// it actually retained, mirrors `tests/common/serve_driver.rs`'s
/// `BoundedCapture` and keeps the pump's own write-side never observing
/// an error solely because the cap was reached) and `truncated` is set.
#[derive(Debug, Default)]
struct CaptureBuf {
    bytes: Vec<u8>,
    truncated: bool,
}

impl CaptureBuf {
    fn push(&mut self, data: &[u8]) {
        if self.truncated {
            return;
        }
        let remaining = MAX_CAPTURE_BYTES.saturating_sub(self.bytes.len());
        if data.len() > remaining {
            self.bytes.extend_from_slice(&data[..remaining]);
            self.truncated = true;
        } else {
            self.bytes.extend_from_slice(data);
        }
    }
}

/// A `Write` sink that appends every write into a shared, lock-protected,
/// size-bounded buffer -- used as `run_duplex_pump`'s `outgoing` side so
/// the exact Copilot CLI child's own stdout is captured in memory rather
/// than forwarded anywhere, without introducing a second pump
/// implementation. Bounded (see [`CaptureBuf`]) so a verbose, misbehaving,
/// or adversarial child cannot grow the wrapper's memory without limit
/// for the entire leg deadline window.
#[derive(Clone, Default)]
struct CaptureSink(Arc<Mutex<CaptureBuf>>);

impl CaptureSink {
    fn new() -> Self {
        Self::default()
    }

    /// Consumes the sink, returning the captured bytes and whether the
    /// capture was truncated at [`MAX_CAPTURE_BYTES`].
    fn into_parts(self) -> (Vec<u8>, bool) {
        Arc::try_unwrap(self.0).map_or_else(
            |shared| {
                let guard = shared
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                (guard.bytes.clone(), guard.truncated)
            },
            |mutex| {
                let buf = mutex
                    .into_inner()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                (buf.bytes, buf.truncated)
            },
        )
    }
}

impl Write for CaptureSink {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Runs `command` (already fully configured except stdio) through the
/// shared bounded-capture composition: piped stdin/stdout/stderr, a
/// direct `ChildGuard`, and `run_duplex_pump` with an immediately-EOF
/// `incoming` side and a `CaptureSink` `outgoing` side. Returns the
/// captured stdout bytes, whether the deadline elapsed, and the child's
/// exit code (`None` only when the deadline elapsed and teardown killed
/// it instead of it exiting on its own) -- the exact same
/// pump-then-reap-or-kill shape `crate::process::run_wrapper` already
/// uses for its inner child, applied here to the exact Copilot CLI child.
/// Spawns `command` with fully piped stdio and wraps it in a
/// [`ChildGuard`] -- the sole kill/wait authority this runner ever uses
/// for the exact Copilot CLI child (or, for Gate 1, the `mcp get` child).
/// Splitting spawn from pump-and-reap (see [`pump_and_reap`]) lets
/// [`run_leg`] observe the freshly spawned child's pid -- to start the
/// bounded sentinel-inheritance watcher below -- before the blocking
/// pump call begins.
fn spawn_piped_child(mut command: Command) -> io::Result<ChildGuard> {
    // `run_duplex_pump` requires a *piped* child stdin/stdout (it takes
    // ownership via `Child::stdin`/`stdout` `.take()`); passing
    // `io::empty()` as the pump's `incoming` side immediately closes the
    // piped stdin handle once the pump observes EOF, which is
    // functionally equivalent to `Stdio::null()` from the child's point
    // of view while still satisfying the pump's piping contract.
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = command.spawn()?;
    Ok(ChildGuard::new(child, "exact-cli-child"))
}

/// Drives `guard`'s owned child through the shared bounded-capture
/// composition: `run_duplex_pump` with an immediately-EOF `incoming`
/// side and a `CaptureSink` `outgoing` side, then reaps or (on deadline)
/// kills the child via the SAME pump-then-reap-or-kill shape
/// `crate::process::run_wrapper` already uses for its inner child,
/// applied here to the exact Copilot CLI child. Returns the captured
/// stdout bytes, whether the deadline elapsed, the child's exit code
/// (`None` only when the deadline elapsed and teardown killed it instead
/// of it exiting on its own), and whether the stdout capture was
/// truncated at [`MAX_CAPTURE_BYTES`].
fn pump_and_reap(guard: &mut ChildGuard, deadline: Duration) -> (Vec<u8>, bool, Option<i32>, bool) {
    let capture = CaptureSink::new();
    let pump_config = PumpConfig {
        deadline: Some(deadline),
        ..PumpConfig::default()
    };
    let pump = run_duplex_pump(
        io::empty(),
        capture.clone(),
        guard.child_mut(),
        &pump_config,
        None,
    );

    let (timed_out, exit_code) = match pump {
        Ok(outcome) if outcome.timed_out => {
            guard.kill_and_wait();
            (true, None)
        }
        Ok(_) => (false, guard.wait().ok().and_then(|status| status.code())),
        Err(_) => {
            guard.kill_and_wait();
            (false, None)
        }
    };

    let (stdout_bytes, stdout_truncated) = capture.into_parts();
    (stdout_bytes, timed_out, exit_code, stdout_truncated)
}

/// Spawns and bounded-captures `command` in one call, for callers (Gate
/// 1) that need no concurrent sentinel watcher.
///
/// Returns empty output, `timed_out: false`, `exit_code: None`, and
/// `stdout_truncated: false` if the child could not even be spawned; the
/// fifth element carries `Some(spawn error message)` in that exact case
/// and `None` in every other case, so a caller that gates a causal
/// classification on `exit_code`/`timed_out` alone (as plain process
/// output) can still fail closed on an unlaunchable executable rather
/// than silently treating it the same as an ordinary non-zero exit
/// (Copilot review, 2026-09 -- 049-S PR #120).
fn spawn_and_capture(
    command: Command,
    deadline: Duration,
) -> (Vec<u8>, bool, Option<i32>, bool, Option<String>) {
    match spawn_piped_child(command) {
        Ok(mut guard) => {
            let (stdout_bytes, timed_out, exit_code, stdout_truncated) =
                pump_and_reap(&mut guard, deadline);
            (stdout_bytes, timed_out, exit_code, stdout_truncated, None)
        }
        Err(err) => (Vec::new(), false, None, false, Some(err.to_string())),
    }
}

/// Best-effort, honestly-labeled result of watching for the wrapper's
/// own inherited environment. `observed: false` NEVER proves
/// non-inheritance -- it only means this bounded, best-effort watcher
/// did not catch a matching descendant with the sentinel present before
/// the leg concluded (the wrapper may have already exited, or this
/// platform's process-environment introspection may be
/// unavailable/restricted for the given process).
#[derive(Debug, Clone)]
pub struct SentinelObservation {
    pub observed: bool,
    pub observed_pid: Option<u32>,
    pub observed_exe: Option<String>,
    pub note: String,
}

impl SentinelObservation {
    fn not_observed(note: impl Into<String>) -> Self {
        Self {
            observed: false,
            observed_pid: None,
            observed_exe: None,
            note: note.into(),
        }
    }
}

/// Polls, on a bounded interval, for a direct child of `root_pid` whose
/// own executable path matches `wrapper_exe` (the wrapper is always a
/// re-invocation of this exact `mcp-probe` binary) and whose OS-reported
/// process environment (`sysinfo::Process::environ`) contains the exact
/// `sentinel_var=sentinel_value` entry. Returns as soon as a match is
/// found, or once `stop` is observed set (checked once per iteration,
/// after that iteration's own scan -- so the very last scan before
/// stopping is never skipped).
fn watch_sentinel_inheritance(
    root_pid: u32,
    wrapper_exe: &Path,
    sentinel_var: &str,
    sentinel_value: &str,
    stop: &AtomicBool,
    poll_interval: Duration,
) -> SentinelObservation {
    let expected_entry = format!("{sentinel_var}={sentinel_value}");
    let wrapper_exe_canon = fs::canonicalize(wrapper_exe).ok();
    if wrapper_exe_canon.is_none() {
        return SentinelObservation::not_observed(
            "this binary's own absolute path could not be canonicalized; sentinel-inheritance \
             watching was skipped",
        );
    }

    let root = Pid::from_u32(root_pid);
    let mut system = System::new();

    loop {
        system.refresh_processes_specifics(
            ProcessRefreshKind::everything().with_environ(UpdateKind::Always),
        );
        for (pid, process) in system.processes() {
            if process.parent() != Some(root) {
                continue;
            }
            let exe_matches = process
                .exe()
                .and_then(|exe_path| fs::canonicalize(exe_path).ok())
                .zip(wrapper_exe_canon.as_ref())
                .is_some_and(|(observed, expected)| &observed == expected);
            if !exe_matches {
                continue;
            }
            if process
                .environ()
                .iter()
                .any(|entry| entry == &expected_entry)
            {
                return SentinelObservation {
                    observed: true,
                    observed_pid: Some(pid.as_u32()),
                    observed_exe: process.exe().map(|p| p.to_string_lossy().into_owned()),
                    note: "a direct child of the exact-CLI process, whose own executable path \
                           matches this binary, was observed with the sentinel present in its \
                           process environment"
                        .to_string(),
                };
            }
        }
        if stop.load(Ordering::SeqCst) {
            break;
        }
        thread::sleep(poll_interval);
    }

    SentinelObservation::not_observed(
        "no direct child matching this binary's own executable path was observed with the \
         sentinel present in its process environment before this leg concluded; this is a \
         best-effort, timing-dependent observation and does NOT prove non-inheritance",
    )
}

/// Starts the bounded sentinel-inheritance watcher on its own thread and
/// returns a stop flag (signal via `Ordering::SeqCst` store, then
/// [`JoinHandle::join`] to retrieve the final [`SentinelObservation`]).
fn spawn_sentinel_watcher(
    root_pid: u32,
    wrapper_exe: PathBuf,
    sentinel_value: String,
) -> (Arc<AtomicBool>, JoinHandle<SentinelObservation>) {
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_for_thread = Arc::clone(&stop_flag);
    let handle = thread::spawn(move || {
        watch_sentinel_inheritance(
            root_pid,
            &wrapper_exe,
            ENV_INHERITANCE_SENTINEL_VAR,
            &sentinel_value,
            &stop_flag_for_thread,
            Duration::from_millis(150),
        )
    });
    (stop_flag, handle)
}

/// Outcome of Gate 1: proving that the exact target CLI's nearest-child
/// `.mcp.json` shadows -- and does not merge -- the deliberately
/// invalid/sentinel ancestor config, using only static config resolution
/// (`mcp get --json`), never a live session.
#[derive(Debug, Clone)]
pub struct Gate1Outcome {
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub resolved_source_path: Option<String>,
    pub expected_source_path: String,
    pub raw_stdout: String,
    pub parse_error: Option<String>,
    /// `Some(message)` only when the exact target CLI could not even be
    /// spawned for this gate (for example, an unlaunchable or missing
    /// `--copilot-exe`); `None` for every other outcome, including a
    /// genuine non-zero exit or timeout. This is the fail-closed signal
    /// `run_exact_cli` uses to distinguish "isolation was never proven
    /// because the CLI never ran" from "isolation was proven to have
    /// failed" -- the two are never collapsed into the same
    /// `H3-B-candidate` classification (Copilot review, 2026-09 -- 049-S
    /// PR #120).
    pub spawn_error: Option<String>,
}

fn run_gate1(
    copilot_exe: &str,
    workspace: &ProbeWorkspace,
    entry_name: &str,
    deadline: Duration,
) -> Gate1Outcome {
    let mut command = Command::new(copilot_exe);
    command
        .arg("-C")
        .arg(workspace.ancestor_run_dir())
        .arg("mcp")
        .arg("get")
        .arg(entry_name)
        .arg("--json");

    // Gate 1's `mcp get --json` output is small structured JSON; the
    // truncation flag is not surfaced on `Gate1Outcome` (unlike
    // `LegOutcome`, see `run_leg`) since a bounded config-resolution
    // response truncating would already fail JSON parsing below and be
    // reported via `parse_error`.
    let (stdout_bytes, timed_out, exit_code, _stdout_truncated, spawn_error) =
        spawn_and_capture(command, deadline);
    let raw_stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
    let expected_source_path = workspace
        .ancestor_run_config_path()
        .to_string_lossy()
        .into_owned();

    if timed_out || exit_code != Some(0) {
        return Gate1Outcome {
            passed: false,
            exit_code,
            timed_out,
            resolved_source_path: None,
            expected_source_path,
            raw_stdout,
            parse_error: None,
            spawn_error,
        };
    }

    match serde_json::from_str::<serde_json::Value>(&raw_stdout) {
        Ok(value) => {
            let resolved_source_path = value
                .get(entry_name)
                .and_then(|entry| entry.get("sourcePath"))
                .and_then(|path| path.as_str())
                .map(std::string::ToString::to_string);
            let passed = resolved_source_path.as_deref() == Some(expected_source_path.as_str());
            Gate1Outcome {
                passed,
                exit_code,
                timed_out,
                resolved_source_path,
                expected_source_path,
                raw_stdout,
                parse_error: None,
                spawn_error: None,
            }
        }
        Err(err) => Gate1Outcome {
            passed: false,
            exit_code,
            timed_out,
            resolved_source_path: None,
            expected_source_path,
            raw_stdout,
            parse_error: Some(err.to_string()),
            spawn_error: None,
        },
    }
}

/// Which of the two byte-identical-except-`cwd` legs a [`LegOutcome`]
/// describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leg {
    Control,
    Treatment,
}

impl Leg {
    fn as_str(self) -> &'static str {
        match self {
            Self::Control => "control",
            Self::Treatment => "treatment",
        }
    }
}

/// Outcome of one causal-pass leg: the exact Copilot CLI's own observed
/// behavior (exit code, deadline/timeout, last MCP status transition for
/// the probed entry) plus the redacted wrapper evidence summary read back
/// from the shared `--evidence-output` file, captured before the next leg
/// can overwrite it; and this leg's own best-effort
/// [`SentinelObservation`].
#[derive(Debug, Clone)]
pub struct LegOutcome {
    pub leg: Leg,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub last_mcp_status: Option<String>,
    pub session_result_exit_code: Option<i64>,
    pub sentinel_observation: SentinelObservation,
    pub wrapper_evidence: Option<serde_json::Value>,
    pub wrapper_evidence_read_error: Option<String>,
    /// Whether this leg's exact-CLI child stdout capture was truncated
    /// at [`MAX_CAPTURE_BYTES`] (see [`CaptureSink`]). A truncated
    /// capture means [`extract_last_mcp_status`] may have missed a later
    /// status transition that arrived after the cap -- callers should
    /// treat `last_mcp_status`/`session_result_exit_code` as
    /// non-authoritative when this is `true`.
    pub stdout_truncated: bool,
}

fn extract_last_mcp_status(stdout: &str, entry_name: &str) -> (Option<String>, Option<i64>) {
    let mut last_status = None;
    let mut result_exit_code = None;
    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let event_type = value
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if event_type == "session.mcp_server_status_changed" {
            let server_name = value
                .get("data")
                .and_then(|data| data.get("serverName"))
                .and_then(|name| name.as_str());
            if server_name == Some(entry_name) {
                if let Some(status) = value
                    .get("data")
                    .and_then(|data| data.get("status"))
                    .and_then(|s| s.as_str())
                {
                    last_status = Some(status.to_string());
                }
            }
        } else if event_type == "result" {
            result_exit_code = value.get("exitCode").and_then(serde_json::Value::as_i64);
        }
    }
    (last_status, result_exit_code)
}

fn read_wrapper_evidence(path: &Path) -> (Option<serde_json::Value>, Option<String>) {
    match fs::read_to_string(path) {
        Ok(contents) => match serde_json::from_str::<serde_json::Value>(&contents) {
            Ok(value) => (Some(value), None),
            Err(err) => (
                None,
                Some(format!("evidence file is not valid JSON: {err}")),
            ),
        },
        Err(err) if err.kind() == io::ErrorKind::NotFound => (None, None),
        Err(err) => (None, Some(format!("failed to read evidence file: {err}"))),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_leg(
    leg: Leg,
    copilot_exe: &str,
    leg_dir: &Path,
    evidence_output: &Path,
    entry_name: &str,
    prompt: &str,
    sentinel_value: &str,
    wrapper_exe: &Path,
    deadline: Duration,
) -> LegOutcome {
    // A leg-scoped copy destination so the shared evidence_output path
    // (deliberately one path across both legs -- see workspace.rs) can be
    // safely overwritten by the very next leg without losing this leg's
    // own evidence.
    let leg_evidence_copy = leg_dir.join("wrapper-evidence-snapshot.json");
    // Best-effort: an evidence file may not exist yet from a prior run at
    // this exact path; ignore a missing source here, it is handled below.
    let _ = fs::remove_file(&leg_evidence_copy);

    // Invalidate the SHARED evidence_output before this leg's own wrapper
    // gets a chance to run. Both legs of a pass deliberately point at one
    // evidence_output path (see workspace.rs's module docs), and both legs
    // of a pass share the very same `run_nonce` too -- so a `run_nonce`
    // equality check on the copied evidence could never distinguish
    // "this leg's own evidence" from "the other leg's still-present
    // evidence" within one pass. Removing the shared file here instead
    // means: if this leg's wrapper never spawns, crashes, or otherwise
    // fails to (re)write evidence_output, the `fs::copy` below simply
    // fails against a missing source, and this leg is correctly recorded
    // as having no wrapper evidence rather than silently inheriting and
    // misattributing a PRIOR leg's still-present evidence file.
    let _ = fs::remove_file(evidence_output);

    let mut command = Command::new(copilot_exe);
    command
        .arg("-C")
        .arg(leg_dir)
        .arg("-p")
        .arg(prompt)
        .arg("--allow-all-tools")
        .arg("--disable-builtin-mcps")
        .arg("--no-color")
        .arg("--no-auto-update")
        .arg("--output-format")
        .arg("json")
        .arg("-s")
        .env(ENV_INHERITANCE_SENTINEL_VAR, sentinel_value);

    let (stdout_bytes, timed_out, exit_code, sentinel_observation, stdout_truncated) =
        match spawn_piped_child(command) {
            Ok(mut guard) => {
                let watcher = guard.pid().map(|pid| {
                    spawn_sentinel_watcher(
                        pid,
                        wrapper_exe.to_path_buf(),
                        sentinel_value.to_string(),
                    )
                });
                let (stdout_bytes, timed_out, exit_code, stdout_truncated) =
                    pump_and_reap(&mut guard, deadline);
                let sentinel_observation = match watcher {
                    Some((stop_flag, handle)) => {
                        stop_flag.store(true, Ordering::SeqCst);
                        handle.join().unwrap_or_else(|_| {
                            SentinelObservation::not_observed(
                                "the sentinel-inheritance watcher thread panicked; treated as \
                                 a best-effort non-observation, never as proof of \
                                 non-inheritance",
                            )
                        })
                    }
                    None => SentinelObservation::not_observed(
                        "this leg's exact-CLI child process id was unavailable; \
                         sentinel-inheritance watching was skipped",
                    ),
                };
                (
                    stdout_bytes,
                    timed_out,
                    exit_code,
                    sentinel_observation,
                    stdout_truncated,
                )
            }
            Err(_) => (
                Vec::new(),
                false,
                None,
                SentinelObservation::not_observed(
                    "the exact-CLI child process could not be spawned for this leg; \
                     sentinel-inheritance watching was skipped",
                ),
                false,
            ),
        };
    let stdout_text = String::from_utf8_lossy(&stdout_bytes).into_owned();
    let (last_mcp_status, session_result_exit_code) =
        extract_last_mcp_status(&stdout_text, entry_name);

    // Snapshot the shared evidence file for this leg before it can be
    // overwritten by whichever leg runs next.
    let snapshot_result = fs::copy(evidence_output, &leg_evidence_copy);
    let (wrapper_evidence, wrapper_evidence_read_error) = if snapshot_result.is_ok() {
        read_wrapper_evidence(&leg_evidence_copy)
    } else {
        (None, None)
    };

    LegOutcome {
        leg,
        exit_code,
        timed_out,
        last_mcp_status,
        session_result_exit_code,
        sentinel_observation,
        wrapper_evidence,
        wrapper_evidence_read_error,
        stdout_truncated,
    }
}

/// One control/treatment pair run against one Copilot build ("affected"
/// or "stable").
#[derive(Debug, Clone)]
pub struct PassOutcome {
    pub build: String,
    pub copilot_identity: CopilotIdentity,
    pub control: LegOutcome,
    pub treatment: LegOutcome,
}

fn run_pass(
    build: &str,
    copilot_exe: &str,
    workspace: &ProbeWorkspace,
    args: &ExactCliArgs,
    wrapper_exe: &Path,
    sentinel_value: &str,
) -> PassOutcome {
    let copilot_identity = identify_copilot(copilot_exe, args.gate1_deadline);
    let control = run_leg(
        Leg::Control,
        copilot_exe,
        workspace.control_dir(),
        workspace.evidence_output(),
        &args.entry_name,
        &args.prompt,
        sentinel_value,
        wrapper_exe,
        args.leg_deadline,
    );
    let treatment = run_leg(
        Leg::Treatment,
        copilot_exe,
        workspace.treatment_dir(),
        workspace.evidence_output(),
        &args.entry_name,
        &args.prompt,
        sentinel_value,
        wrapper_exe,
        args.leg_deadline,
    );

    PassOutcome {
        build: build.to_string(),
        copilot_identity,
        control,
        treatment,
    }
}

/// A leg is only "connected" when `last_mcp_status` reports it AND the
/// stdout capture that produced that status was not truncated. A
/// truncated capture means a later status transition (for example a
/// disconnect) could have arrived after the capture cap and been
/// discarded, so `last_mcp_status == Some("connected")` observed from a
/// truncated capture is never authoritative on its own (Copilot review,
/// 2026-09 -- 049-S PR #120, round 2; see [`LegOutcome::stdout_truncated`]'s
/// own doc comment).
fn leg_connected(leg: &LegOutcome) -> bool {
    !leg.stdout_truncated && leg.last_mcp_status.as_deref() == Some("connected")
}

/// Requires BOTH a well-formed, non-null `initialize_correlation` AND
/// the evidence summary's own top-level `valid` flag being `true`.
/// `valid: false` (with a populated `invalid_reason`) is the collector's
/// own signal that it detected a problem (e.g. internal channel
/// saturation, see `EvidenceCollector::new_with_capacity`'s doc comment)
/// -- evidence explicitly marked invalid must never be treated as "this
/// leg had a valid initialize" merely because `initialize_correlation`
/// happened to populate before the invalidating event occurred.
fn leg_has_valid_initialize(leg: &LegOutcome) -> bool {
    leg.wrapper_evidence.as_ref().is_some_and(|value| {
        value.get("valid").and_then(serde_json::Value::as_bool) == Some(true)
            && value
                .get("initialize_correlation")
                .is_some_and(|correlation| !correlation.is_null())
    })
}

/// Produces the current ordered cause classification from one pass's
/// control/treatment observations. Never overclaims: when the concrete
/// signals this runner controls (MCP status transition, wrapper evidence
/// validity/initialize-correlation presence, exit/timeout behavior) do not
/// cleanly match one hypothesis, this records the ambiguity plainly rather
/// than forcing a conclusion. `H3-A` is deliberately never asserted here:
/// per the deliberation doc's 2026-08-29 Addendum it is already confirmed
/// via prior live-client evidence and owned downstream by `056.011-T`;
/// this classifier only ever reports what this specific run's own control
/// /treatment signals show.
fn classify_pass(pass: &PassOutcome) -> Vec<String> {
    let mut causes = Vec::new();
    let control = &pass.control;
    let treatment = &pass.treatment;

    let control_ok = leg_connected(control) && leg_has_valid_initialize(control);
    let treatment_ok = leg_connected(treatment) && leg_has_valid_initialize(treatment);

    if !control_ok && treatment_ok {
        causes.push(format!(
            "H0a (cwd-relative database discovery gap) retained for build '{}': control leg \
             (no cwd override, entry_name={:?}) did not reach a connected/initialized state \
             (last_mcp_status={:?}, wrapper_evidence_valid={:?}), while the treatment leg \
             (cwd corrected to the canonical repository root) connected and completed the \
             initialize handshake.",
            pass.build,
            pass.control.leg.as_str(),
            control.last_mcp_status,
            control
                .wrapper_evidence
                .as_ref()
                .and_then(|value| value.get("valid"))
                .and_then(serde_json::Value::as_bool),
        ));
    } else if control_ok && treatment_ok {
        causes.push(format!(
            "No H0a reproduction for build '{}': both control and treatment legs reached a \
             connected state with a valid initialize correlation in this environment.",
            pass.build
        ));
    } else if !control_ok && !treatment_ok {
        causes.push(format!(
            "Cause not resolved by cwd alone for build '{}': neither leg reached a connected, \
             initialize-correlated state (control last_mcp_status={:?}, \
             control_stdout_truncated={}, treatment last_mcp_status={:?}, \
             treatment_stdout_truncated={}). See each leg's wrapper evidence \
             (invalid_reason/events/exit-vs-still-alive) for framing-level detail.",
            pass.build,
            control.last_mcp_status,
            control.stdout_truncated,
            treatment.last_mcp_status,
            treatment.stdout_truncated,
        ));
    } else {
        causes.push(format!(
            "Unexpected asymmetry for build '{}': control leg connected/initialized but the \
             treatment leg (cwd-corrected) did not (control last_mcp_status={:?}, \
             control_stdout_truncated={}, treatment last_mcp_status={:?}, \
             treatment_stdout_truncated={}); recorded as-observed rather than forced into an \
             existing hypothesis.",
            pass.build,
            control.last_mcp_status,
            control.stdout_truncated,
            treatment.last_mcp_status,
            treatment.stdout_truncated,
        ));
    }

    causes
}

/// The full recorded outcome of one `exact-cli` run: identity, Gate 1,
/// zero or more causal passes, the ordered cause classification, and the
/// terminal disposition. Serializes to the structured JSON this task
/// prints to stdout and persists under the probe workspace.
// The `ExactCli` prefix is deliberate and clearer than a bare `Outcome`
// for a type re-exported from the crate root's public API surface.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone)]
pub struct ExactCliOutcome {
    pub run_nonce: String,
    pub sentinel_env_var: String,
    pub sentinel_value: String,
    pub manual_verification_invocation: String,
    pub workspace_root: PathBuf,
    pub affected_identity: CopilotIdentity,
    pub stable_identity: Option<CopilotIdentity>,
    /// Read-only, file-hash identity (path/content-hash/content-length)
    /// of the production `--inner-exe` this run's wrapper handoff
    /// encodes -- see [`identify_inner_exe`]'s doc comment for why this
    /// is never spawned to obtain a `version_output`.
    pub inner_identity: CopilotIdentity,
    pub gate1: Gate1Outcome,
    pub passes: Vec<PassOutcome>,
    pub ordered_cause_classification: Vec<String>,
    pub h3_b_candidate: bool,
    pub terminal: &'static str,
}

fn copilot_identity_to_json(identity: &CopilotIdentity) -> serde_json::Value {
    serde_json::json!({
        "exe_path": identity.exe_path,
        "version_output": identity.version_output,
        "content_hash_hex": identity.content_hash_hex,
        "content_hash_algorithm": "FNV-1a-64 (non-cryptographic, diagnostic identity comparison only)",
        "content_len": identity.content_len,
        "identify_error": identity.identify_error,
    })
}

fn leg_outcome_to_json(leg: &LegOutcome) -> serde_json::Value {
    serde_json::json!({
        "leg": leg.leg.as_str(),
        "exit_code": leg.exit_code,
        "timed_out": leg.timed_out,
        "last_mcp_status": leg.last_mcp_status,
        "session_result_exit_code": leg.session_result_exit_code,
        "sentinel_observation": {
            "observed": leg.sentinel_observation.observed,
            "observed_pid": leg.sentinel_observation.observed_pid,
            "observed_exe": leg.sentinel_observation.observed_exe,
            "note": leg.sentinel_observation.note,
        },
        "wrapper_evidence": leg.wrapper_evidence,
        "wrapper_evidence_read_error": leg.wrapper_evidence_read_error,
        "stdout_truncated": leg.stdout_truncated,
    })
}

fn pass_outcome_to_json(pass: &PassOutcome) -> serde_json::Value {
    serde_json::json!({
        "build": pass.build,
        "copilot_identity": copilot_identity_to_json(&pass.copilot_identity),
        "control": leg_outcome_to_json(&pass.control),
        "treatment": leg_outcome_to_json(&pass.treatment),
        "classification": classify_pass(pass),
    })
}

/// Serializes the full outcome to the structured JSON this task both
/// prints to stdout and persists under the probe workspace for durable,
/// redacted-only evidence.
#[must_use]
pub fn outcome_to_json(outcome: &ExactCliOutcome) -> serde_json::Value {
    serde_json::json!({
        "run_nonce": outcome.run_nonce,
        "sentinel_env_var": outcome.sentinel_env_var,
        "sentinel_value": outcome.sentinel_value,
        "manual_verification_invocation": outcome.manual_verification_invocation,
        "workspace_root": outcome.workspace_root.to_string_lossy(),
        "affected_identity": copilot_identity_to_json(&outcome.affected_identity),
        "stable_identity": outcome.stable_identity.as_ref().map(copilot_identity_to_json),
        "inner_identity": copilot_identity_to_json(&outcome.inner_identity),
        "gate1": {
            "passed": outcome.gate1.passed,
            "exit_code": outcome.gate1.exit_code,
            "timed_out": outcome.gate1.timed_out,
            "resolved_source_path": outcome.gate1.resolved_source_path,
            "expected_source_path": outcome.gate1.expected_source_path,
            "parse_error": outcome.gate1.parse_error,
            "spawn_error": outcome.gate1.spawn_error,
        },
        "passes": outcome.passes.iter().map(pass_outcome_to_json).collect::<Vec<_>>(),
        "ordered_cause_classification": outcome.ordered_cause_classification,
        "h3_b_candidate": outcome.h3_b_candidate,
        "terminal": outcome.terminal,
    })
}

/// Result file name this task persists under [`ExactCliOutcome::workspace_root`].
pub const RESULT_FILE_NAME: &str = "exact-cli-result.json";

/// Serializes `outcome` via [`outcome_to_json`] and atomically writes it
/// under `outcome.workspace_root` as [`RESULT_FILE_NAME`] (temp file plus
/// rename, the same create-then-rename shape
/// `evidence::write_evidence_output` uses for its own file), returning
/// the path written on success.
///
/// This is `exact_cli`'s own task-output persistence -- distinct from,
/// and never a reimplementation of, `056.023-T`'s owned evidence-capture
/// module -- delivering the durable, on-disk persistence this task's own
/// doc comments already claimed (see [`outcome_to_json`] and
/// [`ExactCliOutcome`]) but that previously only ever happened if an
/// external caller chose to redirect stdout (Copilot review, 2026-09 --
/// 049-S PR #120).
///
/// This result embeds each leg's `wrapper_evidence`, the same
/// evidence-summary content `evidence::write_evidence_output` protects
/// with owner-only (Unix `0o600`) permissions -- key-based redaction
/// cannot catch a secret embedded inside an otherwise benign-keyed
/// value, so this file is created with the identical owner-only mode at
/// creation time, never a separate `set_permissions` call afterward
/// (Copilot review, 2026-09 -- 049-S PR #120, round 2).
///
/// # Errors
///
/// Returns an error if the outcome cannot be serialized, the temporary
/// file cannot be created/written/flushed, or the final rename fails.
pub fn persist_outcome_json(outcome: &ExactCliOutcome) -> io::Result<PathBuf> {
    let json = outcome_to_json(outcome);
    let bytes = serde_json::to_vec_pretty(&json)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

    let final_path = outcome.workspace_root.join(RESULT_FILE_NAME);
    let tmp_path = outcome
        .workspace_root
        .join(format!(".{RESULT_FILE_NAME}.tmp-{}", std::process::id()));

    {
        let mut open_opts = fs::OpenOptions::new();
        open_opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            open_opts.mode(0o600);
        }
        let mut file = open_opts.open(&tmp_path)?;
        file.write_all(&bytes)?;
        file.flush()?;
    }
    fs::rename(&tmp_path, &final_path)?;
    Ok(final_path)
}

/// `true` only when Gate 1 ran to completion, its output parsed cleanly,
/// and it resolved a CONCRETE source path that does not match the
/// expected isolated-child path -- i.e. positive evidence that the exact
/// CLI actually read or merged some other (presumably ancestor) config,
/// which is the one situation 056.001-T's own acceptance criteria call
/// an `H3-B-candidate` ("If the exact CLI reads or merges the ancestor
/// config ... emit typed H3-B-candidate evidence"). Every other
/// `!gate1.passed` case -- a spawn failure, a timeout, a non-zero exit,
/// a JSON parse error, or a clean parse with no `sourcePath` field at
/// all -- proves NOTHING about whether isolation held or failed, so none
/// of them may be classified as `H3-B-candidate` (Copilot review,
/// 2026-09 -- 049-S PR #120, round 2).
fn gate1_proves_ancestor_merge(gate1: &Gate1Outcome) -> bool {
    !gate1.passed
        && gate1.spawn_error.is_none()
        && !gate1.timed_out
        && gate1.parse_error.is_none()
        && gate1.exit_code == Some(0)
        && gate1.resolved_source_path.is_some()
}

/// Builds the terminal [`ExactCliOutcome`] for a Gate 1 failure: either
/// the `"done"` H3-B-candidate terminal (Gate 1 ran to completion, parsed
/// cleanly, and positively resolved a different config -- see
/// [`gate1_proves_ancestor_merge`]) or a `"blocked"` terminal covering
/// every other case (spawn failure, timeout, non-zero exit, parse
/// error, or a clean-but-empty resolution), since none of those prove
/// isolation failed either. Extracted from [`run_exact_cli`] to keep
/// that function under `clippy::too_many_lines`'s pedantic threshold.
#[allow(clippy::too_many_arguments)]
fn gate1_failure_outcome(
    run_nonce: String,
    sentinel_value: String,
    workspace_root: PathBuf,
    affected_identity: CopilotIdentity,
    stable_identity: Option<CopilotIdentity>,
    inner_identity: CopilotIdentity,
    gate1: Gate1Outcome,
) -> ExactCliOutcome {
    if gate1_proves_ancestor_merge(&gate1) {
        return ExactCliOutcome {
            run_nonce,
            sentinel_env_var: ENV_INHERITANCE_SENTINEL_VAR.to_string(),
            sentinel_value,
            manual_verification_invocation: MANUAL_VERIFICATION_INVOCATION.to_string(),
            workspace_root,
            affected_identity,
            stable_identity,
            inner_identity,
            gate1,
            passes: Vec::new(),
            ordered_cause_classification: vec![
                "H3-B-candidate: Gate 1 (ancestor config-isolation) did not prove the nearest \
                 child .mcp.json shadowed the sentinel ancestor without merging it; causal H0 \
                 comparison stopped, forwarding to 056.019-T (the sole H3-B terminal)."
                    .to_string(),
            ],
            h3_b_candidate: true,
            terminal: "done",
        };
    }

    // Every other `!passed` shape -- a spawn failure, a deadline timeout,
    // a non-zero exit, a JSON parse error, or a clean parse with no
    // `sourcePath` at all -- fails closed here: isolation was never
    // PROVEN either way, so none of these are the same signal as a
    // genuine Gate 1 run that positively observed a merged/ambiguous
    // ancestor config (056.001-T's own acceptance criteria: "Fail closed
    // when ... ancestor config-isolation ... is unproved").
    let reason = if let Some(spawn_error) = &gate1.spawn_error {
        format!("could not even spawn the exact target CLI ({spawn_error})")
    } else if gate1.timed_out {
        "timed out before it produced a result".to_string()
    } else if let Some(parse_error) = &gate1.parse_error {
        format!("produced output that could not be parsed as JSON ({parse_error})")
    } else if gate1.exit_code != Some(0) {
        format!("exited non-zero ({:?})", gate1.exit_code)
    } else {
        "exited zero and parsed cleanly but resolved no sourcePath at all, so no ancestor-merge \
         evidence was obtained either way"
            .to_string()
    };
    ExactCliOutcome {
        run_nonce,
        sentinel_env_var: ENV_INHERITANCE_SENTINEL_VAR.to_string(),
        sentinel_value,
        manual_verification_invocation: MANUAL_VERIFICATION_INVOCATION.to_string(),
        workspace_root,
        affected_identity,
        stable_identity,
        inner_identity,
        gate1,
        passes: Vec::new(),
        ordered_cause_classification: vec![format!(
            "blocked: Gate 1 (ancestor config-isolation) {reason}; isolation was never proven \
             either way, so no causal classification is emitted and this is NOT an \
             H3-B-candidate."
        )],
        h3_b_candidate: false,
        terminal: "blocked",
    }
}

/// Returns a `"blocked"` classification message the first time one of
/// the three recorded identities (`affected`, optional `stable`, or
/// `inner`) carries an `identify_error`, or `None` if all recorded
/// identities were read/hashed successfully. 056.001-T's own acceptance
/// criteria require failing closed when "exact CLI identity ... or
/// same-inner-executable control/treatment parity is unproved" --
/// merely RECORDING an identify failure (as the pre-existing
/// `CopilotIdentity.identify_error` field already did) is not the same
/// as ENFORCING it, so this check is applied unconditionally, ahead of
/// any Gate 1 or causal-pass classification (Copilot review, 2026-09 --
/// 049-S PR #120, round 2).
fn identity_failure_message(
    affected_identity: &CopilotIdentity,
    stable_identity: Option<&CopilotIdentity>,
    inner_identity: &CopilotIdentity,
) -> Option<String> {
    if let Some(err) = &affected_identity.identify_error {
        return Some(format!(
            "blocked: the exact target Copilot CLI's identity could not be recorded/hashed \
             ({err}); exact CLI identity is unproved, so no causal classification is emitted \
             (056.001-T: \"Fail closed when ... exact CLI identity ... is unproved\")."
        ));
    }
    if let Some(err) = stable_identity.and_then(|identity| identity.identify_error.as_ref()) {
        return Some(format!(
            "blocked: the last-known-stable Copilot CLI's identity could not be recorded/hashed \
             ({err}); exact CLI identity is unproved for the stable-build comparison, so no \
             causal classification is emitted (056.001-T: \"Fail closed when ... exact CLI \
             identity ... is unproved\")."
        ));
    }
    if let Some(err) = &inner_identity.identify_error {
        return Some(format!(
            "blocked: the inner executable's identity could not be recorded/hashed ({err}); \
             same-inner-executable control/treatment parity is unproved, so no causal \
             classification is emitted (056.001-T: \"Fail closed when ... same-inner-executable \
             control/treatment parity is unproved\")."
        ));
    }
    None
}

/// Runs the full `exact-cli` classification: creates the isolated probe
/// workspace, records both target identities, proves Gate 1, and -- only
/// if Gate 1 passes -- runs the bounded one-shot control/treatment
/// causal pass(es) and emits the ordered cause classification.
///
/// `terminal` is `"done"` for a normal completion (with `h3_b_candidate`
/// distinguishing a causal classification from a Gate-1-detected
/// ancestor-merge H3-B-candidate), or `"blocked"` when Gate 1 itself
/// could not even spawn the exact target CLI -- a genuine, explicit
/// non-H3-B evidence-capture blocker that must never be reported as an
/// `H3-B-candidate`, since isolation was never proven either way in that
/// case (Copilot review, 2026-09 -- 049-S PR #120).
///
/// # Errors
///
/// Returns an error string only when the isolated probe workspace itself
/// could not be created (a genuine, non-H3-B evidence-capture blocker);
/// every other outcome -- including a Gate 1 failure or Gate 1 spawn
/// blocker -- is represented in the returned [`ExactCliOutcome`], never
/// as an `Err`.
// The `exact_cli` prefix is deliberate and clearer than a bare `run` for
// a function re-exported from the crate root's public API surface.
#[allow(clippy::module_name_repetitions)]
pub fn run_exact_cli(args: &ExactCliArgs) -> Result<ExactCliOutcome, String> {
    let run_nonce = args.run_nonce.clone().unwrap_or_else(generate_run_nonce);
    let sentinel_value = args
        .sentinel_value
        .clone()
        .unwrap_or_else(|| format!("sentinel-{run_nonce}"));

    let repo_root = Path::new(&args.repo_root);
    let wrapper_exe_path = std::env::current_exe()
        .map_err(|err| format!("failed to resolve this binary's own absolute path: {err}"))?;
    let wrapper_exe = wrapper_exe_path.to_string_lossy().into_owned();

    let entry = McpServerEntrySpec {
        entry_name: args.entry_name.clone(),
        wrapper_exe,
        inner_exe: args.inner_exe.clone(),
        inner_args: args.inner_args.clone(),
    };

    let workspace = create_probe_workspace(repo_root, &run_nonce, &entry)
        .map_err(|err| format!("failed to create isolated probe workspace: {err}"))?;

    let affected_identity = identify_copilot(&args.copilot_exe, args.gate1_deadline);
    let stable_identity = args
        .stable_copilot_exe
        .as_deref()
        .map(|exe| identify_copilot(exe, args.gate1_deadline));
    let inner_identity = identify_inner_exe(&args.inner_exe);

    let gate1 = run_gate1(
        &args.copilot_exe,
        &workspace,
        &args.entry_name,
        args.gate1_deadline,
    );

    // Fail closed on an unproved identity BEFORE evaluating Gate 1's own
    // result: 056.001-T's acceptance criteria list "exact CLI identity"
    // and "same-inner-executable ... parity" as independent fail-closed
    // conditions alongside "ancestor config-isolation", so an identity
    // capture failure takes priority over -- and is never masked by --
    // whatever Gate 1 itself observed (Copilot review, 2026-09 -- 049-S
    // PR #120, round 2).
    if let Some(message) = identity_failure_message(
        &affected_identity,
        stable_identity.as_ref(),
        &inner_identity,
    ) {
        return Ok(ExactCliOutcome {
            run_nonce,
            sentinel_env_var: ENV_INHERITANCE_SENTINEL_VAR.to_string(),
            sentinel_value,
            manual_verification_invocation: MANUAL_VERIFICATION_INVOCATION.to_string(),
            workspace_root: workspace.root().to_path_buf(),
            affected_identity,
            stable_identity,
            inner_identity,
            gate1,
            passes: Vec::new(),
            ordered_cause_classification: vec![message],
            h3_b_candidate: false,
            terminal: "blocked",
        });
    }

    if !gate1.passed {
        return Ok(gate1_failure_outcome(
            run_nonce,
            sentinel_value,
            workspace.root().to_path_buf(),
            affected_identity,
            stable_identity,
            inner_identity,
            gate1,
        ));
    }

    let mut passes = vec![run_pass(
        "affected",
        &args.copilot_exe,
        &workspace,
        args,
        &wrapper_exe_path,
        &sentinel_value,
    )];
    if let Some(stable_exe) = &args.stable_copilot_exe {
        passes.push(run_pass(
            "stable",
            stable_exe,
            &workspace,
            args,
            &wrapper_exe_path,
            &sentinel_value,
        ));
    }

    let ordered_cause_classification = passes.iter().flat_map(classify_pass).collect::<Vec<_>>();

    Ok(ExactCliOutcome {
        run_nonce,
        sentinel_env_var: ENV_INHERITANCE_SENTINEL_VAR.to_string(),
        sentinel_value,
        manual_verification_invocation: MANUAL_VERIFICATION_INVOCATION.to_string(),
        workspace_root: workspace.root().to_path_buf(),
        affected_identity,
        stable_identity,
        inner_identity,
        gate1,
        passes,
        ordered_cause_classification,
        h3_b_candidate: false,
        terminal: "done",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnv1a_64_is_deterministic_and_distinguishes_inputs() {
        let a = fnv1a_64(b"hello world");
        let b = fnv1a_64(b"hello world");
        let c = fnv1a_64(b"hello world!");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn version_probe_failure_reason_is_none_for_a_clean_zero_exit() {
        assert!(version_probe_failure_reason(false, Some(0), false, None).is_none());
    }

    #[test]
    fn version_probe_failure_reason_fires_on_spawn_error() {
        let reason = version_probe_failure_reason(false, None, false, Some("no such file"));
        assert!(reason.unwrap().contains("failed to spawn"));
    }

    #[test]
    fn version_probe_failure_reason_fires_on_timeout() {
        let reason = version_probe_failure_reason(true, None, false, None);
        assert!(reason.unwrap().contains("timed out"));
    }

    #[test]
    fn version_probe_failure_reason_fires_on_missing_exit_status() {
        let reason = version_probe_failure_reason(false, None, false, None);
        assert!(reason.unwrap().contains("without an observable status"));
    }

    #[test]
    fn version_probe_failure_reason_fires_on_nonzero_exit() {
        let reason = version_probe_failure_reason(false, Some(1), false, None);
        assert!(reason.unwrap().contains("nonzero status 1"));
    }

    #[test]
    fn version_probe_failure_reason_fires_on_truncated_output_even_with_a_zero_exit() {
        let reason = version_probe_failure_reason(false, Some(0), true, None);
        assert!(reason.unwrap().contains("truncated"));
    }

    #[test]
    fn version_probe_failure_reason_prioritizes_spawn_error_over_every_other_signal() {
        let reason = version_probe_failure_reason(true, Some(1), true, Some("spawn boom"));
        assert!(reason.unwrap().contains("failed to spawn"));
    }

    #[test]
    fn parse_exact_cli_args_requires_copilot_exe() {
        let args = vec!["--repo-root".to_string(), "C:\\repo".to_string()];
        let result = parse_exact_cli_args(args.into_iter());
        assert!(result.is_err());
    }

    #[test]
    fn parse_exact_cli_args_requires_repo_root() {
        let args = vec!["--copilot-exe".to_string(), "C:\\copilot.exe".to_string()];
        let result = parse_exact_cli_args(args.into_iter());
        assert!(result.is_err());
    }

    #[test]
    fn parse_exact_cli_args_requires_inner_exe() {
        let args = vec![
            "--copilot-exe".to_string(),
            "C:\\copilot.exe".to_string(),
            "--repo-root".to_string(),
            "C:\\repo".to_string(),
        ];
        let result = parse_exact_cli_args(args.into_iter());
        assert!(result.is_err());
    }

    #[test]
    fn parse_exact_cli_args_rejects_unknown_flag() {
        let args = vec!["--bogus".to_string()];
        let result = parse_exact_cli_args(args.into_iter());
        assert_eq!(result.unwrap_err(), "exact-cli: unknown argument '--bogus'");
    }

    #[test]
    fn parse_exact_cli_args_parses_full_contract_with_defaults() {
        let args = vec![
            "--copilot-exe".to_string(),
            "C:\\copilot.exe".to_string(),
            "--repo-root".to_string(),
            "C:\\repo".to_string(),
            "--inner-exe".to_string(),
            "C:\\bun.exe".to_string(),
            "--inner-arg".to_string(),
            "shim.cjs".to_string(),
            "--inner-arg".to_string(),
            "serve".to_string(),
        ];
        let parsed = parse_exact_cli_args(args.into_iter()).expect("should parse");
        assert_eq!(parsed.copilot_exe, "C:\\copilot.exe");
        assert_eq!(parsed.repo_root, "C:\\repo");
        assert_eq!(parsed.inner_exe, "C:\\bun.exe");
        assert_eq!(
            parsed.inner_args,
            vec!["shim.cjs".to_string(), "serve".to_string()]
        );
        assert_eq!(parsed.entry_name, DEFAULT_ENTRY_NAME);
        assert_eq!(
            parsed.leg_deadline,
            Duration::from_secs(DEFAULT_LEG_DEADLINE_SECS)
        );
        assert_eq!(
            parsed.gate1_deadline,
            Duration::from_secs(DEFAULT_GATE1_DEADLINE_SECS)
        );
        assert!(parsed.stable_copilot_exe.is_none());
        assert!(parsed.run_nonce.is_none());
        assert!(parsed.sentinel_value.is_none());
    }

    #[test]
    fn parse_exact_cli_args_honors_overrides() {
        let args = vec![
            "--copilot-exe".to_string(),
            "C:\\copilot.exe".to_string(),
            "--stable-copilot-exe".to_string(),
            "C:\\copilot-stable.exe".to_string(),
            "--repo-root".to_string(),
            "C:\\repo".to_string(),
            "--inner-exe".to_string(),
            "C:\\bun.exe".to_string(),
            "--entry-name".to_string(),
            "custom-entry".to_string(),
            "--run-nonce".to_string(),
            "fixed-nonce".to_string(),
            "--leg-deadline-secs".to_string(),
            "9".to_string(),
            "--gate1-deadline-secs".to_string(),
            "3".to_string(),
            "--prompt".to_string(),
            "custom prompt".to_string(),
            "--sentinel-value".to_string(),
            "fixed-sentinel".to_string(),
        ];
        let parsed = parse_exact_cli_args(args.into_iter()).expect("should parse");
        assert_eq!(
            parsed.stable_copilot_exe.as_deref(),
            Some("C:\\copilot-stable.exe")
        );
        assert_eq!(parsed.entry_name, "custom-entry");
        assert_eq!(parsed.run_nonce.as_deref(), Some("fixed-nonce"));
        assert_eq!(parsed.leg_deadline, Duration::from_secs(9));
        assert_eq!(parsed.gate1_deadline, Duration::from_secs(3));
        assert_eq!(parsed.prompt, "custom prompt");
        assert_eq!(parsed.sentinel_value.as_deref(), Some("fixed-sentinel"));
    }

    #[test]
    fn parse_exact_cli_args_rejects_bad_deadline_integer() {
        let args = vec![
            "--copilot-exe".to_string(),
            "C:\\copilot.exe".to_string(),
            "--repo-root".to_string(),
            "C:\\repo".to_string(),
            "--inner-exe".to_string(),
            "C:\\bun.exe".to_string(),
            "--leg-deadline-secs".to_string(),
            "not-a-number".to_string(),
        ];
        let result = parse_exact_cli_args(args.into_iter());
        assert!(result.is_err());
    }

    #[test]
    fn extract_last_mcp_status_finds_last_matching_transition() {
        let stdout = "\
{\"type\":\"session.mcp_server_status_changed\",\"data\":{\"serverName\":\"graphtor-docs\",\"status\":\"pending\"}}
{\"type\":\"session.mcp_server_status_changed\",\"data\":{\"serverName\":\"other\",\"status\":\"connected\"}}
{\"type\":\"session.mcp_server_status_changed\",\"data\":{\"serverName\":\"graphtor-docs\",\"status\":\"connected\"}}
{\"type\":\"result\",\"exitCode\":0}
";
        let (status, exit_code) = extract_last_mcp_status(stdout, "graphtor-docs");
        assert_eq!(status.as_deref(), Some("connected"));
        assert_eq!(exit_code, Some(0));
    }

    #[test]
    fn extract_last_mcp_status_handles_no_matching_entry() {
        let stdout = "{\"type\":\"session.mcp_server_status_changed\",\"data\":{\"serverName\":\"other\",\"status\":\"connected\"}}\n";
        let (status, exit_code) = extract_last_mcp_status(stdout, "graphtor-docs");
        assert_eq!(status, None);
        assert_eq!(exit_code, None);
    }

    #[test]
    fn extract_last_mcp_status_ignores_unparseable_lines() {
        let stdout = "not json\n{\"type\":\"session.mcp_server_status_changed\",\"data\":{\"serverName\":\"graphtor-docs\",\"status\":\"connected\"}}\n";
        let (status, _) = extract_last_mcp_status(stdout, "graphtor-docs");
        assert_eq!(status.as_deref(), Some("connected"));
    }

    #[test]
    fn read_wrapper_evidence_reports_missing_file_as_none_without_error() {
        let (value, error) =
            read_wrapper_evidence(Path::new("C:\\this\\path\\does\\not\\exist\\evidence.json"));
        assert!(value.is_none());
        assert!(error.is_none());
    }

    #[test]
    fn read_wrapper_evidence_reports_invalid_json_as_error() {
        let dir =
            std::env::temp_dir().join(format!("mcp-probe-evidence-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("bad-evidence.json");
        std::fs::write(&path, b"not valid json").expect("write fixture");
        let (value, error) = read_wrapper_evidence(&path);
        assert!(value.is_none());
        assert!(error.is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn classify_pass_reports_h0a_when_only_treatment_connects() {
        let pass = PassOutcome {
            build: "affected".to_string(),
            copilot_identity: CopilotIdentity {
                exe_path: "copilot.exe".to_string(),
                version_output: None,
                content_hash_hex: String::new(),
                content_len: 0,
                identify_error: None,
            },
            control: LegOutcome {
                leg: Leg::Control,
                exit_code: None,
                timed_out: true,
                last_mcp_status: Some("pending".to_string()),
                session_result_exit_code: None,
                sentinel_observation: SentinelObservation::not_observed(
                    "test fixture: not observed",
                ),
                wrapper_evidence: None,
                wrapper_evidence_read_error: None,
                stdout_truncated: false,
            },
            treatment: LegOutcome {
                leg: Leg::Treatment,
                exit_code: Some(0),
                timed_out: false,
                last_mcp_status: Some("connected".to_string()),
                session_result_exit_code: Some(0),
                sentinel_observation: SentinelObservation {
                    observed: true,
                    observed_pid: Some(4321),
                    observed_exe: Some("mcp-probe.exe".to_string()),
                    note: "test fixture: observed".to_string(),
                },
                wrapper_evidence: Some(serde_json::json!({
                    "valid": true,
                    "initialize_correlation": {"protocol_version": "2024-11-05"},
                })),
                wrapper_evidence_read_error: None,
                stdout_truncated: false,
            },
        };
        let causes = classify_pass(&pass);
        assert_eq!(causes.len(), 1);
        assert!(causes[0].contains("H0a"));
    }

    #[test]
    fn classify_pass_reports_no_reproduction_when_both_legs_connect() {
        let ok_leg = |leg: Leg| LegOutcome {
            leg,
            exit_code: Some(0),
            timed_out: false,
            last_mcp_status: Some("connected".to_string()),
            session_result_exit_code: Some(0),
            sentinel_observation: SentinelObservation {
                observed: true,
                observed_pid: Some(1234),
                observed_exe: Some("mcp-probe.exe".to_string()),
                note: "test fixture: observed".to_string(),
            },
            wrapper_evidence: Some(serde_json::json!({
                "valid": true,
                "initialize_correlation": {"protocol_version": "2024-11-05"},
            })),
            wrapper_evidence_read_error: None,
            stdout_truncated: false,
        };
        let pass = PassOutcome {
            build: "affected".to_string(),
            copilot_identity: CopilotIdentity {
                exe_path: "copilot.exe".to_string(),
                version_output: None,
                content_hash_hex: String::new(),
                content_len: 0,
                identify_error: None,
            },
            control: ok_leg(Leg::Control),
            treatment: ok_leg(Leg::Treatment),
        };
        let causes = classify_pass(&pass);
        assert_eq!(causes.len(), 1);
        assert!(causes[0].contains("No H0a reproduction"));
    }

    #[test]
    fn classify_pass_reports_unresolved_when_both_legs_fail() {
        let failing_leg = |leg: Leg| LegOutcome {
            leg,
            exit_code: None,
            timed_out: true,
            last_mcp_status: None,
            session_result_exit_code: None,
            sentinel_observation: SentinelObservation::not_observed("test fixture: not observed"),
            wrapper_evidence: None,
            wrapper_evidence_read_error: None,
            stdout_truncated: false,
        };
        let pass = PassOutcome {
            build: "affected".to_string(),
            copilot_identity: CopilotIdentity {
                exe_path: "copilot.exe".to_string(),
                version_output: None,
                content_hash_hex: String::new(),
                content_len: 0,
                identify_error: None,
            },
            control: failing_leg(Leg::Control),
            treatment: failing_leg(Leg::Treatment),
        };
        let causes = classify_pass(&pass);
        assert_eq!(causes.len(), 1);
        assert!(causes[0].contains("Cause not resolved by cwd alone"));
    }

    #[test]
    fn gate1_outcome_fails_closed_on_nonzero_exit() {
        let outcome = Gate1Outcome {
            passed: false,
            exit_code: Some(1),
            timed_out: false,
            resolved_source_path: None,
            expected_source_path: "expected".to_string(),
            raw_stdout: String::new(),
            parse_error: None,
            spawn_error: None,
        };
        assert!(!outcome.passed);
    }

    /// A `Gate1Outcome` builder for [`gate1_proves_ancestor_merge`]'s
    /// unit tests below, defaulting to the one shape that DOES prove an
    /// ancestor merge (clean parse, zero exit, no spawn/parse/timeout
    /// failure, and a concrete, non-matching `resolved_source_path`) so
    /// each test only overrides the single field it means to exercise.
    fn base_gate1_that_proves_ancestor_merge() -> Gate1Outcome {
        Gate1Outcome {
            passed: false,
            exit_code: Some(0),
            timed_out: false,
            resolved_source_path: Some("C:\\repo\\ancestor\\.mcp.json".to_string()),
            expected_source_path: "C:\\repo\\child\\.mcp.json".to_string(),
            raw_stdout: "{\"sourcePath\": \"C:\\\\repo\\\\ancestor\\\\.mcp.json\"}".to_string(),
            parse_error: None,
            spawn_error: None,
        }
    }

    #[test]
    fn gate1_proves_ancestor_merge_is_true_only_for_a_clean_parse_with_a_resolved_source_path() {
        assert!(gate1_proves_ancestor_merge(
            &base_gate1_that_proves_ancestor_merge()
        ));
    }

    #[test]
    fn gate1_proves_ancestor_merge_is_false_when_gate1_actually_passed() {
        let mut gate1 = base_gate1_that_proves_ancestor_merge();
        gate1.passed = true;
        assert!(!gate1_proves_ancestor_merge(&gate1));
    }

    #[test]
    fn gate1_proves_ancestor_merge_is_false_on_spawn_error() {
        let mut gate1 = base_gate1_that_proves_ancestor_merge();
        gate1.spawn_error = Some("could not launch".to_string());
        assert!(!gate1_proves_ancestor_merge(&gate1));
    }

    #[test]
    fn gate1_proves_ancestor_merge_is_false_on_timeout() {
        let mut gate1 = base_gate1_that_proves_ancestor_merge();
        gate1.timed_out = true;
        assert!(!gate1_proves_ancestor_merge(&gate1));
    }

    #[test]
    fn gate1_proves_ancestor_merge_is_false_on_parse_error() {
        let mut gate1 = base_gate1_that_proves_ancestor_merge();
        gate1.parse_error = Some("unexpected token".to_string());
        assert!(!gate1_proves_ancestor_merge(&gate1));
    }

    #[test]
    fn gate1_proves_ancestor_merge_is_false_on_nonzero_exit() {
        let mut gate1 = base_gate1_that_proves_ancestor_merge();
        gate1.exit_code = Some(1);
        assert!(!gate1_proves_ancestor_merge(&gate1));
    }

    #[test]
    fn gate1_proves_ancestor_merge_is_false_when_no_source_path_was_resolved() {
        let mut gate1 = base_gate1_that_proves_ancestor_merge();
        gate1.resolved_source_path = None;
        assert!(!gate1_proves_ancestor_merge(&gate1));
    }

    fn identity_ok(exe_path: &str) -> CopilotIdentity {
        CopilotIdentity {
            exe_path: exe_path.to_string(),
            version_output: None,
            content_hash_hex: "deadbeef".to_string(),
            content_len: 4,
            identify_error: None,
        }
    }

    fn identity_failed(exe_path: &str) -> CopilotIdentity {
        CopilotIdentity {
            exe_path: exe_path.to_string(),
            version_output: None,
            content_hash_hex: String::new(),
            content_len: 0,
            identify_error: Some("no such file".to_string()),
        }
    }

    #[test]
    fn identity_failure_message_is_none_when_every_identity_is_readable() {
        assert!(identity_failure_message(
            &identity_ok("copilot.exe"),
            Some(&identity_ok("copilot-stable.exe")),
            &identity_ok("inner.exe"),
        )
        .is_none());
    }

    #[test]
    fn identity_failure_message_is_none_when_stable_identity_is_absent() {
        assert!(identity_failure_message(
            &identity_ok("copilot.exe"),
            None,
            &identity_ok("inner.exe")
        )
        .is_none());
    }

    #[test]
    fn identity_failure_message_fires_on_affected_identity_failure() {
        let message = identity_failure_message(
            &identity_failed("copilot.exe"),
            Some(&identity_ok("copilot-stable.exe")),
            &identity_ok("inner.exe"),
        )
        .expect("affected identity failure must fail closed");
        assert!(message.contains("exact target Copilot CLI's identity"));
    }

    #[test]
    fn identity_failure_message_fires_on_stable_identity_failure() {
        let message = identity_failure_message(
            &identity_ok("copilot.exe"),
            Some(&identity_failed("copilot-stable.exe")),
            &identity_ok("inner.exe"),
        )
        .expect("stable identity failure must fail closed");
        assert!(message.contains("last-known-stable Copilot CLI's identity"));
    }

    #[test]
    fn identity_failure_message_fires_on_inner_identity_failure() {
        let message = identity_failure_message(
            &identity_ok("copilot.exe"),
            Some(&identity_ok("copilot-stable.exe")),
            &identity_failed("inner.exe"),
        )
        .expect("inner identity failure must fail closed");
        assert!(message.contains("inner executable's identity"));
    }

    #[test]
    fn identity_failure_message_prioritizes_affected_over_inner() {
        // When both the affected and inner identities fail, the affected
        // identity's message takes priority (checked first) -- proving
        // the check order is deterministic rather than arbitrary.
        let message = identity_failure_message(
            &identity_failed("copilot.exe"),
            None,
            &identity_failed("inner.exe"),
        )
        .expect("either failure must fail closed");
        assert!(message.contains("exact target Copilot CLI's identity"));
    }

    #[test]
    fn outcome_to_json_round_trips_top_level_shape() {
        let outcome = ExactCliOutcome {
            run_nonce: "nonce-1".to_string(),
            sentinel_env_var: "MCP_PROBE_ENV_INHERITANCE_SENTINEL".to_string(),
            sentinel_value: "sentinel-1".to_string(),
            manual_verification_invocation: MANUAL_VERIFICATION_INVOCATION.to_string(),
            workspace_root: PathBuf::from("C:\\repo\\logs\\probe\\nonce-1"),
            affected_identity: CopilotIdentity {
                exe_path: "copilot.exe".to_string(),
                version_output: Some("1.0.0".to_string()),
                content_hash_hex: "deadbeef".to_string(),
                content_len: 42,
                identify_error: None,
            },
            stable_identity: None,
            inner_identity: CopilotIdentity {
                exe_path: "inner.exe".to_string(),
                version_output: None,
                content_hash_hex: "cafef00d".to_string(),
                content_len: 7,
                identify_error: None,
            },
            gate1: Gate1Outcome {
                passed: true,
                exit_code: Some(0),
                timed_out: false,
                resolved_source_path: Some("resolved".to_string()),
                expected_source_path: "resolved".to_string(),
                raw_stdout: String::new(),
                parse_error: None,
                spawn_error: None,
            },
            passes: Vec::new(),
            ordered_cause_classification: vec!["placeholder".to_string()],
            h3_b_candidate: false,
            terminal: "done",
        };
        let json = outcome_to_json(&outcome);
        assert_eq!(json["run_nonce"], "nonce-1");
        assert_eq!(json["gate1"]["passed"], true);
        assert_eq!(json["terminal"], "done");
        assert_eq!(json["ordered_cause_classification"][0], "placeholder");
        assert_eq!(json["inner_identity"]["exe_path"], "inner.exe");
        assert_eq!(json["inner_identity"]["content_hash_hex"], "cafef00d");
    }

    #[test]
    fn persist_outcome_json_fails_closed_when_the_workspace_root_does_not_exist() {
        // Copilot review thread C (PR #120, round 2): `main.rs`'s
        // caller must be able to detect and fail closed on a
        // persistence failure. This proves the underlying signal it
        // relies on is real: writing under a workspace root that was
        // never created (no `create_probe_workspace` call preceded it)
        // must return `Err`, not silently succeed or panic.
        let mut missing_root = std::env::temp_dir();
        missing_root.push(format!(
            "mcp-probe-persist-json-missing-root-{}-{}",
            std::process::id(),
            fnv1a_64(b"persist_outcome_json_fails_closed_when_the_workspace_root_does_not_exist")
        ));
        // Deliberately never created -- `persist_outcome_json` must not
        // create it either; only `create_probe_workspace` does that.
        assert!(!missing_root.exists());

        let outcome = ExactCliOutcome {
            run_nonce: "nonce-missing-root".to_string(),
            sentinel_env_var: "MCP_PROBE_ENV_INHERITANCE_SENTINEL".to_string(),
            sentinel_value: "sentinel-missing-root".to_string(),
            manual_verification_invocation: MANUAL_VERIFICATION_INVOCATION.to_string(),
            workspace_root: missing_root,
            affected_identity: CopilotIdentity {
                exe_path: "copilot.exe".to_string(),
                version_output: None,
                content_hash_hex: "deadbeef".to_string(),
                content_len: 4,
                identify_error: None,
            },
            stable_identity: None,
            inner_identity: CopilotIdentity {
                exe_path: "inner.exe".to_string(),
                version_output: None,
                content_hash_hex: "cafef00d".to_string(),
                content_len: 4,
                identify_error: None,
            },
            gate1: Gate1Outcome {
                passed: false,
                exit_code: None,
                timed_out: false,
                resolved_source_path: None,
                expected_source_path: "expected".to_string(),
                raw_stdout: String::new(),
                parse_error: None,
                spawn_error: Some("could not launch".to_string()),
            },
            passes: Vec::new(),
            ordered_cause_classification: vec!["placeholder".to_string()],
            h3_b_candidate: false,
            terminal: "blocked",
        };

        let result = persist_outcome_json(&outcome);
        assert!(
            result.is_err(),
            "persisting under a workspace root that was never created must fail closed, not \
             silently succeed"
        );
    }

    // ── Adversarial-review remediation regression tests ──────────────

    #[test]
    fn capture_buf_retains_bytes_under_the_cap_and_reports_no_truncation() {
        let mut buf = CaptureBuf::default();
        buf.push(b"hello");
        buf.push(b" world");
        assert_eq!(buf.bytes, b"hello world");
        assert!(!buf.truncated);
    }

    #[test]
    fn capture_buf_truncates_at_the_cap_and_discards_everything_after() {
        let mut buf = CaptureBuf::default();
        // Fill to exactly the cap, then push more: the excess must be
        // dropped and `truncated` must latch `true` permanently (M-1).
        buf.push(&vec![b'a'; MAX_CAPTURE_BYTES - 4]);
        assert!(!buf.truncated);
        buf.push(b"BCDE"); // fills the last 4 bytes exactly to the cap
        assert!(!buf.truncated, "reaching the cap exactly is not truncation");
        assert_eq!(buf.bytes.len(), MAX_CAPTURE_BYTES);

        buf.push(b"overflow-should-be-dropped");
        assert!(buf.truncated);
        assert_eq!(
            buf.bytes.len(),
            MAX_CAPTURE_BYTES,
            "buffer must never grow past the cap"
        );

        // Once truncated, further pushes must be pure no-ops (M-1: a
        // misbehaving child cannot grow memory without limit).
        buf.push(&vec![b'z'; 1024]);
        assert!(buf.truncated);
        assert_eq!(buf.bytes.len(), MAX_CAPTURE_BYTES);
    }

    #[test]
    fn capture_sink_write_always_reports_the_full_len_even_once_truncated() {
        // `Write::write` must report the number of bytes the caller asked
        // to write, never the number actually retained -- otherwise a
        // pump loop that checks the returned count against the input
        // length would treat a capped-but-successful write as a
        // short-write error (see CaptureBuf's own doc comment).
        let mut sink = CaptureSink::new();
        let filler = vec![0u8; MAX_CAPTURE_BYTES];
        let n0 = sink.write(&filler).expect("write never errors");
        assert_eq!(n0, filler.len());
        let n = sink
            .write(b"more-bytes-after-the-cap")
            .expect("write never errors");
        assert_eq!(n, "more-bytes-after-the-cap".len());
        let (bytes, truncated) = sink.into_parts();
        assert!(truncated);
        assert_eq!(bytes.len(), MAX_CAPTURE_BYTES);
    }

    #[test]
    fn leg_has_valid_initialize_requires_the_valid_flag_true() {
        // U-2: a populated `initialize_correlation` alone must never be
        // enough -- the collector's own `valid: false` signal (e.g. a
        // saturated evidence channel) must veto it.
        let leg = LegOutcome {
            leg: Leg::Control,
            exit_code: Some(0),
            timed_out: false,
            last_mcp_status: Some("connected".to_string()),
            session_result_exit_code: Some(0),
            sentinel_observation: SentinelObservation::not_observed("test fixture"),
            wrapper_evidence: Some(serde_json::json!({
                "valid": false,
                "invalid_reason": "evidence channel saturated: one or more copies were dropped",
                "initialize_correlation": {"protocol_version": "2024-11-05"},
            })),
            wrapper_evidence_read_error: None,
            stdout_truncated: false,
        };
        assert!(
            !leg_has_valid_initialize(&leg),
            "valid: false must veto an otherwise-populated initialize_correlation"
        );
    }

    #[test]
    fn leg_has_valid_initialize_requires_a_non_null_correlation() {
        let leg = LegOutcome {
            leg: Leg::Control,
            exit_code: Some(0),
            timed_out: false,
            last_mcp_status: Some("connected".to_string()),
            session_result_exit_code: Some(0),
            sentinel_observation: SentinelObservation::not_observed("test fixture"),
            wrapper_evidence: Some(serde_json::json!({
                "valid": true,
                "initialize_correlation": null,
            })),
            wrapper_evidence_read_error: None,
            stdout_truncated: false,
        };
        assert!(!leg_has_valid_initialize(&leg));
    }

    #[test]
    fn leg_has_valid_initialize_true_when_both_conditions_hold() {
        let leg = LegOutcome {
            leg: Leg::Control,
            exit_code: Some(0),
            timed_out: false,
            last_mcp_status: Some("connected".to_string()),
            session_result_exit_code: Some(0),
            sentinel_observation: SentinelObservation::not_observed("test fixture"),
            wrapper_evidence: Some(serde_json::json!({
                "valid": true,
                "initialize_correlation": {"protocol_version": "2024-11-05"},
            })),
            wrapper_evidence_read_error: None,
            stdout_truncated: false,
        };
        assert!(leg_has_valid_initialize(&leg));
    }
}
