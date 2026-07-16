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
/// A consumption-first MINIMAL install (P2-T1) has no managed binary to
/// upgrade at all — `.graphtor/bin/` was never created — so this is a safe
/// no-op success (never an error, and never creates a `bin/` scaffold as a
/// side effect) regardless of `force`. A full install upgrades exactly as
/// before.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] on I/O failure or when the running
/// binary path cannot be determined.
pub fn upgrade(workspace_dir: &Path, force: bool) -> Result<UpgradeResult, GraphtorError> {
    // `detect_footprint` returns `Minimal` both for a genuine minimal
    // install AND for a workspace that was never installed at all (no
    // ingestion-capable subdirectory ever existing is exactly how it
    // detects "minimal" — see its doc comment). Without this check, running
    // `upgrade` before `install` would silently report a successful
    // minimal-install no-op instead of the actionable error an
    // uninstalled workspace deserves.
    if !workspace_dir.is_dir() {
        return Err(GraphtorError::Config {
            message: format!(
                "no {} directory found; run `graphtor-docs install` first",
                crate::workspace::paths::GRAPHTOR_DIR
            ),
            field: None,
        });
    }

    if crate::workspace::doctor::detect_footprint(workspace_dir)
        == crate::workspace::doctor::WorkspaceFootprint::Minimal
    {
        return Ok(UpgradeResult {
            upgraded: false,
            message: "consumption-first minimal install has no managed binary to upgrade \
                      (graphtor-docs resolves via PATH)"
                .to_string(),
        });
    }

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
    use crate::workspace::install::{install, install_minimal};

    #[test]
    fn upgrade_succeeds_after_install() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        let ws = tmp.path().join(crate::workspace::paths::GRAPHTOR_DIR);
        let result = upgrade(&ws, true).expect("upgrade");
        assert!(result.upgraded);
    }

    #[test]
    fn upgrade_on_minimal_install_is_a_safe_noop() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install_minimal(tmp.path()).expect("install_minimal");
        let ws = tmp.path().join(crate::workspace::paths::GRAPHTOR_DIR);

        let result = upgrade(&ws, false).expect("upgrade on a minimal install must not error");

        assert!(
            !result.upgraded,
            "a minimal install has no managed binary to upgrade"
        );
        assert!(
            !installed_binary_path(&ws).exists(),
            "upgrade must never create a bin/ scaffold on a minimal install"
        );
    }

    #[test]
    fn upgrade_on_minimal_install_force_is_still_a_safe_noop() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install_minimal(tmp.path()).expect("install_minimal");
        let ws = tmp.path().join(crate::workspace::paths::GRAPHTOR_DIR);

        let result =
            upgrade(&ws, true).expect("upgrade --force on a minimal install must not error");

        assert!(!result.upgraded);
        assert!(!installed_binary_path(&ws).exists());
    }

    #[test]
    fn upgrade_on_never_installed_workspace_is_an_actionable_error_not_a_silent_noop() {
        // A workspace where `.graphtor/` never existed at all must NOT be
        // confused with a genuine minimal install: `detect_footprint`
        // returns `Minimal` for both, so `upgrade` must check workspace
        // existence FIRST and fail with an actionable message instead of
        // reporting a misleading successful no-op.
        let tmp = tempfile::tempdir().expect("tempdir");
        let ws = tmp.path().join(crate::workspace::paths::GRAPHTOR_DIR);
        assert!(!ws.exists(), "precondition: workspace must not exist");

        let err = upgrade(&ws, false)
            .expect_err("upgrade on a never-installed workspace must be an error");
        let msg = err.to_string();
        assert!(
            msg.contains("install"),
            "error must point the operator at `graphtor-docs install`: {msg}"
        );
    }

    #[test]
    fn upgrade_on_minimal_install_is_idempotent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install_minimal(tmp.path()).expect("install_minimal");
        let ws = tmp.path().join(crate::workspace::paths::GRAPHTOR_DIR);

        upgrade(&ws, false).expect("first upgrade");
        let second = upgrade(&ws, false).expect("second upgrade");

        assert!(!second.upgraded);
        assert!(!installed_binary_path(&ws).exists());
    }

    #[test]
    fn upgrade_on_config_only_consumption_workspace_is_a_safe_noop() {
        // A consumption-only workspace with `config/sources.yaml` (declaring
        // only `type: database` sources) but no ingestion scaffold must be
        // treated as Minimal: upgrade must no-op instead of trying to copy the
        // running binary into a nonexistent `bin/` (which would error).
        let tmp = tempfile::tempdir().expect("tempdir");
        let ws = tmp.path().join(crate::workspace::paths::GRAPHTOR_DIR);
        let config_dir = ws.join("config");
        fs::create_dir_all(&config_dir).expect("create config dir");
        fs::write(
            config_dir.join("sources.yaml"),
            "sources:\n  - type: database\n    path: ./external.db\n",
        )
        .expect("write sources.yaml");

        let result =
            upgrade(&ws, false).expect("upgrade on a config-only workspace must not error");

        assert!(
            !result.upgraded,
            "a config-only consumption workspace has no managed binary to upgrade"
        );
        assert!(
            !installed_binary_path(&ws).exists(),
            "upgrade must never create a bin/ scaffold on a consumption workspace"
        );
    }
}
