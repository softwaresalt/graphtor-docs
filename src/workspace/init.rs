//! Workspace `sources.yaml` initialization.
//!
//! Generates a template `sources.yaml` at `.graphtor/config/sources.yaml`
//! with commented examples for Git and local sources. Idempotent: will not
//! overwrite an existing file unless `force = true` is passed.

use std::fs;
use std::path::Path;

use graphtor_core::GraphtorError;

/// Default template content for a new `sources.yaml`.
const SOURCES_YAML_TEMPLATE: &str = r#"# graphtor-docs sources.yaml
# Documentation source registry. Add sources below to index them.
# Run `graphtor-docs sync` to ingest all sources.
#
# Supported source types:
#   git   — shallow-clone a Git repository
#   local — scan a local directory

sources: []

# Example Git source:
# sources:
#   - id: azure-docs
#     type: git
#     url: https://github.com/MicrosoftDocs/azure-docs.git
#     branch: main
#     include:
#       - "**/*.md"
#     exclude:
#       - "**/node_modules/**"

# Example local source:
# sources:
#   - id: my-internal-docs
#     type: local
#     path: /absolute/or/relative/path/to/docs
#     include:
#       - "**/*.md"
"#;

/// Initialise the `sources.yaml` in the workspace config directory.
///
/// If the file already exists and `force` is `false`, returns without
/// modification (idempotent). If `force` is `true`, overwrites.
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] on I/O failure.
pub fn init_sources_yaml(workspace_dir: &Path, force: bool) -> Result<InitResult, GraphtorError> {
    let config_dir = workspace_dir.join("config");
    fs::create_dir_all(&config_dir).map_err(|e| GraphtorError::Config {
        message: format!("failed to create config dir: {e}"),
        field: None,
    })?;

    let target = config_dir.join("sources.yaml");
    if target.exists() && !force {
        return Ok(InitResult {
            path: target,
            created: false,
        });
    }

    fs::write(&target, SOURCES_YAML_TEMPLATE).map_err(|e| GraphtorError::Config {
        message: format!("failed to write sources.yaml: {e}"),
        field: None,
    })?;

    Ok(InitResult {
        path: target,
        created: true,
    })
}

/// Result of a sources.yaml init operation.
#[derive(Debug)]
pub struct InitResult {
    /// Absolute path to the sources.yaml file.
    pub path: std::path::PathBuf,
    /// Whether the file was newly created (`true`) or already existed (`false`).
    pub created: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_template() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let ws = tmp.path().join(".graphtor");
        fs::create_dir_all(&ws).expect("create ws");
        let result = init_sources_yaml(&ws, false).expect("init");
        assert!(result.created);
        assert!(result.path.exists());
        let content = fs::read_to_string(&result.path).expect("read");
        assert!(content.contains("sources:"));
    }

    #[test]
    fn does_not_overwrite_without_force() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let ws = tmp.path().join(".graphtor");
        fs::create_dir_all(&ws).expect("create ws");
        init_sources_yaml(&ws, false).expect("first");
        // Write custom content.
        let config = ws.join("config").join("sources.yaml");
        fs::write(&config, "custom").expect("write custom");
        let result = init_sources_yaml(&ws, false).expect("second");
        assert!(!result.created);
        assert_eq!(fs::read_to_string(&result.path).expect("read"), "custom");
    }

    #[test]
    fn force_overwrites() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let ws = tmp.path().join(".graphtor");
        fs::create_dir_all(&ws).expect("create ws");
        let config = ws.join("config").join("sources.yaml");
        fs::create_dir_all(ws.join("config")).expect("mkdir");
        fs::write(&config, "custom").expect("write custom");
        let result = init_sources_yaml(&ws, true).expect("force");
        assert!(result.created);
        let content = fs::read_to_string(&result.path).expect("read");
        assert!(content.contains("sources:"));
    }
}
