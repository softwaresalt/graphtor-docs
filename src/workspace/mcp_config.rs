//! MCP client configuration generation.
//!
//! Generates a single workspace-root `.mcp.json` configuration file that
//! registers the installed `graphtor-docs` binary as an MCP server. This is
//! the editor-agnostic standard understood by MCP clients; graphtor-docs no
//! longer writes editor-specific config files.
//!
//! Uninstall is surgical: it parses each candidate config and removes only the
//! server entry graphtor-docs manages (identified by its `.graphtor/bin/`
//! binary command), preserving any other MCP servers in a shared `.mcp.json`.
//! A file is deleted outright only when the managed entry was its sole server.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use graphtor_core::GraphtorError;

/// Workspace-root MCP client config path (the current standard).
const MCP_CONFIG_PATH: &str = ".mcp.json";

/// Legacy editor-specific config paths cleaned up on uninstall.
const LEGACY_CONFIG_PATHS: &[&str] = &[
    ".vscode/mcp.json",
    ".cursor/mcp.json",
    ".github/copilot/mcp.json",
];

/// Command substring identifying a graphtor-docs-managed MCP server entry.
///
/// Keyed on the managed binary path rather than the bare project name so an
/// incidental `graphtor-docs` occurrence (e.g. a workspace path) in an
/// unrelated server does not cause its config to be treated as managed. This
/// prefix matches both the Unix (`graphtor-docs`) and Windows
/// (`graphtor-docs.exe`) generated commands.
const MANAGED_COMMAND_MARKER: &str = ".graphtor/bin/graphtor-docs";

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
/// When `.mcp.json` does not exist it is created with the `graphtor-docs`
/// server ([`McpConfigAction::Created`]). When it already exists and is a valid
/// JSON object, the `graphtor-docs` entry is merged into its `mcpServers` map,
/// preserving any other servers ([`McpConfigAction::Updated`]). Returns `None`
/// when nothing changed — the entry is already registered, or the existing file
/// is not a JSON object (in which case it is left untouched rather than
/// clobbered).
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] on I/O failure.
pub fn generate_mcp_config(project_root: &Path) -> Result<Option<McpConfigOutcome>, GraphtorError> {
    let dest = project_root.join(MCP_CONFIG_PATH);
    let binary_path = format!(".graphtor/bin/graphtor-docs{}", binary_ext());

    if !dest.exists() {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| GraphtorError::Config {
                message: format!("failed to create {}: {e}", parent.display()),
                field: None,
            })?;
        }
        let document = serde_json::json!({
            "mcpServers": { "graphtor-docs": managed_server_value(&binary_path) }
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
    let already_registered = servers.values().any(|cfg| {
        cfg.get("command")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|command| command.contains(MANAGED_COMMAND_MARKER))
    });
    if already_registered {
        return Ok(None);
    }
    servers.insert(
        "graphtor-docs".to_string(),
        managed_server_value(&binary_path),
    );
    write_json(&dest, &value, MCP_CONFIG_PATH)?;
    Ok(Some(McpConfigOutcome {
        path: MCP_CONFIG_PATH.to_string(),
        action: McpConfigAction::Updated,
    }))
}

/// Remove the graphtor-docs-managed MCP server from workspace configs.
///
/// Scans the workspace-root `.mcp.json` and legacy editor-specific config
/// files (`.vscode/mcp.json`, `.cursor/mcp.json`, `.github/copilot/mcp.json`).
/// For each, it removes only the server entry whose command references the
/// managed `.graphtor/bin/graphtor-docs` binary — other MCP servers in a
/// shared config are preserved. The file is deleted only when the managed
/// entry was its sole content; otherwise it is rewritten in place. Files that
/// are not valid JSON or contain no managed entry are left untouched.
///
/// Returns the per-file outcomes (deleted files vs in-place-pruned files).
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] on I/O failure.
pub fn remove_mcp_config(project_root: &Path) -> Result<Vec<McpConfigOutcome>, GraphtorError> {
    let mut outcomes: Vec<McpConfigOutcome> = Vec::new();
    for rel_path in std::iter::once(MCP_CONFIG_PATH).chain(LEGACY_CONFIG_PATHS.iter().copied()) {
        let dest = project_root.join(rel_path);
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
            PruneOutcome::Rewrite(new_content) => {
                fs::write(&dest, new_content).map_err(|e| GraphtorError::Config {
                    message: format!("failed to rewrite {rel_path}: {e}"),
                    field: None,
                })?;
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
    /// The managed entry was removed but other servers remain; rewrite content.
    Rewrite(String),
}

/// Remove graphtor-docs-managed server entries from an MCP config document.
///
/// A server is "managed" when its `command` references the
/// [`MANAGED_COMMAND_MARKER`] binary path. Non-JSON input and configs without a
/// managed entry yield [`PruneOutcome::Unchanged`], so shared configs holding
/// unrelated servers are never destroyed.
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
        .filter(|(_, cfg)| {
            cfg.get("command")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|command| command.contains(MANAGED_COMMAND_MARKER))
        })
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

    match serde_json::to_string_pretty(&value) {
        Ok(mut serialized) => {
            serialized.push('\n');
            PruneOutcome::Rewrite(serialized)
        }
        Err(_) => PruneOutcome::Unchanged,
    }
}

/// Return platform-specific binary extension (`.exe` on Windows, empty elsewhere).
fn binary_ext() -> &'static str {
    if cfg!(windows) {
        ".exe"
    } else {
        ""
    }
}

/// Build the graphtor-docs MCP server registration value.
fn managed_server_value(binary_path: &str) -> serde_json::Value {
    serde_json::json!({
        "command": binary_path,
        "args": ["serve"],
        "transport": "stdio"
    })
}

/// Serialize `value` as pretty JSON with a trailing newline and write it.
fn write_json(dest: &Path, value: &serde_json::Value, rel_path: &str) -> Result<(), GraphtorError> {
    let mut serialized =
        serde_json::to_string_pretty(value).map_err(|e| GraphtorError::Config {
            message: format!("failed to serialize {rel_path}: {e}"),
            field: None,
        })?;
    serialized.push('\n');
    fs::write(dest, serialized).map_err(|e| GraphtorError::Config {
        message: format!("failed to write {rel_path}: {e}"),
        field: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A shared config document holding a managed graphtor-docs entry.
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

    #[test]
    fn generate_creates_root_mcp_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let outcome = generate_mcp_config(tmp.path())
            .expect("generate")
            .expect("outcome");
        assert_eq!(outcome.action, McpConfigAction::Created);
        assert_eq!(outcome.path, ".mcp.json");
        let content = fs::read_to_string(tmp.path().join(".mcp.json")).expect("read");
        assert!(content.contains(".graphtor/bin/graphtor-docs"));
    }

    #[test]
    fn generate_is_idempotent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        generate_mcp_config(tmp.path()).expect("first");
        let second = generate_mcp_config(tmp.path()).expect("second");
        assert!(second.is_none(), "second run should be a no-op");
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
        assert!(after.contains(".graphtor/bin/graphtor-docs"), "entry added");
        // preserve_order keeps the pre-existing server first.
        assert!(after.find("engram") < after.find("graphtor-docs"));
    }

    #[test]
    fn generate_skips_when_already_registered() {
        let tmp = tempfile::tempdir().expect("tempdir");
        fs::write(tmp.path().join(".mcp.json"), MANAGED_DOC).expect("write");
        let outcome = generate_mcp_config(tmp.path()).expect("generate");
        assert!(outcome.is_none(), "already-registered config is a no-op");
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
}
