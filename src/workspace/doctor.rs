//! Workspace health check and diagnostics (`doctor` command).
//!
//! Validates the workspace installation and reports pass/warn/fail status
//! for each check. Designed to help users diagnose misconfigured or
//! partially-installed workspaces.

use std::path::Path;

use crate::workspace::install::installed_binary_path;
use crate::workspace::paths::{GRAPHTOR_INGESTION_SUBDIRS, GRAPHTOR_SUBDIRS};

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

/// Whether a workspace uses the full ingestion-capable scaffold or the
/// consumption-first minimal footprint (P2-T1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceFootprint {
    /// The full scaffold: `.graphtor/{bin,data,cache,config,logs}` — created
    /// by `install --with-ingestion` (or the legacy default before this
    /// shipment).
    Full,
    /// The consumption-first minimal footprint: `.graphtor/` exists but
    /// none of the ingestion-capable subdirectories were created — created
    /// by the default `install` (P2-T1).
    Minimal,
}

impl WorkspaceFootprint {
    /// Lowercase string form for structured (JSON) output.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Minimal => "minimal",
        }
    }
}

/// Detect whether `workspace_dir` uses the full or minimal footprint.
///
/// A workspace is [`WorkspaceFootprint::Full`] when ANY of the
/// ingestion-scaffold subdirectories ([`GRAPHTOR_INGESTION_SUBDIRS`]: `bin/`,
/// `data/`, `cache/`, `logs/`) exist — matching conservatively toward `Full`
/// for a partially scaffolded or in-transition ingestion install, so `doctor`
/// never silently suppresses a real problem in an otherwise-full install.
/// Otherwise it is [`WorkspaceFootprint::Minimal`].
///
/// `config/` is intentionally EXCLUDED from this signal even though it is a
/// managed subdirectory: it holds `sources.yaml`, which a consumption-only
/// workspace legitimately uses for explicit `type: database` entries with no
/// ingestion scaffold at all. Treating a bare `config/` as `Full` would
/// misclassify that valid consumption-only workspace as a broken full install,
/// making `doctor` report the missing ingestion scaffold as failures and
/// `upgrade` attempt to copy a binary into a nonexistent `bin/`.
#[must_use]
pub fn detect_footprint(workspace_dir: &Path) -> WorkspaceFootprint {
    let any_ingestion_subdir_exists = GRAPHTOR_INGESTION_SUBDIRS
        .iter()
        .any(|sub| workspace_dir.join(sub).is_dir());
    if any_ingestion_subdir_exists {
        WorkspaceFootprint::Full
    } else {
        WorkspaceFootprint::Minimal
    }
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

fn check_subdirs(workspace_dir: &Path, footprint: WorkspaceFootprint) -> Vec<Check> {
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
            } else if footprint == WorkspaceFootprint::Minimal {
                Check {
                    name: "subdir",
                    severity: Severity::Pass,
                    message: format!(
                        "{sub}/ not created — consumption-first minimal install \
                         (run `graphtor-docs install --with-ingestion` for the full scaffold)"
                    ),
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

fn check_sources_yaml(workspace_dir: &Path, footprint: WorkspaceFootprint) -> Check {
    let sources_yaml = workspace_dir.join("config").join("sources.yaml");
    if !sources_yaml.exists() {
        return if footprint == WorkspaceFootprint::Minimal {
            Check {
                name: "sources-yaml",
                severity: Severity::Pass,
                message: "sources.yaml not created — consumption-first minimal install \
                          has no ingestion source to configure"
                    .to_string(),
            }
        } else {
            Check {
                name: "sources-yaml",
                severity: Severity::Warn,
                message: "sources.yaml not found; run `graphtor-docs init`".to_string(),
            }
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
///
/// Layout-aware (P2-T4): when [`detect_footprint`] reports
/// [`WorkspaceFootprint::Minimal`] — the consumption-first default install
/// (P2-T1) — the missing `config/sources.yaml`, `bin/` (and the other
/// ingestion subdirectories), and default `graph.db` checks are downgraded
/// to informational ([`Severity::Pass`]) instead of [`Severity::Warn`] or
/// [`Severity::Fail`], since none of these are expected to exist in a
/// minimal install. [`WorkspaceFootprint::Full`] behaviour is unchanged.
#[must_use]
pub fn run_doctor(workspace_dir: &Path) -> Vec<Check> {
    let mut checks = Vec::new();
    let footprint = detect_footprint(workspace_dir);

    checks.push(check_workspace_dir(workspace_dir));
    checks.extend(check_subdirs(workspace_dir, footprint));

    // Binary present.
    let bin_path = installed_binary_path(workspace_dir);
    checks.push(if bin_path.exists() {
        Check {
            name: "binary",
            severity: Severity::Pass,
            message: format!("binary present: {}", bin_path.display()),
        }
    } else if footprint == WorkspaceFootprint::Minimal {
        Check {
            name: "binary",
            severity: Severity::Pass,
            message: "binary not copied — consumption-first minimal install resolves \
                      graphtor-docs via PATH"
                .to_string(),
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

    checks.push(check_sources_yaml(workspace_dir, footprint));

    // Database file accessible (default path: .graphtor/graph.db).
    let db_path = workspace_dir.join("graph.db");
    checks.push(if db_path.exists() {
        Check {
            name: "database",
            severity: Severity::Pass,
            message: format!("database present: {}", db_path.display()),
        }
    } else if footprint == WorkspaceFootprint::Minimal {
        Check {
            name: "database",
            severity: Severity::Pass,
            message: "no default database yet — drop a `.db` file into .graphtor/ for serve \
                      to auto-discover"
                .to_string(),
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
    use crate::workspace::install::{install, install_minimal};

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

    #[test]
    fn detect_footprint_returns_minimal_when_only_graphtor_root_exists() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install_minimal(tmp.path()).expect("install_minimal");
        let ws = tmp.path().join(crate::workspace::paths::GRAPHTOR_DIR);
        assert_eq!(detect_footprint(&ws), WorkspaceFootprint::Minimal);
    }

    #[test]
    fn detect_footprint_returns_full_when_any_subdir_exists() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        let ws = tmp.path().join(crate::workspace::paths::GRAPHTOR_DIR);
        assert_eq!(detect_footprint(&ws), WorkspaceFootprint::Full);
    }

    #[test]
    fn detect_footprint_returns_minimal_for_config_only_consumption_workspace() {
        // A consumption-only workspace legitimately has `config/sources.yaml`
        // (declaring only `type: database` sources) with NO ingestion scaffold.
        // A bare `config/` must NOT promote the workspace to `Full`.
        let tmp = tempfile::tempdir().expect("tempdir");
        let ws = tmp.path().join(crate::workspace::paths::GRAPHTOR_DIR);
        let config_dir = ws.join("config");
        std::fs::create_dir_all(&config_dir).expect("create config dir");
        std::fs::write(
            config_dir.join("sources.yaml"),
            "sources:\n  - type: database\n    path: ./external.db\n",
        )
        .expect("write sources.yaml");

        assert_eq!(
            detect_footprint(&ws),
            WorkspaceFootprint::Minimal,
            "a config-only consumption workspace must be Minimal, not Full"
        );

        // Layout-aware doctor must not flag the missing ingestion scaffold as a
        // failure for this valid consumption workspace.
        for check in &run_doctor(&ws) {
            assert_ne!(
                check.severity,
                Severity::Fail,
                "config-only consumption layout must report no Fail: {} — {}",
                check.name,
                check.message
            );
        }
    }

    #[test]
    fn doctor_on_minimal_layout_reports_no_fail_or_warn() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install_minimal(tmp.path()).expect("install_minimal");
        let ws = tmp.path().join(crate::workspace::paths::GRAPHTOR_DIR);

        let checks = run_doctor(&ws);

        for check in &checks {
            assert_ne!(
                check.severity,
                Severity::Fail,
                "minimal layout must report no Fail: {} — {}",
                check.name,
                check.message
            );
            assert_ne!(
                check.severity,
                Severity::Warn,
                "minimal layout must report no Warn: {} — {}",
                check.name,
                check.message
            );
        }
    }

    #[test]
    fn doctor_on_full_layout_matches_pre_existing_behavior() {
        let tmp = tempfile::tempdir().expect("tempdir");
        install(tmp.path()).expect("install");
        let ws = tmp.path().join(crate::workspace::paths::GRAPHTOR_DIR);

        let checks = run_doctor(&ws);

        for sub in GRAPHTOR_SUBDIRS {
            let subdir_checks: Vec<_> = checks
                .iter()
                .filter(|c| c.name == "subdir" && c.message.contains(sub))
                .collect();
            assert!(
                subdir_checks.iter().all(|c| c.severity == Severity::Pass),
                "full layout: {sub} subdir check must still Pass"
            );
        }
        let sources_check = checks
            .iter()
            .find(|c| c.name == "sources-yaml")
            .expect("sources-yaml check");
        assert_eq!(
            sources_check.severity,
            Severity::Warn,
            "full layout without sources.yaml must still Warn (pre-existing behaviour)"
        );
        let binary_check = checks
            .iter()
            .find(|c| c.name == "binary")
            .expect("binary check");
        assert_eq!(
            binary_check.severity,
            Severity::Pass,
            "full layout copies the binary, so this must still Pass"
        );
        let db_check = checks
            .iter()
            .find(|c| c.name == "database")
            .expect("database check");
        assert_eq!(
            db_check.severity,
            Severity::Warn,
            "full layout without a synced database must still Warn (pre-existing behaviour)"
        );
    }
}
