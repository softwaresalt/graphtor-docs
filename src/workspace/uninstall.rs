//! Workspace uninstall workflow.
//!
//! Removes the `.graphtor/` directory and cleans up MCP client config files
//! and `.gitignore` entries. Requires explicit `--confirm` to prevent
//! accidental data loss.

use std::fs;
use std::path::Path;

use crate::workspace::gitignore::remove_gitignore_entry;
use crate::workspace::mcp_config::{remove_mcp_configs, Editor};
use crate::workspace::paths::GRAPHTOR_DIR;
use graphtor_core::GraphtorError;

/// Result of an uninstall operation.
#[derive(Debug)]
pub struct UninstallResult {
    /// Files and directories removed.
    pub removed: Vec<String>,
}

/// Uninstall graphtor-docs from the workspace.
///
/// Removes `.graphtor/` (optionally preserving the config sub-directory
/// when `keep_config` is `true`), cleans `.gitignore`, and removes MCP
/// client config files.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] when `project_root` cannot be
/// resolved or on I/O failure.
pub fn uninstall(project_root: &Path, keep_config: bool) -> Result<UninstallResult, GraphtorError> {
    let workspace_dir = project_root.join(GRAPHTOR_DIR);
    let mut removed: Vec<String> = Vec::new();

    if workspace_dir.exists() {
        if keep_config {
            // Remove everything except .graphtor/config/.
            let entries = fs::read_dir(&workspace_dir).map_err(|e| GraphtorError::Config {
                message: format!("failed to read workspace dir: {e}"),
                field: None,
            })?;
            for entry in entries.flatten() {
                let entry: std::fs::DirEntry = entry;
                let path = entry.path();
                let name = entry.file_name();
                if name == "config" {
                    continue;
                }
                if path.is_dir() {
                    fs::remove_dir_all(&path).map_err(|e| GraphtorError::Config {
                        message: format!("failed to remove {}: {e}", path.display()),
                        field: None,
                    })?;
                } else {
                    fs::remove_file(&path).map_err(|e| GraphtorError::Config {
                        message: format!("failed to remove {}: {e}", path.display()),
                        field: None,
                    })?;
                }
                removed.push(path.display().to_string());
            }
        } else {
            fs::remove_dir_all(&workspace_dir).map_err(|e| GraphtorError::Config {
                message: format!("failed to remove workspace dir: {e}"),
                field: None,
            })?;
            removed.push(workspace_dir.display().to_string());
        }
    }

    // Clean .gitignore.
    remove_gitignore_entry(project_root)?;

    // Remove MCP client configs.
    let mcp_removed = remove_mcp_configs(
        project_root,
        &[Editor::VsCode, Editor::Cursor, Editor::Copilot],
    )?;
    removed.extend(mcp_removed);

    Ok(UninstallResult { removed })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::gitignore::add_gitignore_entry;
    use crate::workspace::install::install;

    #[test]
    fn uninstall_removes_workspace_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        add_gitignore_entry(tmp.path()).expect("gitignore");
        uninstall(tmp.path(), false).expect("uninstall");
        assert!(!tmp.path().join(GRAPHTOR_DIR).exists());
    }

    #[test]
    fn uninstall_keep_config_preserves_config_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        // Create a config file.
        let config_dir = tmp.path().join(GRAPHTOR_DIR).join("config");
        fs::create_dir_all(&config_dir).expect("config dir");
        fs::write(config_dir.join("sources.yaml"), "sources: []").expect("write");
        uninstall(tmp.path(), true).expect("uninstall keep-config");
        assert!(
            config_dir.join("sources.yaml").exists(),
            "sources.yaml should be preserved"
        );
    }
}
