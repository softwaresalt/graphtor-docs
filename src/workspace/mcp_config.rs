//! MCP client configuration generation.
//!
//! Generates a single workspace-root `.mcp.json` configuration file that
//! registers the installed `graphtor-docs` binary as an MCP server. This is
//! the editor-agnostic standard understood by MCP clients; graphtor-docs no
//! longer writes editor-specific config files.
//!
//! [`generate_mcp_config`] is the shared foundation both the minimal and full
//! install paths consume. It resolves the server `command` via a binary
//! resolution ladder (an absolute, canonicalized path when a managed binary
//! exists under `.graphtor/bin/`, otherwise the bare `graphtor-docs` PATH
//! command), writes a provenance marker into every managed entry so it can be
//! recognized independent of the command string, and applies a locked
//! four-way decision on the fixed `graphtor-docs` key: absent -> insert;
//! present and marked -> refresh in place; present, unmarked, but exactly the
//! legacy pre-marker shape -> migrate in place (marker added); present,
//! unmarked, any other shape -> fail closed rather than overwrite a user's
//! own entry. Writes are atomic (temp file + rename).
//!
//! Uninstall is surgical: it parses each candidate config and removes only the
//! server entry graphtor-docs manages (identified by the provenance marker,
//! or by its `.graphtor/bin/` binary command for entries written before the
//! marker existed), preserving any other MCP servers in a shared `.mcp.json`.
//! A file is deleted outright only when the managed entry was its sole server.

use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use graphtor_core::path::validate_path;
use graphtor_core::GraphtorError;

/// Workspace-root MCP client config path (the current standard).
const MCP_CONFIG_PATH: &str = ".mcp.json";

/// Legacy editor-specific config paths cleaned up on uninstall.
const LEGACY_CONFIG_PATHS: &[&str] = &[
    ".vscode/mcp.json",
    ".cursor/mcp.json",
    ".github/copilot/mcp.json",
];

/// The fixed, LOCKED key graphtor-docs manages in `mcpServers`.
const MCP_SERVER_KEY: &str = "graphtor-docs";

/// Provenance marker key written into every graphtor-docs-managed server
/// entry. Its presence (set to `true`), not the command string, is the
/// forward-looking identity used to recognize a managed entry — the command
/// value changes shape (bare PATH command vs. absolute pinned path)
/// depending on whether a managed binary exists, so the marker is the only
/// stable signal across that variation.
const MANAGED_MARKER_KEY: &str = "x-graphtor-managed";

/// Historical RELATIVE command shapes written by the pre-marker writer
/// (before this module introduced the resolution ladder + provenance
/// marker). Used ONLY for EXACT-equality backward-compat recognition when
/// migrating an unmarked entry in place — NEVER a substring/contains test. A
/// user-authored command that merely embeds one of these strings as a
/// prefix or suffix (e.g. `/opt/tools/.graphtor/bin/graphtor-docs`) is a
/// genuine collision (case 4), not a legacy entry to migrate.
const LEGACY_COMMAND_SHAPES: &[&str] = &[
    ".graphtor/bin/graphtor-docs",
    ".graphtor/bin/graphtor-docs.exe",
];

/// The change applied to a single MCP config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpConfigAction {
    /// A new config file was created.
    Created,
    /// The graphtor-docs entry was added to or pruned from an existing shared
    /// config; the file was kept with its other servers intact.
    Updated,
    /// The config file was deleted (the managed entry was its sole server).
    Removed,
}

impl McpConfigAction {
    /// Lowercase string form for structured (JSON) output.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Removed => "removed",
        }
    }
}

/// A per-file outcome of an MCP config mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpConfigOutcome {
    /// Config file path, relative to the project root.
    pub path: String,
    /// What happened to the file.
    pub action: McpConfigAction,
}

/// Generate or merge the graphtor-docs server into the workspace `.mcp.json`.
///
/// The server `command` is resolved via a binary resolution ladder: when a
/// managed binary exists at `<project_root>/.graphtor/bin/graphtor-docs[.exe]`,
/// the command is that binary's ABSOLUTE, canonicalized path (so the entry
/// resolves regardless of the MCP client's launch working directory);
/// otherwise it is the bare `graphtor-docs` PATH command (no platform
/// extension — Windows resolves it via PATHEXT).
///
/// When `.mcp.json` does not exist it is created with a new managed
/// `graphtor-docs` server entry, carrying the managed-entry provenance marker
/// ([`McpConfigAction::Created`]). When it already exists and is a valid JSON
/// object, the fixed `graphtor-docs` key is resolved through a locked
/// four-way decision:
///
/// 1. **Absent** — a new marked managed entry is inserted.
/// 2. **Present and marked** (carries the provenance marker) — refreshed in
///    place via the resolution ladder; a no-op (`Ok(None)`) when the
///    refreshed value is identical to what is already there.
/// 3. **Present, unmarked, but exactly the legacy pre-marker shape** (the
///    historical relative command, `args == ["serve"]`, `transport ==
///    "stdio"`, no marker) — migrated in place: the provenance marker is
///    added and the value is refreshed via the resolution ladder. This is
///    the current release's own pre-marker entry, not a user collision.
/// 4. **Present, unmarked, any other shape** — a genuine collision with a
///    user-authored entry. The file is left byte-for-byte unchanged and this
///    returns [`GraphtorError::Config`].
///
/// Any other server already present is always preserved untouched. Writes
/// are atomic (temp file + rename). Returns `None` when the existing file is
/// not a JSON object — it is left untouched rather than clobbered — or when
/// case 2 above determines nothing changed.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] on I/O failure, on a case-4 collision
/// with an unmarked, non-legacy-shaped `graphtor-docs` entry, or
/// [`GraphtorError::PathViolation`] if the managed binary path unexpectedly
/// resolves outside `project_root`.
pub fn generate_mcp_config(project_root: &Path) -> Result<Option<McpConfigOutcome>, GraphtorError> {
    let dest = project_root.join(MCP_CONFIG_PATH);
    let command = resolve_command(project_root)?;

    if !dest.exists() {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| GraphtorError::Config {
                message: format!("failed to create {}: {e}", parent.display()),
                field: None,
            })?;
        }
        let document = serde_json::json!({
            "mcpServers": { MCP_SERVER_KEY: managed_server_value(&command) }
        });
        write_json(&dest, &document, MCP_CONFIG_PATH)?;
        return Ok(Some(McpConfigOutcome {
            path: MCP_CONFIG_PATH.to_string(),
            action: McpConfigAction::Created,
        }));
    }

    // Merge into an existing config without disturbing unrelated servers.
    let content = fs::read_to_string(&dest).map_err(|e| GraphtorError::Config {
        message: format!("failed to read {MCP_CONFIG_PATH}: {e}"),
        field: None,
    })?;
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&content) else {
        // Not valid JSON — do not clobber the user's file.
        return Ok(None);
    };
    let Some(root) = value.as_object_mut() else {
        return Ok(None);
    };
    let servers = root
        .entry("mcpServers")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    let Some(servers) = servers.as_object_mut() else {
        return Ok(None);
    };

    match classify_existing_entry(servers.get(MCP_SERVER_KEY)) {
        ExistingEntryClass::Absent | ExistingEntryClass::LegacyShape => {
            servers.insert(MCP_SERVER_KEY.to_string(), managed_server_value(&command));
            write_json(&dest, &value, MCP_CONFIG_PATH)?;
            Ok(Some(McpConfigOutcome {
                path: MCP_CONFIG_PATH.to_string(),
                action: McpConfigAction::Updated,
            }))
        }
        ExistingEntryClass::Marked => {
            let refreshed = managed_server_value(&command);
            if servers.get(MCP_SERVER_KEY) == Some(&refreshed) {
                return Ok(None);
            }
            servers.insert(MCP_SERVER_KEY.to_string(), refreshed);
            write_json(&dest, &value, MCP_CONFIG_PATH)?;
            Ok(Some(McpConfigOutcome {
                path: MCP_CONFIG_PATH.to_string(),
                action: McpConfigAction::Updated,
            }))
        }
        ExistingEntryClass::Collision => Err(GraphtorError::Config {
            message: format!(
                "'{MCP_SERVER_KEY}' already exists in {MCP_CONFIG_PATH} and is not a \
                 graphtor-docs-managed entry; refusing to overwrite it. Remove or rename the \
                 conflicting entry to let graphtor-docs manage '{MCP_SERVER_KEY}'."
            ),
            field: Some(format!("mcpServers.{MCP_SERVER_KEY}")),
        }),
    }
}

/// Classification of the existing value (if any) at the fixed
/// [`MCP_SERVER_KEY`], used by [`generate_mcp_config`]'s four-way decision.
enum ExistingEntryClass {
    /// No entry exists at this key.
    Absent,
    /// An entry exists and carries the managed-entry provenance marker.
    Marked,
    /// An entry exists, is unmarked, but exactly matches the historical
    /// pre-marker managed shape — the current release's own legacy entry.
    LegacyShape,
    /// An entry exists, is unmarked, and does not match the legacy shape —
    /// a genuine collision with a user-authored entry.
    Collision,
}

fn classify_existing_entry(existing: Option<&serde_json::Value>) -> ExistingEntryClass {
    let Some(existing) = existing else {
        return ExistingEntryClass::Absent;
    };
    if is_marked(existing) {
        ExistingEntryClass::Marked
    } else if is_exact_legacy_shape(existing) {
        ExistingEntryClass::LegacyShape
    } else {
        ExistingEntryClass::Collision
    }
}

/// Resolve the `command` value for the managed graphtor-docs MCP server
/// entry via the binary resolution ladder.
///
/// When a managed binary exists at
/// `<project_root>/.graphtor/bin/graphtor-docs[.exe]`, returns its ABSOLUTE,
/// canonicalized path — computed from the canonical `project_root` — so the
/// entry resolves regardless of the MCP client's launch working directory. A
/// bare workspace-relative string is deliberately NOT used: an MCP client may
/// start the server from a different working directory and would then fail
/// to resolve it. When no managed binary exists, returns the bare
/// `graphtor-docs` PATH command (no platform extension; Windows resolves it
/// via PATHEXT). This bare-PATH fallback carries a documented binary-hijack
/// trade-off; the absolute pinned path is always preferred when a managed
/// binary is known to exist.
///
/// # Errors
///
/// Returns [`GraphtorError::PathViolation`] if the managed binary path
/// unexpectedly resolves outside `project_root` (defence in depth — this
/// path is always constructed from `project_root` itself).
fn resolve_command(project_root: &Path) -> Result<String, GraphtorError> {
    let workspace_dir = project_root.join(super::paths::GRAPHTOR_DIR);
    let managed_binary = super::install::installed_binary_path(&workspace_dir);
    if managed_binary.exists() {
        let canonical = validate_path(&managed_binary, project_root)?;
        return Ok(canonical.display().to_string());
    }
    Ok("graphtor-docs".to_string())
}

/// Returns `true` when `entry` carries the managed-entry provenance marker.
fn is_marked(entry: &serde_json::Value) -> bool {
    entry
        .get(MANAGED_MARKER_KEY)
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

/// Returns `true` when `entry` EXACTLY matches the historical pre-marker
/// managed shape: a JSON object containing EXACTLY the three keys
/// `{command, args, transport}` where `command` equals one of
/// [`LEGACY_COMMAND_SHAPES`] exactly, `args == ["serve"]`, and
/// `transport == "stdio"`. This is deliberately an exact-equality check on
/// both the values AND the key set, never a substring/contains test — a user
/// command that merely embeds a legacy path, or an entry that carries these
/// three fields PLUS any extra key (e.g. `env`, `cwd`, `type`), is a genuine
/// collision (case 4) to preserve, NOT this release's own pre-marker entry to
/// migrate in place. Requiring an exact key count prevents install from
/// overwriting — or uninstall from removing — a user-authored entry that
/// happens to share the three historical fields (data loss).
fn is_exact_legacy_shape(entry: &serde_json::Value) -> bool {
    let Some(object) = entry.as_object() else {
        return false;
    };
    if object.len() != 3 {
        return false;
    }
    let Some(command) = object.get("command").and_then(serde_json::Value::as_str) else {
        return false;
    };
    if !LEGACY_COMMAND_SHAPES.contains(&command) {
        return false;
    }
    let args_match = object
        .get("args")
        .and_then(serde_json::Value::as_array)
        .is_some_and(|args| args.len() == 1 && args[0].as_str() == Some("serve"));
    let transport_match =
        object.get("transport").and_then(serde_json::Value::as_str) == Some("stdio");
    args_match && transport_match
}

/// The full set of MCP client config paths (relative to the project root)
/// graphtor-docs manages: the workspace-root `.mcp.json` plus the legacy
/// editor-specific files (`.vscode/mcp.json`, `.cursor/mcp.json`,
/// `.github/copilot/mcp.json`).
///
/// This is the single source of truth for the managed candidate set. Uninstall
/// consumes it to enumerate — for operator approval — which config files may
/// have their graphtor-docs entry pruned, then replays that exact approved
/// list through [`remove_mcp_config_from`].
#[must_use]
pub fn managed_config_candidates() -> Vec<String> {
    std::iter::once(MCP_CONFIG_PATH)
        .chain(LEGACY_CONFIG_PATHS.iter().copied())
        .map(str::to_string)
        .collect()
}

/// Returns `true` when `rel_path` (relative to `project_root`) exists, parses
/// as JSON, and currently holds a graphtor-docs entry that [`remove_mcp_config_from`]
/// would actually prune.
///
/// Uninstall planning uses this so the operator-approval preview lists ONLY the
/// config files execution will really modify — a candidate file that exists but
/// holds no managed entry (e.g. a user's own `.mcp.json` with unrelated
/// servers) is left off the plan, matching execution's shrink-only behavior.
/// The predicate is the same one pruning uses ([`prune_managed_server`]), so
/// there is no drift between what the plan advertises and what execution does.
#[must_use]
pub fn file_has_managed_entry(project_root: &Path, rel_path: &str) -> bool {
    let dest = project_root.join(rel_path);
    // Workspace-containment guard (Constitution III/IV): mirror the execution
    // guard in `remove_mcp_config_from`. A legacy candidate like
    // `.vscode/mcp.json` can traverse a symlinked/junction parent (`.vscode`),
    // so reading it here during PLANNING would cross the workspace boundary and
    // could advertise a prune of an external file that execution then (correctly)
    // skips. Treat an out-of-root candidate as "no managed entry" so planning
    // never reads outside the project and never previews a mutation that
    // execution's own guard would refuse — keeping plan and execution aligned.
    if validate_path(&dest, project_root).is_err() {
        return false;
    }
    let Ok(content) = fs::read_to_string(&dest) else {
        return false;
    };
    !matches!(prune_managed_server(&content), PruneOutcome::Unchanged)
}

/// Prune the managed graphtor-docs entry from ONLY the explicitly-listed
/// `rel_paths` (each relative to `project_root`), rather than rescanning the
/// full managed candidate set.
///
/// This is the exact-plan execution counterpart to [`remove_mcp_config`]: an
/// uninstall enumerates the config files it will touch UP FRONT (for operator
/// approval), then replays that exact list here so a managed config file
/// created AFTER the plan was shown is never silently mutated. A listed file
/// that no longer exists, is not valid JSON, or no longer holds a managed
/// entry is simply skipped (the set only ever SHRINKS at execution, never
/// expands). Pruning otherwise follows [`prune_managed_server`]: only the
/// fixed `graphtor-docs` key is removed, other servers are preserved, and the
/// file is deleted only when that entry was its sole content.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] on I/O failure.
pub fn remove_mcp_config_from(
    project_root: &Path,
    rel_paths: &[String],
) -> Result<Vec<McpConfigOutcome>, GraphtorError> {
    let mut outcomes: Vec<McpConfigOutcome> = Vec::new();
    for rel_path in rel_paths {
        let rel_path = rel_path.as_str();
        let dest = project_root.join(rel_path);
        // Workspace-containment guard (Constitution III/IV): a legacy candidate
        // like `.vscode/mcp.json` can traverse a symlinked/junction parent
        // (`.vscode`), so the remove or rewrite below would mutate a file
        // OUTSIDE the project. Resolve the destination and confirm it stays
        // within `project_root` before any read or mutation. A candidate that
        // does not exist yet resolves within root (validation walks up to the
        // deepest existing ancestor), so it still reaches the NotFound skip
        // below; only a genuine escape is a `PathViolation`, which we skip.
        match validate_path(&dest, project_root) {
            Ok(_) => {}
            Err(GraphtorError::PathViolation { .. }) => continue,
            Err(other) => return Err(other),
        }
        let content = match fs::read_to_string(&dest) {
            Ok(s) => s,
            Err(e) if e.kind() == ErrorKind::NotFound => continue,
            Err(e) => {
                return Err(GraphtorError::Config {
                    message: format!("failed to read {rel_path}: {e}"),
                    field: None,
                })
            }
        };
        match prune_managed_server(&content) {
            PruneOutcome::Unchanged => {}
            PruneOutcome::RemoveFile => {
                fs::remove_file(&dest).map_err(|e| GraphtorError::Config {
                    message: format!("failed to remove {rel_path}: {e}"),
                    field: None,
                })?;
                outcomes.push(McpConfigOutcome {
                    path: rel_path.to_string(),
                    action: McpConfigAction::Removed,
                });
            }
            PruneOutcome::Rewrite(new_value) => {
                write_json(&dest, &new_value, rel_path)?;
                outcomes.push(McpConfigOutcome {
                    path: rel_path.to_string(),
                    action: McpConfigAction::Updated,
                });
            }
        }
    }
    Ok(outcomes)
}

/// The action to take on a config file after pruning managed server entries.
enum PruneOutcome {
    /// No managed entry found (or not valid JSON); leave the file as-is.
    Unchanged,
    /// The managed entry was the file's sole server; delete the file.
    RemoveFile,
    /// The managed entry was removed but other servers remain; rewrite
    /// content. Carries the parsed [`serde_json::Value`] (not a
    /// pre-serialized string) so the caller can route the write through the
    /// shared atomic `write_json` helper, matching `generate_mcp_config`'s
    /// temp-file + rename guarantee.
    Rewrite(serde_json::Value),
}

/// Remove the graphtor-docs-managed server entry from an MCP config document.
///
/// Pruning is restricted to the fixed [`MCP_SERVER_KEY`] (`graphtor-docs`): a
/// user-authored copy or alias under a DIFFERENT key is ALWAYS preserved,
/// even if it happens to carry the managed marker or the legacy shape. Under
/// that fixed key, the entry is treated as "managed" when it carries the
/// managed-entry provenance marker ([`MANAGED_MARKER_KEY`]) — the primary,
/// forward-looking recognition path — OR, as a narrow backward-compat path
/// for entries written before the marker existed, when it EXACTLY matches the
/// historical pre-marker shape ([`is_exact_legacy_shape`]). Non-JSON input, a
/// config without the managed key, or a `graphtor-docs` entry that is not
/// recognized as managed yields [`PruneOutcome::Unchanged`], so shared
/// configs holding unrelated servers are never destroyed.
fn prune_managed_server(content: &str) -> PruneOutcome {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(content) else {
        return PruneOutcome::Unchanged;
    };
    let Some(servers) = value
        .get_mut("mcpServers")
        .and_then(serde_json::Value::as_object_mut)
    else {
        return PruneOutcome::Unchanged;
    };

    let managed_keys: Vec<String> = servers
        .iter()
        .filter(|(key, cfg)| key.as_str() == MCP_SERVER_KEY && is_managed_for_removal(cfg))
        .map(|(key, _)| key.clone())
        .collect();

    if managed_keys.is_empty() {
        return PruneOutcome::Unchanged;
    }
    for key in &managed_keys {
        // shift_remove keeps the relative order of the remaining servers so a
        // shared config produces a minimal diff (requires serde_json's
        // preserve_order feature).
        servers.shift_remove(key);
    }
    let servers_empty = servers.is_empty();

    if servers_empty
        && value
            .as_object()
            .is_some_and(|obj| obj.keys().all(|key| key == "mcpServers"))
    {
        return PruneOutcome::RemoveFile;
    }

    PruneOutcome::Rewrite(value)
}

/// Returns `true` when `cfg` should be treated as a graphtor-docs-managed
/// entry for REMOVAL purposes (P2-T5b): it carries the provenance marker
/// (primary, forward-looking path), OR, as a NARROW backward-compat path
/// for entries written before the marker existed, it EXACTLY matches the
/// historical pre-marker shape ([`is_exact_legacy_shape`]) — never a
/// substring/contains test, so a user-authored command that merely embeds
/// the legacy path as a prefix or suffix (or reuses the pinned command with
/// different args) is preserved, not removed.
fn is_managed_for_removal(cfg: &serde_json::Value) -> bool {
    is_marked(cfg) || is_exact_legacy_shape(cfg)
}

/// Build the graphtor-docs MCP server registration value, including the
/// managed-entry provenance marker ([`MANAGED_MARKER_KEY`]).
fn managed_server_value(command: &str) -> serde_json::Value {
    let mut entry = serde_json::Map::new();
    entry.insert(
        "command".to_string(),
        serde_json::Value::String(command.to_string()),
    );
    entry.insert("args".to_string(), serde_json::json!(["serve"]));
    entry.insert(
        "transport".to_string(),
        serde_json::Value::String("stdio".to_string()),
    );
    entry.insert(
        MANAGED_MARKER_KEY.to_string(),
        serde_json::Value::Bool(true),
    );
    serde_json::Value::Object(entry)
}

/// Serialize `value` as pretty JSON with a trailing newline and write it
/// ATOMICALLY: the serialized content is written to a temporary file in the
/// same directory, then renamed into place. A reader can therefore never
/// observe a partially-written file, and a crash mid-write leaves the
/// original file (or no file) intact rather than a truncated one. When `dest`
/// already exists, its permissions are captured and reapplied to the temp
/// file before the rename, so replacing a user-owned `0600` shared config
/// never widens it to the umask default.
fn write_json(dest: &Path, value: &serde_json::Value, rel_path: &str) -> Result<(), GraphtorError> {
    let mut serialized =
        serde_json::to_string_pretty(value).map_err(|e| GraphtorError::Config {
            message: format!("failed to serialize {rel_path}: {e}"),
            field: None,
        })?;
    serialized.push('\n');

    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    let base_name = dest
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("mcp-config");
    let pid = std::process::id();

    // Create the temp file with EXCLUSIVE creation (`create_new` -> O_EXCL /
    // CREATE_NEW). The temp path is derived from the PID and is therefore
    // predictable, so a pre-planted symlink or a stale temp file left at that
    // path could otherwise be FOLLOWED by a plain `fs::write`, redirecting the
    // write OUTSIDE the workspace (containment escape) or letting a colliding
    // concurrent writer share our temp file. `create_new` fails closed on any
    // pre-existing path (including a symlink), so it never follows one; on
    // collision we retry with an incrementing suffix and write through the
    // returned handle.
    let mut opened: Option<(PathBuf, fs::File)> = None;
    for attempt in 0..256u32 {
        let candidate = parent.join(format!(".{base_name}.tmp-{pid}-{attempt}"));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                opened = Some((candidate, file));
                break;
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            Err(e) => {
                return Err(GraphtorError::Config {
                    message: format!("failed to create temporary file for {rel_path}: {e}"),
                    field: None,
                });
            }
        }
    }
    let Some((tmp_path, mut file)) = opened else {
        return Err(GraphtorError::Config {
            message: format!("failed to create a unique temporary file for {rel_path}"),
            field: None,
        });
    };
    if let Err(e) = file.write_all(serialized.as_bytes()) {
        let _ = fs::remove_file(&tmp_path);
        return Err(GraphtorError::Config {
            message: format!("failed to write temporary file for {rel_path}: {e}"),
            field: None,
        });
    }
    // Close the handle before the permission set / rename below: Windows cannot
    // rename a file that is still open.
    drop(file);
    // Preserve the destination's existing permissions across the atomic
    // replace. The temp file was created fresh with umask-default permissions;
    // without this, a user-owned `0600` shared `.mcp.json` (which may hold
    // credentials for OTHER MCP servers) would be widened to `0644` after the
    // rename, exposing its contents. On Windows `Permissions` only tracks the
    // readonly bit, so this call is harmless there.
    //
    // Fail CLOSED: if the destination's permissions cannot be reapplied to the
    // temp file, abort the write (removing the temp file) rather than renaming
    // a broader-perm temp over a restrictive config and silently widening it —
    // the exact credential-exposure case this block exists to prevent.
    if let Ok(meta) = fs::metadata(dest) {
        if let Err(e) = fs::set_permissions(&tmp_path, meta.permissions()) {
            let _ = fs::remove_file(&tmp_path);
            return Err(GraphtorError::Config {
                message: format!("failed to preserve destination permissions for {rel_path}: {e}"),
                field: None,
            });
        }
    }
    fs::rename(&tmp_path, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp_path);
        GraphtorError::Config {
            message: format!("failed to atomically write {rel_path}: {e}"),
            field: None,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test-only convenience mirroring the old full-scan `remove_mcp_config`:
    /// prune the managed entry across the ENTIRE managed candidate set. Real
    /// callers (uninstall) instead enumerate the candidates for approval and
    /// replay that exact list through [`remove_mcp_config_from`].
    fn remove_mcp_config(project_root: &Path) -> Result<Vec<McpConfigOutcome>, GraphtorError> {
        remove_mcp_config_from(project_root, &managed_config_candidates())
    }

    /// A shared config document holding an UNMARKED legacy-shape managed
    /// entry (the exact shape the pre-P2-T3 writer produced). Used both to
    /// characterize backward-compat migration and to prove the removal
    /// fallback still recognizes pre-marker entries.
    const MANAGED_DOC: &str = r#"{
  "mcpServers": {
    "graphtor-docs": {
      "command": ".graphtor/bin/graphtor-docs",
      "args": ["serve"],
      "transport": "stdio"
    }
  }
}
"#;

    /// A shared config document holding an ALREADY-MARKED managed entry
    /// (post-migration shape) whose command is the bare PATH command — the
    /// value a fresh `generate_mcp_config` call would ALSO resolve to in a
    /// tempdir with no managed binary installed, so re-running it is a
    /// genuine, content-verified no-op.
    const MARKED_DOC: &str = r#"{
  "mcpServers": {
    "graphtor-docs": {
      "command": "graphtor-docs",
      "args": ["serve"],
      "transport": "stdio",
      "x-graphtor-managed": true
    }
  }
}
"#;

    #[test]
    fn generate_creates_root_mcp_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let outcome = generate_mcp_config(tmp.path())
            .expect("generate")
            .expect("outcome");
        assert_eq!(outcome.action, McpConfigAction::Created);
        assert_eq!(outcome.path, ".mcp.json");
        let content = fs::read_to_string(tmp.path().join(".mcp.json")).expect("read");
        let parsed: serde_json::Value = serde_json::from_str(&content).expect("valid json");
        let entry = &parsed["mcpServers"]["graphtor-docs"];
        assert_eq!(
            entry["command"], "graphtor-docs",
            "no managed binary exists in a fresh tempdir, so the bare PATH command is used"
        );
        assert_eq!(
            entry[MANAGED_MARKER_KEY], true,
            "entry must carry the provenance marker"
        );
    }

    #[test]
    fn generate_uses_absolute_path_when_managed_binary_exists() {
        let tmp = tempfile::tempdir().expect("tempdir");
        crate::workspace::install::install(tmp.path()).expect("install managed binary");

        let outcome = generate_mcp_config(tmp.path())
            .expect("generate")
            .expect("outcome");
        assert_eq!(outcome.action, McpConfigAction::Created);

        let content = fs::read_to_string(tmp.path().join(".mcp.json")).expect("read");
        let parsed: serde_json::Value = serde_json::from_str(&content).expect("valid json");
        let command = parsed["mcpServers"]["graphtor-docs"]["command"]
            .as_str()
            .expect("command string");

        let expected_binary = crate::workspace::install::installed_binary_path(
            &tmp.path().join(crate::workspace::paths::GRAPHTOR_DIR),
        );
        // Use the same public canonicalization helper production code uses
        // (`validate_path`), not raw `std::fs::canonicalize` directly: on
        // Windows the latter returns the verbatim `\\?\`-prefixed form,
        // which `validate_path` deliberately strips for downstream
        // comparisons — comparing against the verbatim form here would be
        // an apples-to-oranges mismatch, not a real production bug.
        let canonical_expected = graphtor_core::path::validate_path(&expected_binary, tmp.path())
            .expect("validate_path");
        assert_eq!(
            Path::new(command),
            canonical_expected,
            "when a managed binary exists, the command must be its absolute canonical path"
        );
        assert!(
            Path::new(command).is_absolute(),
            "command must be absolute so it resolves regardless of the MCP client's launch cwd"
        );
    }

    #[test]
    fn generate_is_idempotent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        generate_mcp_config(tmp.path()).expect("first");
        let second = generate_mcp_config(tmp.path()).expect("second");
        assert!(second.is_none(), "second run should be a no-op");
    }

    #[test]
    fn generate_is_noop_when_marked_entry_already_matches_ladder() {
        // Case 2 (marked): the ladder resolves to the bare command in a
        // fresh tempdir with no managed binary — identical to MARKED_DOC's
        // existing value — so this must be a genuine, content-verified
        // no-op, not merely "already registered by key name".
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        fs::write(&path, MARKED_DOC).expect("write");
        let before = fs::read_to_string(&path).expect("read before");

        let outcome = generate_mcp_config(tmp.path()).expect("generate");

        assert!(outcome.is_none(), "matching marked entry is a no-op");
        let after = fs::read_to_string(&path).expect("read after");
        assert_eq!(
            after, before,
            "file must be byte-for-byte unchanged on a no-op"
        );
    }

    #[test]
    fn generate_does_not_write_editor_configs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        generate_mcp_config(tmp.path()).expect("generate");
        assert!(!tmp.path().join(".vscode/mcp.json").exists());
        assert!(!tmp.path().join(".cursor/mcp.json").exists());
        assert!(!tmp.path().join(".github/copilot/mcp.json").exists());
    }

    #[test]
    fn generate_merges_into_existing_shared_config() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        fs::write(
            &path,
            "{\n  \"mcpServers\": {\n    \"engram\": { \"command\": \"engram\" }\n  }\n}\n",
        )
        .expect("write");

        let outcome = generate_mcp_config(tmp.path())
            .expect("generate")
            .expect("outcome");

        assert_eq!(outcome.action, McpConfigAction::Updated);
        let after = fs::read_to_string(&path).expect("read");
        assert!(after.contains("engram"), "existing server preserved");
        let parsed: serde_json::Value = serde_json::from_str(&after).expect("valid json");
        assert_eq!(
            parsed["mcpServers"]["graphtor-docs"]["command"],
            "graphtor-docs"
        );
        assert_eq!(
            parsed["mcpServers"]["graphtor-docs"][MANAGED_MARKER_KEY],
            true
        );
        // preserve_order keeps the pre-existing server first.
        assert!(after.find("engram") < after.find("graphtor-docs"));
    }

    #[test]
    fn generate_migrates_unmarked_legacy_entry_in_place() {
        // Case 3: an unmarked entry that EXACTLY matches the historical
        // pre-marker shape is the current release's OWN legacy entry, not a
        // user collision — it is migrated in place (marker added, command
        // refreshed via the ladder), and this does NOT fail.
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        let shared = r#"{
  "mcpServers": {
    "engram": { "command": "engram", "args": ["shim"] },
    "graphtor-docs": {
      "command": ".graphtor/bin/graphtor-docs",
      "args": ["serve"],
      "transport": "stdio"
    }
  }
}
"#;
        fs::write(&path, shared).expect("write");

        let outcome = generate_mcp_config(tmp.path())
            .expect("legacy migration must not fail")
            .expect("outcome");

        assert_eq!(outcome.action, McpConfigAction::Updated);
        let after = fs::read_to_string(&path).expect("read");
        assert!(
            after.contains("engram"),
            "unrelated server preserved through migration"
        );
        let parsed: serde_json::Value = serde_json::from_str(&after).expect("valid json");
        let entry = &parsed["mcpServers"]["graphtor-docs"];
        assert_eq!(
            entry[MANAGED_MARKER_KEY], true,
            "migration must add the provenance marker"
        );
        assert_eq!(
            entry["command"], "graphtor-docs",
            "migration must refresh the command via the resolution ladder"
        );
    }

    #[test]
    fn generate_fails_closed_on_unmarked_user_collision() {
        // Case 4: an unmarked entry with a completely different command is a
        // genuine user collision — installation fails closed and the file
        // is preserved byte-for-byte.
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        let user_doc = r#"{
  "mcpServers": {
    "graphtor-docs": {
      "command": "my-custom-graphtor",
      "args": ["run", "--custom"]
    }
  }
}
"#;
        fs::write(&path, user_doc).expect("write");
        let before = fs::read_to_string(&path).expect("read before");

        let result = generate_mcp_config(tmp.path());

        assert!(result.is_err(), "unmarked user collision must fail closed");
        let after = fs::read_to_string(&path).expect("read after");
        assert_eq!(
            after, before,
            "user's file must be byte-for-byte unchanged on collision"
        );
    }

    #[test]
    fn generate_fails_closed_on_pinned_command_with_different_args() {
        // Case 4b: even when the command string matches the legacy shape
        // exactly, DIFFERENT args or transport is still a genuine collision
        // (not the current release's own shape), so it must also fail
        // closed rather than being (mis)treated as a legacy migration.
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        let user_doc = r#"{
  "mcpServers": {
    "graphtor-docs": {
      "command": ".graphtor/bin/graphtor-docs",
      "args": ["serve", "--verbose"],
      "transport": "stdio"
    }
  }
}
"#;
        fs::write(&path, user_doc).expect("write");
        let before = fs::read_to_string(&path).expect("read before");

        let result = generate_mcp_config(tmp.path());

        assert!(
            result.is_err(),
            "pinned command with different args must fail closed, not migrate"
        );
        let after = fs::read_to_string(&path).expect("read after");
        assert_eq!(
            after, before,
            "user's file must be byte-for-byte unchanged on collision"
        );
    }

    #[test]
    fn generate_write_leaves_no_stray_temp_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        generate_mcp_config(tmp.path()).expect("generate");
        let stray: Vec<_> = fs::read_dir(tmp.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.contains(".tmp-"))
            })
            .collect();
        assert!(
            stray.is_empty(),
            "atomic write must not leave a stray temp file behind: {stray:?}"
        );
    }

    #[test]
    fn generate_leaves_invalid_json_untouched() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        fs::write(&path, "not json at all").expect("write");
        let outcome = generate_mcp_config(tmp.path()).expect("generate");
        assert!(outcome.is_none());
        assert_eq!(fs::read_to_string(&path).expect("read"), "not json at all");
    }

    #[test]
    fn remove_removes_root_mcp_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        generate_mcp_config(tmp.path()).expect("generate");
        let outcomes = remove_mcp_config(tmp.path()).expect("remove");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].path, ".mcp.json");
        assert_eq!(outcomes[0].action, McpConfigAction::Removed);
        assert!(!tmp.path().join(".mcp.json").exists());
    }

    #[test]
    fn remove_removes_legacy_editor_configs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        for legacy in [
            ".vscode/mcp.json",
            ".cursor/mcp.json",
            ".github/copilot/mcp.json",
        ] {
            let path = tmp.path().join(legacy);
            fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            fs::write(&path, MANAGED_DOC).expect("write");
        }

        let outcomes = remove_mcp_config(tmp.path()).expect("remove");

        let removed: Vec<&str> = outcomes
            .iter()
            .filter(|o| o.action == McpConfigAction::Removed)
            .map(|o| o.path.as_str())
            .collect();
        assert_eq!(
            removed,
            vec![
                ".vscode/mcp.json",
                ".cursor/mcp.json",
                ".github/copilot/mcp.json"
            ]
        );
        assert!(!tmp.path().join(".vscode/mcp.json").exists());
        assert!(!tmp.path().join(".cursor/mcp.json").exists());
        assert!(!tmp.path().join(".github/copilot/mcp.json").exists());
    }

    #[test]
    fn remove_ignores_unmanaged_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        fs::write(&path, "{\"mcpServers\": {}}").expect("write");
        let outcomes = remove_mcp_config(tmp.path()).expect("remove");
        assert!(
            outcomes.is_empty(),
            "unmanaged file should be left in place"
        );
        assert!(path.exists());
    }

    #[test]
    fn remove_preserves_shared_mcp_json_without_managed_server() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        // Shared config with unrelated servers and an incidental "graphtor-docs"
        // occurrence in a workspace path — must NOT be deleted.
        let shared = r#"{
  "mcpServers": {
    "engram": { "command": "engram", "args": ["shim"] },
    "backlogit": {
      "command": "backlogit",
      "args": ["mcp"],
      "env": { "BACKLOGIT_WORKSPACE": "/home/user/Source/graphtor-docs" }
    }
  }
}
"#;
        fs::write(&path, shared).expect("write");

        let outcomes = remove_mcp_config(tmp.path()).expect("remove");

        assert!(outcomes.is_empty(), "shared config should be untouched");
        let after = fs::read_to_string(&path).expect("read");
        assert!(after.contains("engram"));
        assert!(after.contains("backlogit"));
    }

    #[test]
    fn remove_prunes_only_managed_entry_from_shared_mcp_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        let shared = r#"{
  "mcpServers": {
    "engram": { "command": "engram", "args": ["shim"] },
    "graphtor-docs": {
      "command": ".graphtor/bin/graphtor-docs",
      "args": ["serve"],
      "transport": "stdio"
    }
  }
}
"#;
        fs::write(&path, shared).expect("write");

        let outcomes = remove_mcp_config(tmp.path()).expect("remove");

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].path, ".mcp.json");
        assert_eq!(
            outcomes[0].action,
            McpConfigAction::Updated,
            "in-place prune is Updated, not Removed"
        );
        assert!(path.exists(), "file with other servers must be kept");
        let after = fs::read_to_string(&path).expect("read");
        assert!(after.contains("engram"), "unrelated server preserved");
        assert!(
            !after.contains(".graphtor/bin/graphtor-docs"),
            "managed entry pruned"
        );
    }

    #[test]
    fn remove_prunes_only_the_managed_key_not_a_user_copy_under_another_key() {
        // Copilot mcp_config.rs:389: pruning must be restricted to the fixed
        // MCP_SERVER_KEY. A user-authored copy under a DIFFERENT key that
        // happens to carry the managed marker (or the legacy shape) must be
        // preserved — only `graphtor-docs` is ever pruned.
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        let shared = r#"{
  "mcpServers": {
    "graphtor-docs": {
      "command": "graphtor-docs",
      "args": ["serve"],
      "transport": "stdio",
      "x-graphtor-managed": true
    },
    "my-graphtor-docs": {
      "command": ".graphtor/bin/graphtor-docs",
      "args": ["serve"],
      "transport": "stdio",
      "x-graphtor-managed": true
    }
  }
}
"#;
        fs::write(&path, shared).expect("write");

        let outcomes = remove_mcp_config(tmp.path()).expect("remove");

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].action, McpConfigAction::Updated);
        assert!(path.exists(), "file with the user copy must be kept");
        let parsed: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("read")).expect("parse");
        let servers = parsed["mcpServers"].as_object().expect("mcpServers object");
        assert!(
            !servers.contains_key("graphtor-docs"),
            "the managed graphtor-docs key must be pruned"
        );
        assert!(
            servers.contains_key("my-graphtor-docs"),
            "a user copy under another key must be preserved even when it carries the marker"
        );
    }

    #[test]
    fn remove_mcp_config_from_only_prunes_listed_files() {
        // Fix 5 support: execution must touch ONLY the approved list. A managed
        // config file NOT in the list is left untouched, even though a full
        // rescan would have pruned it.
        let tmp = tempfile::tempdir().expect("tempdir");
        let listed = tmp.path().join(".mcp.json");
        let unlisted_dir = tmp.path().join(".vscode");
        fs::create_dir_all(&unlisted_dir).expect("mkdir .vscode");
        let unlisted = unlisted_dir.join("mcp.json");
        fs::write(&listed, MARKED_DOC).expect("write listed");
        fs::write(&unlisted, MARKED_DOC).expect("write unlisted");

        let outcomes =
            remove_mcp_config_from(tmp.path(), &[".mcp.json".to_string()]).expect("remove listed");

        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].path, ".mcp.json");
        assert!(!listed.exists(), "the listed managed file must be pruned");
        assert!(
            unlisted.exists(),
            "a managed file NOT in the approved list must never be touched"
        );
    }

    // ── P2-T5b: exact-match legacy removal (never CONTAINS) ─────────────────

    #[test]
    fn remove_preserves_unmarked_entry_with_prefixed_legacy_command() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        let shared = r#"{
  "mcpServers": {
    "graphtor-docs": {
      "command": "/opt/tools/.graphtor/bin/graphtor-docs",
      "args": ["serve"],
      "transport": "stdio"
    }
  }
}
"#;
        fs::write(&path, shared).expect("write");

        let outcomes = remove_mcp_config(tmp.path()).expect("remove");

        assert!(
            outcomes.is_empty(),
            "a user command with a PREFIX around the legacy path must survive (exact-match \
             only, never contains)"
        );
        let after = fs::read_to_string(&path).expect("read");
        assert!(after.contains("/opt/tools/.graphtor/bin/graphtor-docs"));
    }

    #[test]
    fn remove_preserves_unmarked_entry_with_suffixed_legacy_command() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        let shared = r#"{
  "mcpServers": {
    "graphtor-docs": {
      "command": ".graphtor/bin/graphtor-docs-wrapper",
      "args": ["serve"],
      "transport": "stdio"
    }
  }
}
"#;
        fs::write(&path, shared).expect("write");

        let outcomes = remove_mcp_config(tmp.path()).expect("remove");

        assert!(
            outcomes.is_empty(),
            "a user command with a SUFFIX around the legacy path must survive"
        );
    }

    #[test]
    fn remove_preserves_unmarked_entry_with_pinned_command_but_different_args() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        let shared = r#"{
  "mcpServers": {
    "graphtor-docs": {
      "command": ".graphtor/bin/graphtor-docs",
      "args": ["serve", "--verbose"],
      "transport": "stdio"
    }
  }
}
"#;
        fs::write(&path, shared).expect("write");

        let outcomes = remove_mcp_config(tmp.path()).expect("remove");

        assert!(
            outcomes.is_empty(),
            "an unmarked entry with the pinned legacy command but DIFFERENT args must survive"
        );
    }

    #[test]
    fn remove_preserves_unmarked_entry_with_legacy_fields_plus_extra_key() {
        // An unmarked user entry that carries the three historical fields
        // (command/args/transport) PLUS an extra key (`env`) is a genuine
        // collision to preserve — NOT this release's exact pre-marker shape.
        // The exact-key-count guard must classify it as a collision so
        // uninstall does not silently remove a user-authored entry (data loss).
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        let shared = r#"{
  "mcpServers": {
    "graphtor-docs": {
      "command": ".graphtor/bin/graphtor-docs",
      "args": ["serve"],
      "transport": "stdio",
      "env": { "RUST_LOG": "debug" }
    }
  }
}
"#;
        fs::write(&path, shared).expect("write");

        let outcomes = remove_mcp_config(tmp.path()).expect("remove");

        assert!(
            outcomes.is_empty(),
            "an unmarked entry with the legacy fields PLUS an extra key must survive (exact key \
             set required, not a subset match)"
        );
        let after = fs::read_to_string(&path).expect("read");
        assert!(
            after.contains("RUST_LOG"),
            "the user-authored entry (with its extra env key) must be preserved intact"
        );
    }

    #[test]
    fn remove_removes_unmarked_exact_legacy_entry_windows_exe_shape() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        let shared = r#"{
  "mcpServers": {
    "graphtor-docs": {
      "command": ".graphtor/bin/graphtor-docs.exe",
      "args": ["serve"],
      "transport": "stdio"
    }
  }
}
"#;
        fs::write(&path, shared).expect("write");

        let outcomes = remove_mcp_config(tmp.path()).expect("remove");

        assert_eq!(
            outcomes.len(),
            1,
            "the exact Windows .exe legacy shape must still be recognized and removed"
        );
        assert_eq!(outcomes[0].action, McpConfigAction::Removed);
    }

    #[test]
    fn remove_prune_rewrite_is_atomic_leaves_no_stray_temp_file() {
        // The in-place prune rewrite (a managed entry removed, other servers
        // kept) must go through the SAME atomic temp-file + rename path as
        // `generate_mcp_config`, not a direct `fs::write`.
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        let shared = r#"{
  "mcpServers": {
    "engram": { "command": "engram", "args": ["shim"] },
    "graphtor-docs": {
      "command": ".graphtor/bin/graphtor-docs",
      "args": ["serve"],
      "transport": "stdio"
    }
  }
}
"#;
        fs::write(&path, shared).expect("write");

        let outcomes = remove_mcp_config(tmp.path()).expect("remove");
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].action, McpConfigAction::Updated);

        let stray: Vec<_> = fs::read_dir(tmp.path())
            .expect("read_dir")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.contains(".tmp-"))
            })
            .collect();
        assert!(
            stray.is_empty(),
            "prune rewrite must be atomic (temp-file + rename), leaving no stray temp file: \
             {stray:?}"
        );
        let after = fs::read_to_string(&path).expect("read after prune");
        assert!(
            after.contains("engram"),
            "unrelated server preserved: {after}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn write_json_preserves_destination_file_mode_across_rewrite() {
        // Copilot mcp_config.rs:472: a pre-existing `.mcp.json` may be a
        // user-owned 0600 file holding credentials for OTHER MCP servers. The
        // temp-file + rename rewrite must not widen it to umask-default 0644.
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join(".mcp.json");
        // A pre-existing shared config with an unrelated server.
        let shared = r#"{
  "mcpServers": {
    "secret-server": { "command": "secret", "args": ["--token", "hunter2"] }
  }
}
"#;
        fs::write(&path, shared).expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("chmod 0600");

        // Merge graphtor-docs in, exercising the temp-file + rename write path.
        generate_mcp_config(tmp.path()).expect("generate merges into existing config");

        let mode = fs::metadata(&path).expect("metadata").permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "the rewrite must preserve the destination's original 0600 mode, not widen to 0644"
        );
    }

    // ── workspace containment: symlinked config parent (X3) ─────────────────

    /// Create a directory symlink cross-platform, returning `Err` when the
    /// platform refuses (e.g. Windows without the symlink privilege) so the
    /// caller can self-skip rather than fail.
    fn try_symlink_dir(target: &Path, link: &Path) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(target, link)
        }
    }

    #[test]
    fn remove_skips_a_candidate_whose_parent_is_a_symlink_out_of_project() {
        // A legacy candidate `.vscode/mcp.json` whose `.vscode` parent is a
        // symlink/junction pointing OUTSIDE the project must never be read or
        // mutated — the remove branch would otherwise delete the external file
        // (MANAGED_DOC holds only the managed server, so prune => RemoveFile).
        let project = tempfile::tempdir().expect("project tempdir");
        let external = tempfile::tempdir().expect("external tempdir");
        let external_cfg = external.path().join("mcp.json");
        fs::write(&external_cfg, MANAGED_DOC).expect("write external mcp.json");

        let vscode_link = project.path().join(".vscode");
        if try_symlink_dir(external.path(), &vscode_link).is_err() {
            return; // platform refused symlink creation — skip
        }

        let outcomes = remove_mcp_config_from(project.path(), &[".vscode/mcp.json".to_string()])
            .expect("remove_mcp_config_from");

        assert!(
            outcomes.is_empty(),
            "a candidate escaping the project via a symlinked parent must be skipped"
        );
        assert!(
            external_cfg.exists(),
            "the external config behind a symlinked parent must never be deleted"
        );
        assert_eq!(
            fs::read_to_string(&external_cfg).expect("read external"),
            MANAGED_DOC,
            "the external config must be left byte-for-byte unchanged"
        );
    }

    #[test]
    fn plan_predicate_skips_a_candidate_whose_parent_is_a_symlink_out_of_project() {
        // W6-1: the uninstall PLANNING predicate `file_has_managed_entry` must
        // apply the same workspace-containment guard the remove branch does. A
        // legacy candidate `.vscode/mcp.json` reached through a symlinked
        // `.vscode` parent pointing OUTSIDE the project must not be read during
        // planning — otherwise the plan would preview a prune of an external
        // file that execution then (correctly) skips. The predicate reports
        // `false` for the escaping candidate even though the external file
        // genuinely holds a managed entry.
        let project = tempfile::tempdir().expect("project tempdir");
        let external = tempfile::tempdir().expect("external tempdir");
        fs::write(external.path().join("mcp.json"), MANAGED_DOC).expect("write external mcp.json");

        // Isolation proof: read in-root of the external dir → detected managed.
        assert!(
            file_has_managed_entry(external.path(), "mcp.json"),
            "precondition: the external file must genuinely hold a managed entry"
        );

        let vscode_link = project.path().join(".vscode");
        if try_symlink_dir(external.path(), &vscode_link).is_err() {
            return; // platform refused symlink creation — skip
        }

        assert!(
            !file_has_managed_entry(project.path(), ".vscode/mcp.json"),
            "a candidate reached through a symlinked parent must not be read or \
             detected as managed during planning"
        );
    }

    /// Create a file symlink cross-platform, returning `Err` when the platform
    /// refuses so the caller can self-skip rather than fail.
    fn try_symlink_file(target: &Path, link: &Path) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_file(target, link)
        }
    }

    #[test]
    fn write_json_never_follows_a_symlink_planted_at_the_predictable_temp_path() {
        // W6-2: the temp path is PID-derived and therefore predictable. If a
        // symlink is planted there (by an attacker or a stale run) pointing at
        // an external file, a plain `fs::write` to the temp path would FOLLOW it
        // and clobber the external target. Exclusive creation (`create_new` /
        // O_EXCL) refuses to open any pre-existing path — including a symlink —
        // so the write is redirected to a fresh suffix and the external target
        // is never touched.
        let project = tempfile::tempdir().expect("project tempdir");
        let external = tempfile::tempdir().expect("external tempdir");
        let victim = external.path().join("victim.txt");
        fs::write(&victim, b"external data that must survive").expect("seed victim");

        let dest = project.path().join(".mcp.json");
        let pid = std::process::id();
        // Plant a symlink at BOTH the current first-candidate temp path (`-0`)
        // and the historical single-temp path (no suffix). The current writer
        // must collide on `-0` and retry; a regression to the old
        // symlink-following `fs::write` would clobber the victim through the
        // no-suffix path and fail this test.
        let planted_new = project
            .path()
            .join(format!(".{}.tmp-{}-0", ".mcp.json", pid));
        let planted_old = project.path().join(format!(".{}.tmp-{}", ".mcp.json", pid));
        if try_symlink_file(&victim, &planted_new).is_err()
            || try_symlink_file(&victim, &planted_old).is_err()
        {
            return; // platform refused symlink creation — skip
        }

        let value = serde_json::json!({ "mcpServers": {} });
        write_json(&dest, &value, ".mcp.json")
            .expect("write_json must succeed by selecting a fresh, uncontended temp path");

        assert_eq!(
            fs::read_to_string(&victim).expect("victim still readable"),
            "external data that must survive",
            "write_json must not follow a planted symlink and overwrite an external file"
        );
        assert!(
            fs::symlink_metadata(&planted_new)
                .expect("planted symlink metadata")
                .file_type()
                .is_symlink(),
            "the planted temp-path symlink must be left intact (create_new refused it), \
             proving it was neither followed nor replaced"
        );
        assert!(
            dest.exists(),
            "the destination config must still be written"
        );
        assert!(
            fs::read_to_string(&dest)
                .expect("read dest")
                .contains("mcpServers"),
            "the destination must hold the intended serialized content"
        );
    }
}
