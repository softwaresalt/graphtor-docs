//! Process spawning, teardown, and the injectable process-observation
//! seam for the standalone `mcp-probe` diagnostic crate, composed onto
//! the `056.020-T` transport (see [`crate::transport`]).
//!
//! This module owns the versioned `wrapper` subcommand's core logic
//! (`run_wrapper`) and the direct-`Child`-handle-only kill/wait
//! authority ([`ChildGuard`]). It never reimplements the byte pumps --
//! the wrapper wires the inner child's stdio through
//! [`crate::transport::run_duplex_pump`] unchanged.
//!
//! Kill/wait authority is exclusively the owned direct
//! `std::process::Child` handle inside a [`ChildGuard`]. The injectable
//! [`ProcessObserver`] seam exists purely for diagnostics and
//! deterministic tests: it can verify and report a process's identity
//! (pid / start time / executable / parent) but has no way to kill or
//! otherwise mutate what it observes. A same-second (or otherwise
//! indistinguishable) start-time match is always reported ambiguous,
//! never a confirmed identity -- see [`is_ambiguous_match`].

use crate::evidence::{write_evidence_output, EvidenceCollector};
use crate::transport::{run_duplex_pump, PumpConfig, PumpOutcome};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::Duration;

/// Name of the sentinel environment variable used to prove downstream
/// environment-variable inheritance for `056.006-T`'s selection
/// criterion. `056.001-T` sets this (to a unique per-run value) on the
/// exact-CLI-spawned process tree; this module observes whether ITS OWN
/// process (running as the `wrapper` subcommand, a descendant of that
/// tree when composed under a real CLI run) inherited it. Observation
/// only: never used to alter the wire or gate any behavior in this
/// crate.
pub const ENV_INHERITANCE_SENTINEL_VAR: &str = "MCP_PROBE_ENV_INHERITANCE_SENTINEL";

/// RAII guard around a directly-owned [`std::process::Child`]: kills and
/// (bounded) waits on every outcome -- normal drop, error return, or
/// panic/unwind (stack unwinding) -- so no owned child process ever
/// leaks. This is the SOLE kill/wait authority for any process this
/// crate spawns. The wrapper owns the inner-server child's guard; the
/// `056.001-T` exact-CLI runner owns the Copilot child's guard the same
/// way. Nothing else in this crate (including [`ProcessObserver`]) is
/// ever permitted to kill a process.
pub struct ChildGuard {
    child: Option<Child>,
    label: &'static str,
}

impl ChildGuard {
    /// Takes ownership of `child`, labeling it for diagnostics (for
    /// example `"inner"` for the wrapper's inner server child).
    #[must_use]
    pub fn new(child: Child, label: &'static str) -> Self {
        Self {
            child: Some(child),
            label,
        }
    }

    /// The diagnostic label this guard was created with.
    #[must_use]
    pub fn label(&self) -> &'static str {
        self.label
    }

    /// The owned child's process id, if the guard has not already been
    /// torn down. Remains queryable after `kill_and_wait`/`wait` --
    /// `Child::id` reports the pid recorded at spawn time and stays
    /// valid regardless of whether the process has since exited.
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    /// Mutable access to the owned child, for wiring its stdio into a
    /// pump.
    ///
    /// # Panics
    ///
    /// Panics if called after this guard has already been torn down.
    /// This never happens in ordinary use: teardown only ever occurs in
    /// `kill_and_wait` (an explicit, single, caller-driven call) or
    /// `Drop` (at end of scope), neither of which is followed by further
    /// use of the guard.
    pub fn child_mut(&mut self) -> &mut Child {
        self.child.as_mut().expect("child guard already torn down")
    }

    /// Waits for the owned child to exit on its own (normal completion),
    /// returning its exit status. This does not disable this guard's own
    /// `Drop` teardown -- once the child has exited, the subsequent
    /// `kill()`/`wait()` inside `Drop` become no-ops (the OS reports the
    /// process as already reaped and both calls are ignored).
    ///
    /// # Errors
    ///
    /// Returns an error only if the underlying OS wait call itself fails
    /// (not on a non-zero exit code, which is reported via the returned
    /// [`ExitStatus`]).
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child_mut().wait()
    }

    /// Explicitly kills and (bounded) waits on the owned child right
    /// now, ahead of `Drop`. Used by a caller (for example a deadline
    /// path) that needs teardown to have completed before proceeding,
    /// rather than only guaranteed-eventually via `Drop`.
    pub fn kill_and_wait(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Diagnostic identity of an observed process: pid, best-available
/// process-start time, executable path, and parent pid. Observation
/// only -- nothing in this module ever kills a process except through
/// the owned direct [`ChildGuard`] above.
// The `Process` prefix is deliberate and clearer than a bare `Identity`
// for a type re-exported from the crate root's public API surface.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessIdentity {
    pub pid: u32,
    /// Best-available process start time, in whole seconds since the
    /// Unix epoch. Second-granularity is a deliberate, documented
    /// limitation, not an oversight: it is exactly why a same-second
    /// start-time match is always reported ambiguous rather than
    /// confirmed (see [`is_ambiguous_match`]).
    pub start_time_unix_secs: u64,
    pub executable: Option<String>,
    pub parent_pid: Option<u32>,
}

/// Injectable process-observation seam. REQUIRED so tests can supply
/// deterministic identities without depending on real OS process
/// timing/pid-reuse behavior. The production implementation
/// ([`SysinfoProcessObserver`]) is backed by the standalone `sysinfo`
/// dependency.
///
/// Implementors MUST NOT expose any way to kill or otherwise mutate an
/// observed process through this trait -- verification and reporting
/// only. All kill/wait authority routes exclusively through the owned
/// direct [`ChildGuard`] above.
// The `Process` prefix is deliberate and clearer than a bare `Observer`
// for a trait re-exported from the crate root's public API surface.
#[allow(clippy::module_name_repetitions)]
pub trait ProcessObserver {
    /// Observes the process currently at `pid`, if one is currently
    /// observable.
    fn observe(&self, pid: u32) -> Option<ProcessIdentity>;

    /// Lists the pids of any currently-observable child processes of
    /// `parent_pid`. Used only to surface residual/unknown descendants
    /// for operator-approved action -- never to act on them.
    fn child_pids(&self, parent_pid: u32) -> Vec<u32>;
}

/// A same-second (or otherwise indistinguishable) start-time match
/// between an expected identity and a freshly observed one at the same
/// pid. This is NEVER promoted to a positive confirmed identity match:
/// through this observation seam alone, a coincidental reused-pid
/// process that happens to start in the same second is indistinguishable
/// from the original tracked process. Callers must treat a `true` result
/// as ambiguous (fail closed), never as proof of identity.
#[must_use]
pub fn is_ambiguous_match(expected: &ProcessIdentity, observed: &ProcessIdentity) -> bool {
    expected.pid == observed.pid && expected.start_time_unix_secs == observed.start_time_unix_secs
}

/// Production [`ProcessObserver`] backed by the standalone `sysinfo`
/// dependency. Verifies and reports only; never kills.
pub struct SysinfoProcessObserver {
    system: std::cell::RefCell<sysinfo::System>,
}

impl SysinfoProcessObserver {
    #[must_use]
    pub fn new() -> Self {
        let mut system = sysinfo::System::new();
        system.refresh_processes();
        Self {
            system: std::cell::RefCell::new(system),
        }
    }
}

impl Default for SysinfoProcessObserver {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcessObserver for SysinfoProcessObserver {
    fn observe(&self, pid: u32) -> Option<ProcessIdentity> {
        let mut system = self.system.borrow_mut();
        system.refresh_processes();
        let sys_pid = sysinfo::Pid::from_u32(pid);
        let process = system.process(sys_pid)?;
        Some(ProcessIdentity {
            pid,
            start_time_unix_secs: process.start_time(),
            executable: process.exe().and_then(|p| p.to_str()).map(str::to_owned),
            parent_pid: process.parent().map(sysinfo::Pid::as_u32),
        })
    }

    fn child_pids(&self, parent_pid: u32) -> Vec<u32> {
        let mut system = self.system.borrow_mut();
        system.refresh_processes();
        let parent = sysinfo::Pid::from_u32(parent_pid);
        system
            .processes()
            .iter()
            .filter(|(_, process)| process.parent() == Some(parent))
            .map(|(pid, _)| pid.as_u32())
            .collect()
    }
}

/// Argv-level configuration for the `wrapper` subcommand, exactly as
/// parsed from its byte-identical-between-legs argv contract:
/// `--inner-exe`, repeated `--inner-arg`, `--evidence-output`, and
/// `--run-nonce`.
#[derive(Debug, Clone)]
pub struct WrapperArgs {
    pub inner_exe: String,
    pub inner_args: Vec<String>,
    pub evidence_output: String,
    pub run_nonce: String,
}

/// Parses the `wrapper` subcommand's argv contract. Unknown arguments or
/// a missing required value are reported as an error string suitable for
/// direct `eprintln!` display; this task performs no other argv
/// validation (for example path existence) -- that is out of this
/// task's scope.
///
/// # Errors
///
/// Returns an error string when an unknown flag is encountered, a flag
/// requiring a value has none following it, or a required flag
/// (`--inner-exe`, `--evidence-output`, `--run-nonce`) is absent.
pub fn parse_wrapper_args(args: impl Iterator<Item = String>) -> Result<WrapperArgs, String> {
    let mut inner_exe = None;
    let mut inner_args = Vec::new();
    let mut evidence_output = None;
    let mut run_nonce = None;

    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--inner-exe" => {
                inner_exe = Some(args.next().ok_or("--inner-exe requires a value")?);
            }
            "--inner-arg" => {
                inner_args.push(args.next().ok_or("--inner-arg requires a value")?);
            }
            "--evidence-output" => {
                evidence_output = Some(args.next().ok_or("--evidence-output requires a value")?);
            }
            "--run-nonce" => {
                run_nonce = Some(args.next().ok_or("--run-nonce requires a value")?);
            }
            other => return Err(format!("wrapper: unknown argument '{other}'")),
        }
    }

    Ok(WrapperArgs {
        inner_exe: inner_exe.ok_or("--inner-exe is required")?,
        inner_args,
        evidence_output: evidence_output.ok_or("--evidence-output is required")?,
        run_nonce: run_nonce.ok_or("--run-nonce is required")?,
    })
}

/// Full configuration for [`run_wrapper`]. `pump_deadline` is a
/// test-only knob never populated from the parsed CLI argv contract
/// (which has no `--deadline` flag): production wrapper runs always
/// default it to `None` (unbounded pump wait -- the outer bound belongs
/// to `056.001-T`'s exact-CLI runner). Only this crate's own integration
/// self-tests construct a `WrapperConfig` directly with a bounded
/// deadline, to deterministically exercise deadline/error teardown
/// without waiting on a real wedged process.
#[derive(Debug, Clone)]
pub struct WrapperConfig {
    pub args: WrapperArgs,
    pub pump_deadline: Option<Duration>,
}

/// Outcome of one `wrapper` run: the inner child's preserved exit code
/// (`None` only when the pump deadline elapsed and the inner child had
/// to be torn down instead of exiting on its own), the underlying pump
/// outcome, this wrapper process's own observed identity, whether the
/// env-inheritance sentinel was observed, any residual descendant
/// processes surfaced (never acted on), and the `056.023-T`
/// evidence-collection outcome: whether the collected summary was
/// itself valid (no saturation/observer failure), and, separately,
/// whether writing that summary to `evidence_output` succeeded. Evidence
/// collection and its write are always best-effort -- neither ever
/// changes `inner_exit_code` or fails this function.
#[derive(Debug, Clone)]
pub struct WrapperOutcome {
    pub inner_exit_code: Option<i32>,
    pub pump: PumpOutcome,
    pub wrapper_identity: Option<ProcessIdentity>,
    pub sentinel_inherited: Option<String>,
    pub residual_descendants: Vec<ProcessIdentity>,
    pub run_nonce: String,
    pub evidence_output: String,
    /// `false` when the `056.023-T` evidence summary itself was marked
    /// invalid (channel saturation or a caught correlator panic) -- see
    /// `crate::evidence::EvidenceSummary::valid`.
    pub evidence_valid: bool,
    /// `Some(message)` only if writing the evidence summary to
    /// `evidence_output` failed; `None` on a successful write. This is
    /// independent of `evidence_valid` -- a valid summary can still fail
    /// to persist (for example, an unwritable path).
    pub evidence_write_error: Option<String>,
}

/// Runs the `wrapper` subcommand's core logic: spawns the inner process
/// named by `config.args.inner_exe`/`inner_args`, wires its stdio through
/// [`run_duplex_pump`] (never reimplementing the byte pumps), preserves
/// its exit code, reports diagnostic identity/sentinel/residual
/// information through `observer`, and -- composing `056.023-T`'s
/// observer seam onto the same pump call -- collects a redacted evidence
/// summary and writes it atomically to `config.args.evidence_output`.
///
/// `incoming`/`outgoing` stand in for the actual Copilot CLI's
/// stdin/stdout from this process's perspective; production callers pass
/// `io::stdin()`/`io::stdout()`, and tests pass in-memory fixtures the
/// same way `056.020-T`'s transport self-tests do.
///
/// # Errors
///
/// Returns an error only if the inner process could not be spawned, or
/// if [`run_duplex_pump`] itself errors (missing piped stdio). Evidence
/// collection/write failures are reported via the returned
/// [`WrapperOutcome`], never as an `Err` from this function.
pub fn run_wrapper<R, W>(
    incoming: R,
    outgoing: W,
    config: &WrapperConfig,
    observer: &dyn ProcessObserver,
) -> io::Result<WrapperOutcome>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    let child = Command::new(&config.args.inner_exe)
        .args(&config.args.inner_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut guard = ChildGuard::new(child, "inner");

    let pump_config = PumpConfig {
        deadline: config.pump_deadline,
        ..PumpConfig::default()
    };

    let evidence_collector = EvidenceCollector::new(config.args.run_nonce.clone());
    let pump = run_duplex_pump(
        incoming,
        outgoing,
        guard.child_mut(),
        &pump_config,
        Some(evidence_collector.hook()),
    )?;

    let inner_exit_code = if pump.timed_out {
        // Deadline/error teardown: the inner child never closed both
        // directions on its own within the bound. Tear it down right now
        // via the owned direct guard rather than only-eventually via
        // `Drop`, then report no exit code -- the child was killed, not
        // waited-to-completion.
        guard.kill_and_wait();
        None
    } else {
        guard.wait().ok().and_then(|status| status.code())
    };

    let wrapper_identity = observer.observe(std::process::id());
    let sentinel_inherited = std::env::var(ENV_INHERITANCE_SENTINEL_VAR).ok();

    // Residual/unknown descendants: after the owned guard's teardown
    // above, ask the observer whether any child processes of the
    // (already torn-down) inner child remain observable. Diagnostic
    // only: surfaced, never acted on -- this task never kills them.
    let residual_descendants = guard.pid().map_or_else(Vec::new, |inner_pid| {
        observer
            .child_pids(inner_pid)
            .into_iter()
            .filter_map(|pid| observer.observe(pid))
            .collect()
    });

    // 056.023-T: finalize evidence collection and persist the redacted
    // summary to the wrapper-owned evidence_output path. This is always
    // best-effort -- neither collection validity nor a write failure
    // ever changes `inner_exit_code` or turns into an `Err` here.
    //
    // A transport-level delivery drop (`pump.transport_copies_dropped`)
    // happens at transport's OWN outer delivery channel, strictly before
    // `evidence_collector`'s hook (and therefore the collector itself)
    // ever observes the copy -- the collector has no way to detect this
    // loss on its own, so it must be told about it explicitly, before
    // `finalize()`, or a summary with silently missing data could be
    // finalized as `valid: true`.
    evidence_collector.note_transport_drops(pump.transport_copies_dropped);
    let evidence_summary = evidence_collector.finalize();
    let evidence_valid = evidence_summary.valid;
    let evidence_write_error =
        write_evidence_output(&evidence_summary, Path::new(&config.args.evidence_output))
            .err()
            .map(|err| err.to_string());

    Ok(WrapperOutcome {
        inner_exit_code,
        pump,
        wrapper_identity,
        sentinel_inherited,
        residual_descendants,
        run_nonce: config.args.run_nonce.clone(),
        evidence_output: config.args.evidence_output.clone(),
        evidence_valid,
        evidence_write_error,
    })
}
