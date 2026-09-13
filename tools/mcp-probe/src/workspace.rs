//! Isolated probe workspace and control/treatment/ancestor `.mcp.json`
//! config fixtures for the standalone `mcp-probe` diagnostic crate
//! (`056.021-T`), composed onto the `056.020-T` transport, the
//! `056.022-T` process guards, and the `056.023-T` observer/evidence
//! seam -- this module reimplements none of them, and owns no
//! observer/evidence module of its own.
//!
//! # What this module owns
//!
//! * Exclusive creation of a fresh isolated workspace under the
//!   canonical repository path `logs/probe/<nonce>`.
//! * A probe-local, std-only containment check (`canonicalize` +
//!   `symlink_metadata`) -- this module never imports `graphtor_core`
//!   and never claims a reusable production path/security primitive.
//! * Generating the temporary control and treatment `.mcp.json` fixtures
//!   used by `056.001-T`'s future one-shot exact-CLI classification.
//! * Generating an owned nested ancestor/child config-discovery fixture
//!   used by `056.001-T`'s Gate 1 (ancestor config-isolation) proof.
//!
//! # What this module never does
//!
//! It never reads, modifies, backs up, restores, or substitutes the
//! user's real `.mcp.json` -- every config this module writes lives
//! exclusively inside its own freshly created, exclusively-owned
//! workspace. It performs no production acceptance and runs no process;
//! the exact-CLI run that actually consumes these fixtures is
//! `056.001-T`.
//!
//! # Threat model (explicit)
//!
//! The `canonicalize` + `symlink_metadata` containment check protects
//! against **accidental** escape of the repository root (for example, a
//! tampered or redirected ancestor directory component) and against a
//! **pre-existing** reparse point/junction (Windows) or symlink (Unix)
//! sitting at the intended workspace path. It does **not** defend
//! against a malicious same-user process that can modify source code or
//! this workspace while the probe is running: the check-then-use window
//! between validating a path and using it (TOCTOU) is an **accepted
//! residual risk** for this non-sensitive, same-user diagnostic
//! workspace. This module is not a security boundary against a
//! same-user adversary; it is a guard against accidental misuse.
//!
//! # Byte-identical wrapper args (control vs. treatment)
//!
//! Both the control and treatment `.mcp.json` entries use the
//! `056.022-T` wrapper as `command` and pass **byte-identical** wrapper
//! args -- `--inner-exe`, the repeated original `--inner-arg` values,
//! `--evidence-output`, and `--run-nonce` are literally the same values
//! on both legs (both legs even share the same `evidence_output` file:
//! `056.001-T` runs them one at a time and reads/copies each leg's
//! evidence before the next leg overwrites it). The treatment entry
//! **alone** adds a `cwd` key (the canonical repository root, the H3-B
//! candidate fix) to its JSON object. No other arg, env, target, or
//! stdio-discriminator difference is ever introduced.

use std::fs;
use std::path::{Path, PathBuf};

/// Deliberately syntactically-invalid contents for the ancestor
/// config-discovery fixture's sentinel `.mcp.json`. Any CLI that
/// actually attempts to read or merge this file will fail loudly and
/// unambiguously (a parse error), which is a stronger, more legible
/// signal than a validly-shaped-but-wrong value would be. A CLI that
/// correctly discovers only the nearest (child) config never touches
/// this file at all.
const ANCESTOR_SENTINEL_CONTENTS: &str =
    "{ \"mcpServers\": SENTINEL-ANCESTOR-CONFIG-MUST-NEVER-BE-PARSED-OR-MERGED";

/// Everything that can go wrong constructing or validating a probe
/// workspace. Every variant is fail-closed: on any error, no unvalidated
/// path is ever used, followed, reused, or removed.
#[derive(Debug)]
pub enum WorkspaceError {
    /// Something already exists at the path this module intended to
    /// exclusively create (any type other than a reparse point/junction,
    /// which gets the more specific [`Self::ReparsePoint`]). Never
    /// reused, followed, or removed -- callers should choose a different
    /// nonce.
    AlreadyExists(PathBuf),
    /// `symlink_metadata` at this exact path reports a reparse
    /// point/junction (Windows) or symlink (Unix). Never followed, never
    /// treated as the intended plain directory.
    ReparsePoint(PathBuf),
    /// The candidate path's canonical form does not fall under the
    /// canonical repository root -- an accidental (or tampered) escape,
    /// most commonly caused by a reparse point/junction on an ancestor
    /// path component rather than on the candidate itself.
    ContainmentEscape {
        candidate: PathBuf,
        repo_root: PathBuf,
    },
    /// An ordinary I/O failure: directory creation, `canonicalize`,
    /// `symlink_metadata`, or a config file write.
    Io(std::io::Error),
    /// Building or serializing a generated `.mcp.json` document failed.
    Json(serde_json::Error),
    /// The caller-supplied `--run-nonce` is not a safe, single, relative
    /// path component (empty, contains a path separator, is `.`/`..`, or
    /// is itself absolute). Rejected before any join/create so a
    /// malicious or malformed nonce can never cause a directory to be
    /// created outside `logs/probe/` in the first place.
    InvalidNonce(String),
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyExists(path) => {
                write!(
                    f,
                    "path already exists, refusing to reuse it: {}",
                    path.display()
                )
            }
            Self::ReparsePoint(path) => {
                write!(
                    f,
                    "path is a reparse point/junction/symlink, refusing to follow or use it: {}",
                    path.display()
                )
            }
            Self::ContainmentEscape {
                candidate,
                repo_root,
            } => write!(
                f,
                "path {} canonicalizes outside the repository root {}",
                candidate.display(),
                repo_root.display()
            ),
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::Json(err) => write!(f, "config JSON error: {err}"),
            Self::InvalidNonce(nonce) => write!(
                f,
                "run-nonce {nonce:?} is not a safe single relative path component \
                 (must not be empty, contain a path separator, be '.'/'..', or be absolute)"
            ),
        }
    }
}

impl std::error::Error for WorkspaceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Json(err) => Some(err),
            Self::AlreadyExists(_)
            | Self::ReparsePoint(_)
            | Self::ContainmentEscape { .. }
            | Self::InvalidNonce(_) => None,
        }
    }
}

impl From<std::io::Error> for WorkspaceError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<serde_json::Error> for WorkspaceError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err)
    }
}

/// Caller-supplied parameters for the wrapper-based `mcpServers` entry
/// this module writes into every generated `.mcp.json` fixture. This
/// module never reads the user's real `.mcp.json`; `056.001-T`'s future
/// exact-CLI runner is responsible for discovering these exact,
/// unmodified production values and passing them in here.
#[derive(Debug, Clone)]
pub struct McpServerEntrySpec {
    /// The `mcpServers` key this probe workspace generates (for example,
    /// `"graphtor-docs"`).
    pub entry_name: String,
    /// Absolute path to the compiled `mcp-probe` binary; becomes every
    /// generated entry's `command`.
    pub wrapper_exe: String,
    /// The exact, unmodified production inner executable
    /// (`--inner-exe`); byte-identical on both legs.
    pub inner_exe: String,
    /// The exact, unmodified original inner arguments, in order
    /// (each becomes one repeated `--inner-arg`); byte-identical on both
    /// legs.
    pub inner_args: Vec<String>,
}

/// The complete set of paths this module produced inside one exclusively
/// created `logs/probe/<nonce>` workspace.
#[derive(Debug, Clone)]
pub struct ProbeWorkspace {
    /// The exclusively created `logs/probe/<nonce>` directory itself --
    /// the only path [`remove_probe_workspace`] is ever allowed to
    /// remove.
    pub root: PathBuf,
    pub nonce: String,
    /// Shared by both the control and treatment `.mcp.json` entries (see
    /// module docs: wrapper args are byte-identical on both legs), so
    /// this is deliberately one path, not two.
    pub evidence_output: PathBuf,
    pub control_dir: PathBuf,
    pub control_config_path: PathBuf,
    pub treatment_dir: PathBuf,
    pub treatment_config_path: PathBuf,
    /// The candidate working directory written into the treatment leg's
    /// `cwd` -- the canonical repository root.
    pub treatment_cwd: PathBuf,
    pub ancestor_dir: PathBuf,
    /// The deliberately invalid/sentinel ancestor `.mcp.json`.
    pub ancestor_config_path: PathBuf,
    pub ancestor_run_dir: PathBuf,
    /// The intended, valid, temporary child `.mcp.json` that must shadow
    /// (not merge with) `ancestor_config_path`.
    pub ancestor_run_config_path: PathBuf,
    pub ancestor_run_evidence_output: PathBuf,
}

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

/// Rejects a leaf path this module is about to exclusively create if
/// anything at all already sits there: a reparse point/junction/symlink
/// gets the specific [`WorkspaceError::ReparsePoint`], anything else
/// gets [`WorkspaceError::AlreadyExists`]. Only `NotFound` is `Ok(())`.
fn reject_if_leaf_unsafe(path: &Path) -> Result<(), WorkspaceError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_reparse_point(&metadata) => {
            Err(WorkspaceError::ReparsePoint(path.to_path_buf()))
        }
        Ok(_) => Err(WorkspaceError::AlreadyExists(path.to_path_buf())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(WorkspaceError::Io(err)),
    }
}

/// Validates that `nonce` is a single, safe, relative path component
/// before it is ever joined onto `logs/probe/` or used to create a
/// directory. Rejects an empty string, any embedded path separator
/// (`/` or `\` on every platform, not just the host's native one, since
/// a `--run-nonce` value is arbitrary caller-supplied text and this
/// check must not depend on which OS produced it), the special `.`/`..`
/// components, and an absolute path. This runs strictly BEFORE any
/// `Path::join`/`fs::create_dir` call in [`create_probe_workspace`] so a
/// malformed or malicious nonce can never cause a directory to be
/// created outside `logs/probe/<nonce>` in the first place -- unlike
/// [`validate_containment`], which can only validate a path that
/// already exists on disk.
fn validate_nonce(nonce: &str) -> Result<(), WorkspaceError> {
    let is_safe = !nonce.is_empty()
        && nonce != "."
        && nonce != ".."
        && !nonce.contains('/')
        && !nonce.contains('\\')
        && !Path::new(nonce).is_absolute();
    if is_safe {
        Ok(())
    } else {
        Err(WorkspaceError::InvalidNonce(nonce.to_string()))
    }
}

/// Validates that `candidate` is not itself a reparse point/junction/
/// symlink and that its canonical form falls under `canonical_repo_root`
/// (which the caller must have already canonicalized). Catches both a
/// reparse point directly on `candidate` and an escape caused by a
/// reparse point on any ancestor path component (canonicalizing through
/// it resolves outside the repository root).
fn validate_containment(
    canonical_repo_root: &Path,
    candidate: &Path,
) -> Result<(), WorkspaceError> {
    let candidate_link_metadata = fs::symlink_metadata(candidate)?;
    if is_reparse_point(&candidate_link_metadata) {
        return Err(WorkspaceError::ReparsePoint(candidate.to_path_buf()));
    }
    let canonical_candidate = fs::canonicalize(candidate)?;
    if !canonical_candidate.starts_with(canonical_repo_root) {
        return Err(WorkspaceError::ContainmentEscape {
            candidate: canonical_candidate,
            repo_root: canonical_repo_root.to_path_buf(),
        });
    }
    Ok(())
}

/// Creates one path component of the shared, cross-run parent chain
/// (`logs/`, then `logs/probe/`) if it does not already exist, and
/// validates containment (never following a reparse point/junction)
/// BEFORE ever creating or entering the *next* component beneath it.
///
/// This is deliberately NOT `fs::create_dir_all`: a single
/// `create_dir_all(repo_root.join("logs").join("probe"))` call treats a
/// pre-existing `logs` as "already there" and transparently creates
/// `probe` *inside* it -- silently traversing through `logs` even if
/// `logs` itself is a reparse point/junction redirecting outside
/// `repo_root`, before any containment check ever runs. Validating each
/// component in turn, in this exact create-or-check-then-validate
/// order, closes that gap: a tampered `logs` is caught here, at the
/// `logs` component itself, before `probe` is ever joined onto it.
fn create_shared_dir_component_validated(
    canonical_repo_root: &Path,
    path: &Path,
) -> Result<(), WorkspaceError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            // Something already sits at this exact component (the
            // ordinary case for a shared parent directory reused across
            // many probe runs). A reparse point/junction/symlink is
            // never followed or traversed into.
            if is_reparse_point(&metadata) {
                return Err(WorkspaceError::ReparsePoint(path.to_path_buf()));
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path)?;
        }
        Err(err) => return Err(WorkspaceError::Io(err)),
    }
    // Either freshly created just above, or a pre-existing plain
    // directory confirmed not to be a reparse point itself: re-validate
    // containment either way before the caller is allowed to join and
    // create anything beneath it.
    validate_containment(canonical_repo_root, path)
}

/// Builds the `wrapper` subcommand argv (see `056.022-T`'s
/// `parse_wrapper_args`: `--inner-exe`, repeated `--inner-arg`,
/// `--evidence-output`, `--run-nonce`) shared byte-for-byte by every
/// generated config in this module.
fn wrapper_argv(
    entry: &McpServerEntrySpec,
    evidence_output: &Path,
    run_nonce: &str,
) -> Vec<String> {
    let mut args = vec![
        "wrapper".to_string(),
        "--inner-exe".to_string(),
        entry.inner_exe.clone(),
    ];
    for inner_arg in &entry.inner_args {
        args.push("--inner-arg".to_string());
        args.push(inner_arg.clone());
    }
    args.push("--evidence-output".to_string());
    args.push(evidence_output.to_string_lossy().into_owned());
    args.push("--run-nonce".to_string());
    args.push(run_nonce.to_string());
    args
}

/// Writes one wrapper-based `.mcp.json` document to `path`: a single
/// `mcpServers` entry named `entry.entry_name`, `"type": "stdio"`,
/// `"command"` set to `entry.wrapper_exe`, `"args"` set to
/// [`wrapper_argv`]'s output, and -- only when `cwd` is `Some` -- a
/// `"cwd"` key. Built as an explicit `serde_json::Map`/`Value` tree
/// (never `#[derive(Serialize)]`), matching `056.023-T`'s own
/// manual-JSON style and adding no new dependency beyond the
/// already-shared standalone `serde_json` crate.
fn write_wrapper_mcp_json(
    path: &Path,
    entry: &McpServerEntrySpec,
    evidence_output: &Path,
    run_nonce: &str,
    cwd: Option<&Path>,
) -> Result<(), WorkspaceError> {
    let args = wrapper_argv(entry, evidence_output, run_nonce);

    let mut server_entry = serde_json::Map::new();
    server_entry.insert(
        "type".to_string(),
        serde_json::Value::String("stdio".to_string()),
    );
    server_entry.insert(
        "command".to_string(),
        serde_json::Value::String(entry.wrapper_exe.clone()),
    );
    server_entry.insert(
        "args".to_string(),
        serde_json::Value::Array(args.into_iter().map(serde_json::Value::String).collect()),
    );
    if let Some(cwd) = cwd {
        server_entry.insert(
            "cwd".to_string(),
            serde_json::Value::String(cwd.to_string_lossy().into_owned()),
        );
    }

    let mut mcp_servers = serde_json::Map::new();
    mcp_servers.insert(
        entry.entry_name.clone(),
        serde_json::Value::Object(server_entry),
    );

    let mut document = serde_json::Map::new();
    document.insert(
        "mcpServers".to_string(),
        serde_json::Value::Object(mcp_servers),
    );

    let bytes = serde_json::to_vec_pretty(&serde_json::Value::Object(document))?;
    fs::write(path, bytes)?;
    Ok(())
}

/// Creates a fresh, exclusively owned probe workspace at
/// `repo_root/logs/probe/<nonce>` and populates it with the control,
/// treatment, and ancestor/child config fixtures described in the module
/// docs. Performs no production acceptance and spawns no process --
/// `056.001-T`'s exact-CLI runner is the sole future consumer of the
/// paths this returns.
///
/// # Errors
///
/// Returns [`WorkspaceError::AlreadyExists`] or
/// [`WorkspaceError::ReparsePoint`] if anything already sits at the
/// intended workspace path; [`WorkspaceError::ContainmentEscape`] if the
/// shared `logs/probe` parent chain or the freshly created workspace
/// itself canonicalizes outside `repo_root` (most commonly caused by a
/// reparse point on an ancestor component); [`WorkspaceError::InvalidNonce`]
/// if `nonce` is not a safe single relative path component (checked
/// FIRST, before anything is joined or created, so an unsafe nonce can
/// never reach the filesystem at all); and [`WorkspaceError::Io`] /
/// [`WorkspaceError::Json`] for ordinary filesystem or serialization
/// failures.
pub fn create_probe_workspace(
    repo_root: &Path,
    nonce: &str,
    entry: &McpServerEntrySpec,
) -> Result<ProbeWorkspace, WorkspaceError> {
    // Reject an unsafe nonce (path separator, `..`, absolute, empty)
    // BEFORE any `Path::join`/`fs::create_dir` call below -- validating
    // only after creation (as `validate_containment` must, since it
    // needs the path to already exist to `canonicalize` it) would let a
    // malicious or malformed nonce cause a directory to be created
    // outside `logs/probe/` first and only be caught afterward.
    validate_nonce(nonce)?;

    let canonical_repo_root = fs::canonicalize(repo_root)?;

    // Shared parent chain across many probe runs: each component is
    // validated (never followed if a reparse point) BEFORE the next
    // component is joined onto it or created -- see
    // create_shared_dir_component_validated's doc comment for why this
    // must NOT be a single fs::create_dir_all call.
    let logs_dir = repo_root.join("logs");
    create_shared_dir_component_validated(&canonical_repo_root, &logs_dir)?;
    let probe_root = logs_dir.join("probe");
    create_shared_dir_component_validated(&canonical_repo_root, &probe_root)?;

    // The exclusively created leaf: reject any pre-existing path first
    // (specific ReparsePoint vs. generic AlreadyExists), then create,
    // then re-validate the freshly created directory itself.
    let workspace_root = probe_root.join(nonce);
    reject_if_leaf_unsafe(&workspace_root)?;
    fs::create_dir(&workspace_root)?;
    validate_containment(&canonical_repo_root, &workspace_root)?;

    let control_dir = workspace_root.join("control");
    let treatment_dir = workspace_root.join("treatment");
    let ancestor_dir = workspace_root.join("ancestor");
    let ancestor_run_dir = ancestor_dir.join("run");
    fs::create_dir_all(&control_dir)?;
    fs::create_dir_all(&treatment_dir)?;
    // Creates `ancestor_dir` as a side effect of creating its child.
    fs::create_dir_all(&ancestor_run_dir)?;

    // Control and treatment share ONE evidence_output path: both legs'
    // wrapper args are byte-identical, and `056.001-T` runs them
    // sequentially, reading/copying each leg's evidence before the next
    // leg overwrites it.
    let evidence_output = workspace_root.join("evidence.json");
    let control_config_path = control_dir.join(".mcp.json");
    let treatment_config_path = treatment_dir.join(".mcp.json");
    let treatment_cwd = canonical_repo_root.clone();

    write_wrapper_mcp_json(&control_config_path, entry, &evidence_output, nonce, None)?;
    write_wrapper_mcp_json(
        &treatment_config_path,
        entry,
        &evidence_output,
        nonce,
        Some(&treatment_cwd),
    )?;

    let ancestor_config_path = ancestor_dir.join(".mcp.json");
    fs::write(&ancestor_config_path, ANCESTOR_SENTINEL_CONTENTS)?;

    let ancestor_run_config_path = ancestor_run_dir.join(".mcp.json");
    let ancestor_run_evidence_output = ancestor_run_dir.join("evidence.json");
    write_wrapper_mcp_json(
        &ancestor_run_config_path,
        entry,
        &ancestor_run_evidence_output,
        nonce,
        None,
    )?;

    Ok(ProbeWorkspace {
        root: workspace_root,
        nonce: nonce.to_string(),
        evidence_output,
        control_dir,
        control_config_path,
        treatment_dir,
        treatment_config_path,
        treatment_cwd,
        ancestor_dir,
        ancestor_config_path,
        ancestor_run_dir,
        ancestor_run_config_path,
        ancestor_run_evidence_output,
    })
}

/// Removes exactly the owned `workspace.root` directory tree and nothing
/// else -- never `logs/probe`, never a sibling probe workspace, never
/// anything outside `workspace.root`.
///
/// # Destructive-cleanup policy
///
/// This is the only removal path this module provides, and it performs
/// no operator-approval check itself (it has no operator-interaction
/// surface). Any caller outside this crate's own self-tests -- in
/// practice, `056.001-T`'s future exact-CLI runner -- MUST treat invoking
/// this as a destructive action requiring the same real-time
/// operator-approval discipline as any other destructive cleanup in this
/// repository's workflow policies before ever calling it.
///
/// # Errors
///
/// Returns an error if the underlying recursive removal fails (for
/// example, a file still open elsewhere).
pub fn remove_probe_workspace(workspace: &ProbeWorkspace) -> std::io::Result<()> {
    fs::remove_dir_all(&workspace.root)
}
