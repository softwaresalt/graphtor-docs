//! Integration tests for acquisition planning — local-only after docline pivot.

use std::fs;
use std::path::PathBuf;

use graphtor_core::acquire::{execute, plan, validate_sources, SourceAction, SourceOutcome};
use graphtor_core::config::source::{LocalSource, Source};
use graphtor_core::config::SourceConfig;

fn local_source(id: &str, path: impl Into<PathBuf>) -> Source {
    Source::Local(LocalSource {
        id: id.to_string(),
        path: path.into(),
        include: vec![],
        exclude: vec![],
        formats: vec![],
        database: None,
    })
}

// ── T031: S049 — Local source re-scanned on second run ─────────────────────────

#[test]
fn s049_local_source_rescanned_picks_up_new_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let local_dir = root.join("docs");
    fs::create_dir_all(&local_dir).expect("create docs dir");
    fs::write(local_dir.join("a.md"), "# a").expect("write a.md");
    fs::write(local_dir.join("b.md"), "# b").expect("write b.md");

    let config = SourceConfig {
        sources: vec![local_source("docs-source", &local_dir)],
    };
    let data_root = root.join("data");

    // First run: 2 files
    let plan1 = plan(&config, &data_root, root).expect("plan1");
    assert_eq!(plan1.sources[0].action, SourceAction::ScanLocal);
    let result1 = execute(&plan1, false);
    let files1 = match &result1.outcomes[0] {
        SourceOutcome::Success(ffs) => ffs.files.len(),
        SourceOutcome::Failed { error, .. } => panic!("expected Success, got Failed: {error}"),
    };
    assert_eq!(files1, 2, "first scan should find 2 files");

    // Add a new file
    fs::write(local_dir.join("c.md"), "# c").expect("write c.md");

    // Second run: 3 files (re-scan picks up new file)
    let plan2 = plan(&config, &data_root, root).expect("plan2");
    let result2 = execute(&plan2, false);
    let files2 = match &result2.outcomes[0] {
        SourceOutcome::Success(ffs) => ffs.files.len(),
        SourceOutcome::Failed { error, .. } => panic!("expected Success, got Failed: {error}"),
    };
    assert_eq!(files2, 3, "second scan should find 3 files");
}

// ── T035: valid config produces no validation errors ──────────────────────────

#[test]
fn s035_valid_local_config_produces_no_validation_errors() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let local_dir = root.join("docs");
    fs::create_dir_all(&local_dir).expect("create docs dir");

    let config = SourceConfig {
        sources: vec![local_source("local-docs", &local_dir)],
    };

    let report = validate_sources(&config, root);
    assert!(report.is_valid(), "valid config must produce no errors");
    assert_eq!(report.total_count, 1);
    assert_eq!(report.valid_count, 1);
    assert!(report.errors.is_empty());
}

// ── T038: S038 — Non-existent local path is detected ──────────────────────────

#[test]
fn s038_nonexistent_local_path_is_detected() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let missing = root.join("does-not-exist");

    let config = SourceConfig {
        sources: vec![local_source("missing-source", &missing)],
    };

    let report = validate_sources(&config, root);
    assert!(
        !report.is_valid(),
        "non-existent path must produce an error"
    );
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].source_id, "missing-source");
    assert_eq!(report.errors[0].field, "path");
}

// ── T039: S040 — Multiple errors collected in a single pass ───────────────────

#[test]
fn s040_multiple_local_errors_collected_in_single_pass() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let missing1 = root.join("not-here-1");
    let missing2 = root.join("not-here-2");

    let config = SourceConfig {
        sources: vec![
            local_source("bad-local-1", &missing1),
            local_source("bad-local-2", &missing2),
        ],
    };

    let report = validate_sources(&config, root);
    assert!(!report.is_valid());
    assert_eq!(report.total_count, 2);
    assert_eq!(report.errors.len(), 2);
    let ids: Vec<&str> = report.errors.iter().map(|e| e.source_id.as_str()).collect();
    assert!(ids.contains(&"bad-local-1"));
    assert!(ids.contains(&"bad-local-2"));
}

// ── RI-008: non-**-prefix pattern regression ──────────────────────────────────

#[test]
fn non_star_star_prefix_include_pattern_matches_subdir_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let local_dir = root.join("source");
    fs::create_dir_all(local_dir.join("docs")).expect("create docs dir");
    fs::create_dir_all(local_dir.join("api")).expect("create api dir");
    fs::write(local_dir.join("docs").join("guide.md"), "# Guide").expect("write guide.md");
    fs::write(local_dir.join("api").join("ref.md"), "# Ref").expect("write ref.md");

    let config = SourceConfig {
        sources: vec![Source::Local(LocalSource {
            id: "docs-only".to_string(),
            path: local_dir.clone(),
            include: vec!["docs/**/*.md".to_string()], // no leading **
            exclude: vec![],
            formats: vec![],
            database: None,
        })],
    };

    let data_root = root.join("data");
    let acq_plan = plan(&config, &data_root, root).expect("plan");
    let result = execute(&acq_plan, false);

    let ffs = match &result.outcomes[0] {
        SourceOutcome::Success(ffs) => ffs,
        SourceOutcome::Failed { error, .. } => panic!("expected Success, got Failed: {error}"),
    };

    assert_eq!(ffs.original_count, 2);
    assert_eq!(ffs.filtered_count, 1);
    assert!(ffs.files[0].to_string_lossy().contains("guide.md"));
}
