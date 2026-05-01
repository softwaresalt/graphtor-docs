//! Workspace installation and `.graphtor/` directory scaffold.
//!
//! Creates the `.graphtor/` workspace directory structure under the
//! project root and copies the running binary into `.graphtor/bin/`.
//! The install operation is idempotent — running it on an already-installed
//! workspace is safe.

use std::fs;
use std::path::{Path, PathBuf};

use crate::workspace::paths::{GRAPHTOR_DIR, GRAPHTOR_SUBDIRS};
use graphtor_core::GraphtorError;

/// Result of a workspace install operation.
#[derive(Debug)]
pub struct InstallResult {
    /// Path to the created or pre-existing `.graphtor/` directory.
    pub workspace_dir: PathBuf,
    /// Whether the workspace was freshly created (`true`) or already existed (`false`).
    pub created: bool,
    /// Path to the installed binary.
    pub binary_path: PathBuf,
}

/// Install graphtor-docs into `project_root`.
///
/// 1. Creates `.graphtor/{bin,data,cache,config,logs}/` (idempotent).
/// 2. Copies the currently-executing binary to `.graphtor/bin/graphtor-docs[.exe]`.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] on I/O failure or when the running
/// executable path cannot be determined.
pub fn install(project_root: &Path) -> Result<InstallResult, GraphtorError> {
    let workspace_dir = project_root.join(GRAPHTOR_DIR);
    let already_existed = workspace_dir.exists();

    // Create subdirectory scaffold.
    for sub in GRAPHTOR_SUBDIRS {
        let dir = workspace_dir.join(sub);
        fs::create_dir_all(&dir).map_err(|e| GraphtorError::Config {
            message: format!("failed to create {}: {e}", dir.display()),
            field: None,
        })?;
    }

    // Resolve the path to the running binary.
    let exe = std::env::current_exe().map_err(|e| GraphtorError::Config {
        message: format!("failed to locate running binary: {e}"),
        field: None,
    })?;

    let bin_name = format!("graphtor-docs{}", if cfg!(windows) { ".exe" } else { "" });
    let dest = workspace_dir.join("bin").join(&bin_name);

    // Copy only when the source differs from the destination (avoid copying
    // on top of ourselves when the binary is already in .graphtor/bin/).
    let should_copy = if dest.exists() {
        // Use file size as a cheap heuristic; a full hash is not worth the I/O
        // cost on the install fast path.
        let src_len = exe.metadata().map_or(0, |m: std::fs::Metadata| m.len());
        let dst_len = dest.metadata().map_or(1, |m: std::fs::Metadata| m.len());
        src_len != dst_len
    } else {
        true
    };

    if should_copy {
        fs::copy(&exe, &dest).map_err(|e| GraphtorError::Config {
            message: format!("failed to copy binary to {}: {e}", dest.display()),
            field: None,
        })?;

        // Make executable on Unix.
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
    }

    Ok(InstallResult {
        workspace_dir,
        created: !already_existed,
        binary_path: dest,
    })
}

/// Return the path to the installed binary, if present.
pub fn installed_binary_path(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("bin").join(format!(
        "graphtor-docs{}",
        if cfg!(windows) { ".exe" } else { "" }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_creates_subdirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let result = install(tmp.path()).expect("install");
        for sub in GRAPHTOR_SUBDIRS {
            assert!(result.workspace_dir.join(sub).is_dir(), "missing {sub}");
        }
    }

    #[test]
    fn install_is_idempotent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("first");
        install(tmp.path()).expect("second");
        // Verify subdirs still exist after second run.
        let ws = tmp.path().join(GRAPHTOR_DIR);
        for sub in GRAPHTOR_SUBDIRS {
            assert!(ws.join(sub).is_dir());
        }
    }
}
