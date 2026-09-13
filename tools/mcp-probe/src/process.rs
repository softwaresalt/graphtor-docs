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
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Name of the sentinel environment variable used to prove downstream
/// environment-variable inheritance for `056.006-T`'s selection
/// criterion. `056.001-T` sets this (to a unique per-run value) on the
/// exact-CLI-spawned process tree; this module observes whether ITS OWN
/// process (running as the `wrapper` subcommand, a descendant of that
/// tree when composed under a real CLI run) inherited it. Observation
/// only: never used to alter the wire or gate any behavior in this
/// crate.
pub const ENV_INHERITANCE_SENTINEL_VAR: &str = "MCP_PROBE_ENV_INHERITANCE_SENTINEL";

/// Bounded budget for the poll-based wait after `kill()` inside
/// [`ChildGuard::kill_and_wait`] and its `Drop` teardown, mirroring
/// `crate::transport`'s `DELIVERY_DRAIN_BUDGET` bounded-wait pattern. A
/// killed child is normally reaped almost immediately once the OS
/// delivers the termination signal, but an unconditional blocking
/// `Child::wait()` has no such guarantee against every possible
/// wedged-process scenario -- bounding the wait keeps teardown itself
/// (and therefore the caller, or `Drop` at scope exit) from blocking
/// forever (Copilot review, 2026-09 -- 049-S PR #120, round 3).
///
/// `pub(crate)` so `crate::exact_cli`'s own pump-then-reap-or-kill shape
/// (`pump_and_reap`) can reuse the exact same budget for its normal-path
/// bounded wait instead of duplicating a second magic-number constant
/// (Copilot review, 2026-09 -- 049-S PR #120, round 5).
pub(crate) const KILL_WAIT_BUDGET: Duration = Duration::from_millis(500);

/// Poll interval used while bounding the wait after `kill()`.
const KILL_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Polls [`Child::try_wait`] on `child` until it reports the child has
/// exited, or `budget` elapses, whichever comes first. Never blocks past
/// `budget`. Returns `true` if the child was observed to have exited
/// within budget; `false` if the budget elapsed while the child was
/// still running, or if `try_wait` itself returned an OS-level error
/// (treated as "could not confirm exit", never propagated -- teardown
/// callers already discard the underlying `kill`/`wait` errors).
fn bounded_wait_after_kill(child: &mut Child, budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => return true,
            Ok(None) => {
                if Instant::now() >= deadline {
                    return false;
                }
                thread::sleep(KILL_WAIT_POLL_INTERVAL);
            }
            Err(_) => return false,
        }
    }
}

/// Polls [`Child::try_wait`] on `child` until it reports the child has
/// exited (returning its [`ExitStatus`]), or `budget` elapses, whichever
/// comes first. Never blocks past `budget`. Returns `None` if the budget
/// elapses while the child is still running, or if `try_wait` itself
/// returns an OS-level error (treated as "could not confirm exit",
/// mirroring [`bounded_wait_after_kill`]'s own error handling).
fn bounded_wait_for_exit(child: &mut Child, budget: Duration) -> Option<ExitStatus> {
    let deadline = Instant::now() + budget;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    return None;
                }
                thread::sleep(KILL_WAIT_POLL_INTERVAL);
            }
            Err(_) => return None,
        }
    }
}

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

    /// Waits for the owned child to exit within `budget`, falling back
    /// to [`Self::kill_and_wait`] if it has not exited by then. Returns
    /// `(Some(status), false)` if the child was confirmed to have
    /// exited on its own within budget; `(None, teardown_incomplete)`
    /// otherwise, where `teardown_incomplete` is `kill_and_wait`'s own
    /// negated confirmed-reap result.
    ///
    /// This exists because a pump/reader loop ending "normally" (for
    /// example both stdio directions closing) does not by itself prove
    /// the child has exited: closing pipes and exiting are two distinct
    /// events, and a child that closes its pipes but never actually
    /// terminates would otherwise hang an unconditional, unbounded
    /// `wait()` call immediately afterward -- and, transitively, its
    /// caller -- forever (Copilot review, 2026-09 -- 049-S PR #120,
    /// round 5).
    #[must_use]
    pub fn bounded_wait_then_kill(&mut self, budget: Duration) -> (Option<ExitStatus>, bool) {
        let exited = self
            .child
            .as_mut()
            .and_then(|child| bounded_wait_for_exit(child, budget));
        if let Some(status) = exited {
            (Some(status), false)
        } else {
            let confirmed = self.kill_and_wait();
            (None, !confirmed)
        }
    }

    /// Explicitly kills and (bounded) waits on the owned child right
    /// now, ahead of `Drop`. Used by a caller (for example a deadline
    /// path) that needs teardown to have completed before proceeding,
    /// rather than only guaranteed-eventually via `Drop`. The wait is
    /// bounded by [`KILL_WAIT_BUDGET`] via [`bounded_wait_after_kill`]:
    /// an unconditional blocking `wait()` here could hang this call (and
    /// its caller) forever if the killed child never becomes reapable
    /// (Copilot review, 2026-09 -- 049-S PR #120, round 3).
    ///
    /// Returns `true` only if the child was subsequently confirmed
    /// reaped (exited) within [`KILL_WAIT_BUDGET`], regardless of
    /// whether `kill()` itself reported success; `false` if the bounded
    /// wait elapsed without observing the child exit. A `kill()` error
    /// is expected and harmless when the child had already exited on its
    /// own in the narrow window just before this call -- treating that
    /// case as an incomplete teardown (by additionally requiring
    /// `kill()` to have succeeded) previously made this diagnostic
    /// inaccurate even though the bounded wait had already confirmed the
    /// child was gone (Copilot review, 2026-09 -- 049-S PR #120,
    /// round 5). Discarding this result entirely previously let a caller
    /// report teardown as complete even though its owned child process
    /// could still be alive after the budget elapsed -- the caller MUST
    /// record an explicit incomplete-teardown outcome when this returns
    /// `false` rather than silently treating teardown as clean (Copilot
    /// review, 2026-09 -- 049-S PR #120, round 4).
    #[must_use]
    pub fn kill_and_wait(&mut self) -> bool {
        self.child.as_mut().is_some_and(|child| {
            let _ = child.kill();
            bounded_wait_after_kill(child, KILL_WAIT_BUDGET)
        })
    }
}

impl Drop for ChildGuard {
    /// Best-effort fallback teardown for a guard that was never
    /// explicitly torn down via `kill_and_wait`. Its result is
    /// intentionally still discarded here (unlike `kill_and_wait`
    /// itself): there is no caller left to report an incomplete
    /// teardown to once a value's `Drop` is running, and the whole
    /// point of routing every owned child through this guard is that
    /// leaking a still-running process is already impossible by
    /// construction -- `Drop` always at least attempts `kill()`, it
    /// just cannot surface whether that attempt was confirmed.
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = bounded_wait_after_kill(&mut child, KILL_WAIT_BUDGET);
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
/// requiring a value has none following it, a required flag
/// (`--inner-exe`, `--evidence-output`, `--run-nonce`) is absent, or
/// `--evidence-output` fails [`validate_evidence_output_path`] (not an
/// absolute path, or contains a `..` path-traversal component).
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

    let evidence_output = evidence_output.ok_or("--evidence-output is required")?;
    validate_evidence_output_path(&evidence_output)?;

    Ok(WrapperArgs {
        inner_exe: inner_exe.ok_or("--inner-exe is required")?,
        inner_args,
        evidence_output,
        run_nonce: run_nonce.ok_or("--run-nonce is required")?,
    })
}

/// Rejects an `--evidence-output` value that is not a "safe" path shape:
/// relative (deferring resolution to an ambient, caller-controlled
/// current working directory rather than the path this crate itself
/// resolved), or containing a literal `..` parent-dir component (a
/// path-traversal escape attempt). Every legitimately generated
/// evidence-output path is always built by
/// `workspace::create_probe_workspace` as an already-canonicalized,
/// absolute, `..`-free path under the caller's own isolated probe
/// workspace; this check exists purely to fail closed on anything else
/// BEFORE this module ever opens/creates/renames a file at that path.
/// Previously the parsed value was accepted verbatim with no shape
/// validation at all, so a crafted or tampered `--evidence-output`
/// argument could direct a write anywhere on the filesystem the running
/// process had permission to reach (Copilot review, 2026-09 -- 049-S PR
/// #120, round 3).
///
/// This is deliberately a shape check, not a full
/// `workspace::validate_containment`-style canonicalize-and-compare: the
/// target file need not exist yet (this function runs before
/// `write_evidence_output` ever creates it), and the wrapper subcommand
/// has no independent notion of "the expected workspace root" to compare
/// against -- it only ever receives this single path via argv. Rejecting
/// non-absolute and traversal-bearing shapes closes the concrete escape
/// vectors without requiring that additional context.
///
/// Absolute-and-`..`-free is still not enough on its own: an absolute,
/// traversal-free path can still resolve somewhere entirely different
/// from what it appears to name if a reparse point/junction/symlink sits
/// at ANY already-existing ancestor directory component (the exact same
/// class of escape `workspace::validate_containment` and
/// `create_shared_dir_component_validated` already close for workspace
/// creation itself, see `workspace.rs`). This function additionally
/// walks every already-existing ancestor of the candidate path and
/// rejects if any of them is a reparse point -- still without requiring
/// an "expected root" parameter, since the check is self-contained
/// ("no existing ancestor may be a reparse point") rather than a
/// containment comparison against one (Copilot review, 2026-09 -- 049-S
/// PR #120, round 4).
///
/// # Errors
///
/// Returns an error string (matching this module's other argv validation
/// error style) if `path` is not absolute, if any component is a literal
/// `..` parent-dir segment, if any already-existing ancestor directory is
/// a reparse point/junction/symlink, or if an ancestor's metadata could
/// not be read for a reason other than the ancestor simply not existing
/// yet.
fn validate_evidence_output_path(path: &str) -> Result<(), String> {
    let candidate = Path::new(path);
    if !candidate.is_absolute() {
        return Err(format!(
            "--evidence-output must be an absolute path, got '{path}'"
        ));
    }
    if candidate
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(format!(
            "--evidence-output must not contain a '..' path-traversal component, got '{path}'"
        ));
    }
    reject_reparse_point_ancestor(candidate)
}

/// `true` only if OS metadata reports `path` itself (never followed
/// through) as a reparse point/junction (Windows) or symlink
/// (everywhere else) -- the exact same check `workspace.rs`'s own
/// private `is_reparse_point` performs, kept as an independent,
/// self-contained copy here rather than exported cross-module: neither
/// module claims a shared, reusable production security primitive (see
/// `workspace.rs`'s own module-level doc comment).
#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

/// Walks every ancestor directory of `candidate` (its parent, that
/// directory's parent, and so on up to the filesystem root), rejecting
/// as soon as any ANCESTOR THAT ALREADY EXISTS is found to be a reparse
/// point/junction/symlink. An ancestor that does not exist yet is simply
/// skipped -- there is nothing planted there to redirect through, and
/// [`fs::symlink_metadata`] never follows the final component itself, so
/// this never silently traverses through a redirecting component to
/// reach the one above it. `candidate` itself (the leaf evidence-output
/// file) is intentionally excluded from this walk: it need not exist
/// yet, and even if something unexpected already sits there, the actual
/// write path (`write_evidence_output`) always creates/overwrites the
/// exact named leaf itself rather than following it as a directory.
fn reject_reparse_point_ancestor(candidate: &Path) -> Result<(), String> {
    for ancestor in candidate.ancestors().skip(1) {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                if is_reparse_point(&metadata) {
                    return Err(format!(
                        "--evidence-output must not resolve through a reparse \
                         point/junction/symlink, but '{}' is one",
                        ancestor.display()
                    ));
                }
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => {
                return Err(format!(
                    "--evidence-output could not be validated against ancestor '{}': {err}",
                    ancestor.display()
                ));
            }
        }
    }
    Ok(())
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
    /// `true` only when the inner child was torn down via
    /// `ChildGuard::kill_and_wait` (a deadline/error path) AND that
    /// teardown could not be confirmed within `process::KILL_WAIT_BUDGET`
    /// -- i.e. `kill()` itself failed, or the child was still not
    /// reapable once the bounded wait elapsed. `false` for a normal
    /// completion (nothing was torn down) or a confirmed clean teardown.
    /// Discarding `kill_and_wait`'s result previously meant this
    /// function could report completion while its owned inner process
    /// might still be alive; this field makes that outcome explicit
    /// instead of silently clean (Copilot review, 2026-09 -- 049-S PR
    /// #120, round 4).
    pub inner_teardown_incomplete: bool,
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

    // Residual/unknown descendants (Copilot review, 2026-09 -- 049-S PR
    // #120): a platform that reparents orphaned children away from
    // their original parent typically does so at or before that
    // parent's own exit, which for a wedged/killed inner process can
    // happen strictly between this point and the post-teardown
    // observation below. Querying `child_pids` only after teardown can
    // therefore silently miss exactly the residual descendants this
    // diagnostic exists to surface. Take a first, best-effort candidate
    // snapshot right now while the inner process is freshly alive
    // (nothing has spawned yet, but an already-detaching child could
    // already be visible), and take a second snapshot immediately after
    // the pump returns but strictly before teardown below; the union of
    // both candidate sets is then re-observed once the guard has torn
    // the inner process down. This is still best-effort, never a proof
    // of completeness -- a descendant that both spawns AND is reparented
    // inside either narrow window between snapshots can still be missed
    // -- but it materially widens the observation window versus a single
    // post-teardown-only query.
    let mut candidate_descendant_pids: Vec<u32> = guard
        .pid()
        .map_or_else(Vec::new, |inner_pid| observer.child_pids(inner_pid));

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

    if let Some(inner_pid) = guard.pid() {
        candidate_descendant_pids.extend(observer.child_pids(inner_pid));
    }

    let (inner_exit_code, inner_teardown_incomplete) = if pump.timed_out {
        // Deadline/error teardown: the inner child never closed both
        // directions on its own within the bound. Tear it down right now
        // via the owned direct guard rather than only-eventually via
        // `Drop`, then report no exit code -- the child was killed, not
        // waited-to-completion. `kill_and_wait`'s own return value is
        // recorded, never discarded: a `false` here means teardown could
        // not be confirmed within budget, so the inner process might
        // still be alive despite this function otherwise completing
        // (Copilot review, 2026-09 -- 049-S PR #120, round 4).
        let confirmed = guard.kill_and_wait();
        (None, !confirmed)
    } else {
        // Normal pump completion (both stdio directions closed) does
        // NOT by itself prove the inner child has exited -- it could
        // still be alive holding neither pipe open. Production `wrapper`
        // runs configure no pump deadline at all, so an unconditional,
        // unbounded `wait()` immediately here could hang this call (and
        // this whole subcommand) forever in that scenario. Bound the
        // wait, falling back to the same owned kill/reap authority if
        // the child cannot be confirmed exited within budget (Copilot
        // review, 2026-09 -- 049-S PR #120, round 5).
        let (status, teardown_incomplete) = guard.bounded_wait_then_kill(KILL_WAIT_BUDGET);
        (status.and_then(|s| s.code()), teardown_incomplete)
    };

    let wrapper_identity = observer.observe(std::process::id());

    let sentinel_inherited = std::env::var(ENV_INHERITANCE_SENTINEL_VAR).ok();

    // Re-observe the union of both pre-teardown candidate snapshots
    // above, now that the owned guard's teardown has run. Diagnostic
    // only: surfaced, never acted on -- this task never kills them.
    candidate_descendant_pids.sort_unstable();
    candidate_descendant_pids.dedup();
    let residual_descendants = candidate_descendant_pids
        .into_iter()
        .filter_map(|pid| observer.observe(pid))
        .collect();

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
    // Likewise, `pump.delivery_drain_incomplete` (Copilot review thread
    // H, 2026-09 -- 049-S PR #120, round 2) signals that transport's own
    // delivery worker did not finish draining its already-accepted
    // copies before the pump returned; this collector has no way to
    // detect that on its own either, so it too must be told explicitly
    // before `finalize()`.
    evidence_collector.note_transport_delivery_drain_incomplete(pump.delivery_drain_incomplete);
    // And likewise, the inner child's own teardown confirmation (see the
    // `inner_teardown_incomplete` computation above) is another signal
    // this collector cannot observe on its own -- see
    // `EvidenceCollector::note_inner_teardown_incomplete`'s own doc
    // comment for why this makes gating flow through the existing
    // `leg_has_valid_initialize` pipeline for free (Copilot review,
    // 2026-09 -- 049-S PR #120, round 5).
    evidence_collector.note_inner_teardown_incomplete(inner_teardown_incomplete);
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
        inner_teardown_incomplete,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // `bounded_wait_after_kill`'s own process-spawning regression
    // coverage lives in the integration test
    // `kill_and_wait_tears_down_a_still_running_child_within_a_bounded_wall_clock_window`
    // in `tests/process_test.rs`, not here: a unit test's own
    // `std::env::current_exe()`/`CARGO_BIN_EXE_mcp-probe` story does not
    // resolve to this crate's real `main.rs` dispatch from inside the
    // `cargo test` harness binary that runs THIS module -- see
    // `crate::transport`'s module doc comment for the full rationale,
    // which applies identically here. This module only covers the pure,
    // non-process-spawning logic added alongside that fix.

    #[test]
    fn validate_evidence_output_path_accepts_a_clean_absolute_path() {
        let candidate = if cfg!(windows) {
            r"C:\probe-workspace\evidence.json"
        } else {
            "/probe-workspace/evidence.json"
        };
        assert!(validate_evidence_output_path(candidate).is_ok());
    }

    #[test]
    fn validate_evidence_output_path_rejects_a_relative_path() {
        let err = validate_evidence_output_path("evidence.json")
            .expect_err("a relative path must be rejected");
        assert!(err.contains("absolute"), "unexpected message: {err}");
    }

    #[test]
    fn validate_evidence_output_path_accepts_ancestors_that_do_not_exist_yet() {
        // The existing clean-absolute-path fixture above already
        // exercises this implicitly, but this test makes the invariant
        // explicit: `reject_reparse_point_ancestor` must never fail
        // closed on an ancestor that simply is not present on disk yet
        // -- only on one that IS present and IS a reparse point.
        let unique = format!(
            "mcp-probe-evidence-path-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        );
        let candidate = std::env::temp_dir()
            .join(unique)
            .join("does-not-exist-yet")
            .join("evidence.json");
        assert!(
            candidate.is_absolute(),
            "std::env::temp_dir() must be absolute for this test to be meaningful"
        );
        assert!(
            validate_evidence_output_path(&candidate.to_string_lossy()).is_ok(),
            "an absolute, traversal-free path with no existing ancestors must be accepted"
        );
    }

    #[cfg(windows)]
    #[test]
    fn validate_evidence_output_path_rejects_a_junction_ancestor() {
        // Round 4 (Copilot review, 2026-09 -- 049-S PR #120): an
        // absolute, `..`-free path can still resolve somewhere entirely
        // different from what it appears to name if an ancestor
        // component is a junction/reparse point. This proves
        // `validate_evidence_output_path` now fails closed on that,
        // mirroring `workspace.rs`'s own junction-escape test fixtures
        // in `tests/workspace_test.rs`.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let base = std::env::temp_dir().join(format!(
            "mcp-probe-evidence-junction-test-{}-{nanos}",
            std::process::id()
        ));
        let junction_point = base.join("probe-workspace");
        let junction_target = std::env::temp_dir().join(format!(
            "mcp-probe-evidence-junction-target-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&base).expect("create base test dir");
        fs::create_dir_all(&junction_target).expect("create junction target dir");

        let mklink_ok = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                &junction_point.to_string_lossy(),
                &junction_target.to_string_lossy(),
            ])
            .status()
            .is_ok_and(|status| status.success());
        if !mklink_ok {
            eprintln!("skipping: mklink /J failed to create a junction in this environment");
            let _ = fs::remove_dir_all(&base);
            let _ = fs::remove_dir_all(&junction_target);
            return;
        }

        let candidate = junction_point.join("evidence.json");
        let err = validate_evidence_output_path(&candidate.to_string_lossy())
            .expect_err("a junction ancestor must be rejected");
        assert!(
            err.contains("reparse point/junction/symlink"),
            "unexpected message: {err}"
        );

        let _ = fs::remove_dir(&junction_point);
        let _ = fs::remove_dir_all(&base);
        let _ = fs::remove_dir_all(&junction_target);
    }

    #[test]
    fn validate_evidence_output_path_rejects_a_traversal_component() {
        let candidate = if cfg!(windows) {
            r"C:\probe-workspace\..\..\secrets\evidence.json"
        } else {
            "/probe-workspace/../../secrets/evidence.json"
        };
        let err = validate_evidence_output_path(candidate)
            .expect_err("a '..' component must be rejected even inside an absolute path");
        assert!(err.contains(".."), "unexpected message: {err}");
    }

    #[test]
    fn parse_wrapper_args_surfaces_the_evidence_output_validation_error() {
        let args = vec![
            "--inner-exe".to_string(),
            "some-exe".to_string(),
            "--evidence-output".to_string(),
            "relative/evidence.json".to_string(),
            "--run-nonce".to_string(),
            "nonce".to_string(),
        ];
        let err = parse_wrapper_args(args.into_iter()).expect_err(
            "a relative --evidence-output must fail parsing, not just be accepted verbatim",
        );
        assert!(err.contains("absolute"), "unexpected message: {err}");
    }
}
