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

/// Generate the workspace-root `.mcp.json` MCP client config.
///
/// Writes a `.mcp.json` file at the project root that registers
/// `graphtor-docs` as an MCP server. If the config file already exists it is
/// left unchanged (idempotent). Returns the relative paths written — empty
/// when the file already existed.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] on I/O failure.
pub fn generate_mcp_config(project_root: &Path) -> Result<Vec<String>, GraphtorError> {
    let dest = project_root.join(MCP_CONFIG_PATH);

    if dest.exists() {
        // Already configured; leave it unchanged.
        return Ok(Vec::new());
    }

    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| GraphtorError::Config {
            message: format!("failed to create {}: {e}", parent.display()),
            field: None,
        })?;
    }

    let binary_path = format!(".graphtor/bin/graphtor-docs{}", binary_ext());
    let config = mcp_config_json(&binary_path);
    fs::write(&dest, config).map_err(|e| GraphtorError::Config {
        message: format!("failed to write {MCP_CONFIG_PATH}: {e}"),
        field: None,
    })?;

    Ok(vec![MCP_CONFIG_PATH.to_string()])
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
/// Returns the relative paths that were modified or deleted.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] on I/O failure.
pub fn remove_mcp_config(project_root: &Path) -> Result<Vec<String>, GraphtorError> {
    let mut removed: Vec<String> = Vec::new();
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
                removed.push(rel_path.to_string());
            }
            PruneOutcome::Rewrite(new_content) => {
                fs::write(&dest, new_content).map_err(|e| GraphtorError::Config {
                    message: format!("failed to rewrite {rel_path}: {e}"),
                    field: None,
                })?;
                removed.push(rel_path.to_string());
            }
        }
    }
    Ok(removed)
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
        servers.remove(key);
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

/// Render the MCP client JSON config for the given binary path.
fn mcp_config_json(binary_path: &str) -> String {
    // Standard MCP server registration format understood by VS Code and Cursor.
    format!(
        r#"{{
  "mcpServers": {{
    "graphtor-docs": {{
      "command": "{binary_path}",
      "args": ["serve"],
      "transport": "stdio"
    }}
  }}
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_creates_root_mcp_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let written = generate_mcp_config(tmp.path()).expect("generate");
        assert_eq!(written, vec![".mcp.json".to_string()]);
        let content = fs::read_to_string(tmp.path().join(".mcp.json")).expect("read");
        assert!(content.contains("graphtor-docs"));
    }

    #[test]
    fn generate_is_idempotent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        generate_mcp_config(tmp.path()).expect("first");
        let second = generate_mcp_config(tmp.path()).expect("second");
        assert!(second.is_empty(), "second run should produce no writes");
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
    fn remove_removes_root_mcp_json() {
        let tmp = tempfile::tempdir().expect("tempdir");
        generate_mcp_config(tmp.path()).expect("generate");
        let removed = remove_mcp_config(tmp.path()).expect("remove");
        assert_eq!(removed, vec![".mcp.json".to_string()]);
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
            fs::write(&path, mcp_config_json(".graphtor/bin/graphtor-docs")).expect("write");
        }

        let removed = remove_mcp_config(tmp.path()).expect("remove");

        assert_eq!(
            removed,
            vec![
                ".vscode/mcp.json".to_string(),
                ".cursor/mcp.json".to_string(),
                ".github/copilot/mcp.json".to_string(),
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
        let removed = remove_mcp_config(tmp.path()).expect("remove");
        assert!(removed.is_empty(), "unmanaged file should be left in place");
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

        let removed = remove_mcp_config(tmp.path()).expect("remove");

        assert!(removed.is_empty(), "shared config should be untouched");
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

        let removed = remove_mcp_config(tmp.path()).expect("remove");

        assert_eq!(removed, vec![".mcp.json".to_string()]);
        assert!(path.exists(), "file with other servers must be kept");
        let after = fs::read_to_string(&path).expect("read");
        assert!(after.contains("engram"), "unrelated server preserved");
        assert!(
            !after.contains(".graphtor/bin/graphtor-docs"),
            "managed entry pruned"
        );
    }
}
