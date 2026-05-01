//! Workspace health check and diagnostics (`doctor` command).
//!
//! Validates the workspace installation and reports pass/warn/fail status
//! for each check. Designed to help users diagnose misconfigured or
//! partially-installed workspaces.

use std::path::Path;

use crate::workspace::install::installed_binary_path;
use crate::workspace::paths::GRAPHTOR_SUBDIRS;

/// Severity of a diagnostic finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    /// Everything is correct.
    Pass,
    /// Something is sub-optimal but the system will still function.
    Warn,
    /// A required component is missing or broken.
    Fail,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pass => write!(f, "PASS"),
            Self::Warn => write!(f, "WARN"),
            Self::Fail => write!(f, "FAIL"),
        }
    }
}

/// A single diagnostic check result.
#[derive(Debug, Clone)]
pub struct Check {
    /// Short human-readable name for the check.
    #[allow(dead_code)]
    pub name: &'static str,
    /// Outcome of the check.
    pub severity: Severity,
    /// Descriptive message.
    pub message: String,
}

/// Disk usage warning threshold: 5 GiB.
const WARN_BYTES: u64 = 5 * 1024 * 1024 * 1024;

fn check_workspace_dir(workspace_dir: &Path) -> Check {
    if workspace_dir.is_dir() {
        Check {
            name: "workspace-dir",
            severity: Severity::Pass,
            message: format!("workspace directory exists: {}", workspace_dir.display()),
        }
    } else {
        Check {
            name: "workspace-dir",
            severity: Severity::Fail,
            message: format!(
                "workspace directory not found: {}; run `graphtor-docs install`",
                workspace_dir.display()
            ),
        }
    }
}

fn check_subdirs(workspace_dir: &Path) -> Vec<Check> {
    GRAPHTOR_SUBDIRS
        .iter()
        .map(|sub| {
            let dir = workspace_dir.join(sub);
            if dir.is_dir() {
                Check {
                    name: "subdir",
                    severity: Severity::Pass,
                    message: format!("{sub}/ present"),
                }
            } else {
                Check {
                    name: "subdir",
                    severity: Severity::Fail,
                    message: format!("{sub}/ missing; re-run `graphtor-docs install`"),
                }
            }
        })
        .collect()
}

fn check_sources_yaml(workspace_dir: &Path) -> Check {
    let sources_yaml = workspace_dir.join("config").join("sources.yaml");
    if !sources_yaml.exists() {
        return Check {
            name: "sources-yaml",
            severity: Severity::Warn,
            message: "sources.yaml not found; run `graphtor-docs init`".to_string(),
        };
    }
    match std::fs::read_to_string(&sources_yaml) {
        Ok(content) => match serde_yaml::from_str::<serde_yaml::Value>(&content) {
            Ok(_) => Check {
                name: "sources-yaml",
                severity: Severity::Pass,
                message: "sources.yaml is valid YAML".to_string(),
            },
            Err(e) => Check {
                name: "sources-yaml",
                severity: Severity::Fail,
                message: format!("sources.yaml has invalid YAML: {e}"),
            },
        },
        Err(e) => Check {
            name: "sources-yaml",
            severity: Severity::Fail,
            message: format!("failed to read sources.yaml: {e}"),
        },
    }
}

/// Run all workspace health checks.
///
/// Returns a list of [`Check`] results. The caller determines overall
/// pass/fail by inspecting the severities.
#[must_use]
pub fn run_doctor(workspace_dir: &Path) -> Vec<Check> {
    let mut checks = Vec::new();

    checks.push(check_workspace_dir(workspace_dir));
    checks.extend(check_subdirs(workspace_dir));

    // Binary present.
    let bin_path = installed_binary_path(workspace_dir);
    checks.push(if bin_path.exists() {
        Check {
            name: "binary",
            severity: Severity::Pass,
            message: format!("binary present: {}", bin_path.display()),
        }
    } else {
        Check {
            name: "binary",
            severity: Severity::Warn,
            message: format!(
                "binary not found at {}; run `graphtor-docs install`",
                bin_path.display()
            ),
        }
    });

    checks.push(check_sources_yaml(workspace_dir));

    // Database file accessible (default path: .graphtor/graph.db).
    let db_path = workspace_dir.join("graph.db");
    checks.push(if db_path.exists() {
        Check {
            name: "database",
            severity: Severity::Pass,
            message: format!("database present: {}", db_path.display()),
        }
    } else {
        Check {
            name: "database",
            severity: Severity::Warn,
            message: "database not yet created; run `graphtor-docs sync` to populate".to_string(),
        }
    });

    // Disk usage (warn if .graphtor/ > 5 GiB).
    let usage_bytes = dir_size(workspace_dir);
    checks.push(if usage_bytes < WARN_BYTES {
        Check {
            name: "disk-usage",
            severity: Severity::Pass,
            message: format!("workspace disk usage: {}", human_bytes(usage_bytes)),
        }
    } else {
        Check {
            name: "disk-usage",
            severity: Severity::Warn,
            message: format!(
                "workspace disk usage is high: {}; consider pruning cached clones",
                human_bytes(usage_bytes)
            ),
        }
    });

    checks
}

/// Compute total size of a directory tree in bytes.
fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

/// Format bytes as a human-readable string (e.g. `1.2 GB`).
#[allow(clippy::cast_precision_loss)]
fn human_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_reports_fail_when_dir_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let fake_ws = tmp.path().join("nonexistent");
        let checks = run_doctor(&fake_ws);
        let ws_check = checks
            .iter()
            .find(|c| c.name == "workspace-dir")
            .expect("check");
        assert_eq!(ws_check.severity, Severity::Fail);
    }

    #[test]
    fn human_bytes_formats_correctly() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024 * 2), "2.0 KB");
        assert_eq!(human_bytes(1024 * 1024 * 3), "3.0 MB");
    }
}
