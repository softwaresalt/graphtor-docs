//! Workspace upgrade workflow.
//!
//! Replaces the installed binary in `.graphtor/bin/` with the currently
//! running binary. Preserves config and data directories.

use std::fs;
use std::path::Path;

use sha2::{Digest as _, Sha256};

use crate::workspace::install::installed_binary_path;
use graphtor_core::GraphtorError;

/// Compute the SHA-256 digest of a file's contents.
///
/// Returns `None` when the file cannot be read.
fn file_sha256(path: &Path) -> Option<Vec<u8>> {
    let data = fs::read(path).ok()?;
    Some(Sha256::digest(&data).to_vec())
}

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
/// When `force` is `false`, computes the SHA-256 hash of both the running
/// and installed binaries and skips the copy when they match.  Pass
/// `force = true` to always replace regardless of content.
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
        let src_hash = file_sha256(&exe);
        let dst_hash = file_sha256(&dest);
        if src_hash.is_some() && src_hash == dst_hash {
            return Ok(UpgradeResult {
                upgraded: false,
                message: "binary is already up-to-date".to_string(),
            });
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
