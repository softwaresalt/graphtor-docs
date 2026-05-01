//! Workspace upgrade workflow.
//!
//! Replaces the installed binary in `.graphtor/bin/` with the currently
//! running binary. Preserves config and data directories.

use std::fs;
use std::path::Path;

use crate::workspace::install::installed_binary_path;
use graphtor_core::GraphtorError;

/// Result of an upgrade operation.
#[derive(Debug)]
pub struct UpgradeResult {
    /// Whether the binary was replaced (`true`) or was already up-to-date (`false`).
    pub upgraded: bool,
    /// Human-readable status message.
    pub message: String,
}

/// Upgrade the installed binary in the workspace.
///
/// Copies the running binary over `.graphtor/bin/graphtor-docs[.exe]`.
///
/// When `force` is `false`, uses file size as a cheap heuristic to skip
/// the copy if the binary appears unchanged. Pass `force = true` to
/// always replace.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] on I/O failure or when the running
/// binary path cannot be determined.
pub fn upgrade(workspace_dir: &Path, force: bool) -> Result<UpgradeResult, GraphtorError> {
    let dest = installed_binary_path(workspace_dir);
    let exe = std::env::current_exe().map_err(|e| GraphtorError::Config {
        message: format!("failed to locate running binary: {e}"),
        field: None,
    })?;

    if !force && dest.exists() {
        let src_mtime = exe.metadata().and_then(|m| m.modified()).ok();
        let dst_mtime = dest.metadata().and_then(|m| m.modified()).ok();
        if let (Some(src_t), Some(dst_t)) = (src_mtime, dst_mtime) {
            if src_t == dst_t {
                return Ok(UpgradeResult {
                    upgraded: false,
                    message: "binary is already up-to-date".to_string(),
                });
            }
        }
    }

    fs::copy(&exe, &dest).map_err(|e| GraphtorError::Config {
        message: format!("failed to copy binary: {e}"),
        field: None,
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut perms = dest
            .metadata()
            .map_err(|e| GraphtorError::Config {
                message: e.to_string(),
                field: None,
            })?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms).map_err(|e| GraphtorError::Config {
            message: e.to_string(),
            field: None,
        })?;
    }

    Ok(UpgradeResult {
        upgraded: true,
        message: format!("binary upgraded: {}", dest.display()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::install::install;

    #[test]
    fn upgrade_succeeds_after_install() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        let ws = tmp.path().join(crate::workspace::paths::GRAPHTOR_DIR);
        let result = upgrade(&ws, true).expect("upgrade");
        assert!(result.upgraded);
    }
}
