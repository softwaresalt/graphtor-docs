//! Integration tests for acquisition planning, idempotent re-runs (US4), and source validation (US5).
//!
//! Tests cover: idempotent Git re-run (S048), local source re-scan (S049),
//! valid config (S035), invalid URL (S036), non-existent path (S038),
//! and multiple errors collected in a single pass (S040).

use std::fs;
use std::path::{Path, PathBuf};

use graphtor_core::acquire::{execute, plan, validate_sources, SourceAction, SourceOutcome};
use graphtor_core::config::source::{GitSource, LocalSource, Source};
use graphtor_core::config::SourceConfig;

// ── Test infrastructure ─────────────────────────────────────────────────────────

fn make_bare_repo(parent: &Path) -> PathBuf {
    let bare_path = parent.join("bare_repo.git");
    let repo = git2::Repository::init_bare(&bare_path).expect("init bare repo");
    let blob = repo
        .blob(b"# Test Repository\n")
        .expect("create README blob");
    let mut tb = repo.treebuilder(None).expect("treebuilder");
    tb.insert("README.md", blob, 0o100_644)
        .expect("insert README.md");
    let tree_oid = tb.write().expect("write tree");
    let tree = repo.find_tree(tree_oid).expect("find tree");
    let sig =
        git2::Signature::now("Test Author", "test@example.com").expect("create git signature");
    repo.commit(
        Some("refs/heads/main"),
        &sig,
        &sig,
        "Initial commit",
        &tree,
        &[],
    )
    .expect("commit");
    repo.set_head("refs/heads/main").expect("set HEAD");
    bare_path
}

fn path_to_url(path: &Path) -> String {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        format!("file:///{}", stripped.replace('\\', "/"))
    } else if s.starts_with('\\') {
        format!("file://{}", s.replace('\\', "/"))
    } else {
        format!("file:///{}", s.replace('\\', "/"))
    }
}

fn git_source(id: &str, url: &str, branch: &str) -> Source {
    Source::Git(GitSource {
        id: id.to_string(),
        url: url.to_string(),
        branch: branch.to_string(),
        include: vec![],
        exclude: vec![],
    })
}

fn local_source(id: &str, path: impl Into<PathBuf>) -> Source {
    Source::Local(LocalSource {
        id: id.to_string(),
        path: path.into(),
        include: vec![],
        exclude: vec![],
    })
}

// ── T030: S048 — Idempotent Git re-run ─────────────────────────────────────────

#[test]
fn s048_second_git_acquire_produces_skip_outcome() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let bare_path = make_bare_repo(root);
    let url = path_to_url(&bare_path);

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![git_source("repo1", &url, "main")],
    };

    // First run: must CloneGit
    let plan1 = plan(&config, &data_root, root).expect("plan1");
    assert_eq!(
        plan1.sources[0].action,
        SourceAction::CloneGit,
        "first run must CloneGit"
    );
    let result1 = execute(&plan1, false);
    assert_eq!(result1.succeeded, 1, "first run: succeeded=1");
    assert_eq!(result1.skipped, 0, "first run: skipped=0");

    // Second run: must SkipGit
    let plan2 = plan(&config, &data_root, root).expect("plan2");
    assert_eq!(
        plan2.sources[0].action,
        SourceAction::SkipGit,
        "second run must SkipGit"
    );
    let result2 = execute(&plan2, false);
    assert_eq!(result2.succeeded, 0, "second run: succeeded=0");
    assert_eq!(result2.skipped, 1, "second run: skipped=1");
    assert!(
        matches!(&result2.outcomes[0], SourceOutcome::Skipped { source_id } if source_id == "repo1"),
        "second run outcome must be Skipped for repo1"
    );
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
    let result1 = execute(&plan1, false);
    let files1 = match &result1.outcomes[0] {
        SourceOutcome::Success(ffs) => ffs.files.len(),
        other => panic!("expected Success, got: {other:?}"),
    };
    assert_eq!(files1, 2, "first scan should find 2 files");

    // Add a new file
    fs::write(local_dir.join("c.md"), "# c").expect("write c.md");

    // Second run: 3 files (re-scan picks up new file)
    let plan2 = plan(&config, &data_root, root).expect("plan2");
    let result2 = execute(&plan2, false);
    let files2 = match &result2.outcomes[0] {
        SourceOutcome::Success(ffs) => ffs.files.len(),
        other => panic!("expected Success, got: {other:?}"),
    };
    assert_eq!(
        files2, 3,
        "second scan should find 3 files (including c.md)"
    );
}

// ── T043: S046 — Dry-run mode skips all I/O ───────────────────────────────────

#[test]
fn s046_dry_run_does_not_clone_or_create_files() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let bare_path = make_bare_repo(root);
    let url = path_to_url(&bare_path);

    let data_root = root.join("data");
    let config = SourceConfig {
        sources: vec![git_source("repo1", &url, "main")],
    };

    let acq_plan = plan(&config, &data_root, root).expect("plan");
    assert_eq!(
        acq_plan.sources[0].action,
        SourceAction::CloneGit,
        "plan must resolve CloneGit"
    );

    let target_dir = acq_plan.sources[0].target_dir.clone();

    // Execute with dry_run=true — must NOT clone
    let result = execute(&acq_plan, true);

    assert!(
        !target_dir.join(".git").exists(),
        "dry_run must not create a .git directory"
    );
    assert_eq!(result.total_sources, 1);
    assert_eq!(
        result.succeeded, 0,
        "dry_run: no sources count as succeeded"
    );
    assert_eq!(result.skipped, 1, "dry_run: all sources counted as skipped");
}

#[test]
fn s035_valid_config_produces_no_validation_errors() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let local_dir = root.join("docs");
    fs::create_dir_all(&local_dir).expect("create docs dir");

    let config = SourceConfig {
        sources: vec![
            git_source(
                "ms-docs",
                "https://github.com/MicrosoftDocs/azure-docs.git",
                "main",
            ),
            local_source("local-docs", &local_dir),
        ],
    };

    let report = validate_sources(&config, root);
    assert!(report.is_valid(), "valid config must produce no errors");
    assert_eq!(report.total_count, 2);
    assert_eq!(report.valid_count, 2);
    assert!(report.errors.is_empty());
}

// ── T037: S036 — Invalid URL produces validation error ────────────────────────

#[test]
fn s036_invalid_url_is_detected() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let config = SourceConfig {
        sources: vec![git_source("bad-url-source", "not-a-valid-url", "main")],
    };

    let report = validate_sources(&config, root);
    assert!(!report.is_valid(), "invalid URL must produce an error");
    assert_eq!(report.errors.len(), 1);
    assert_eq!(report.errors[0].source_id, "bad-url-source");
    assert_eq!(report.errors[0].field, "url");
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
fn s040_multiple_errors_collected_in_single_pass() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();
    let missing = root.join("also-missing");

    let config = SourceConfig {
        sources: vec![
            git_source("bad-git", "ftp://not-valid.com/repo.git", "main"),
            local_source("bad-local", &missing),
        ],
    };

    let report = validate_sources(&config, root);
    assert!(!report.is_valid());
    assert_eq!(
        report.total_count, 2,
        "total_count must reflect all sources"
    );
    assert_eq!(
        report.errors.len(),
        2,
        "both sources must produce an error (single-pass collection)"
    );
    let source_ids: Vec<&str> = report.errors.iter().map(|e| e.source_id.as_str()).collect();
    assert!(
        source_ids.contains(&"bad-git"),
        "bad-git error must be collected"
    );
    assert!(
        source_ids.contains(&"bad-local"),
        "bad-local error must be collected"
    );
}

// ── RI-008: non-**-prefix pattern regression ────────────────────────────────────
//
// Verifies that patterns without a leading `**` (e.g. `docs/**/*.md`) correctly
// match files from scan_local_source() when path relativization is applied in
// scan_and_filter(). Without the fix these patterns would never match because glob
// patterns are compared against absolute paths.

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
        })],
    };

    let data_root = root.join("data");
    let acq_plan = plan(&config, &data_root, root).expect("plan");
    let result = execute(&acq_plan, false);

    let ffs = match &result.outcomes[0] {
        SourceOutcome::Success(ffs) => ffs,
        other => panic!("expected Success, got: {other:?}"),
    };

    assert_eq!(
        ffs.original_count, 2,
        "should scan both files before filtering"
    );
    assert_eq!(
        ffs.filtered_count, 1,
        "only the docs/ file should match `docs/**/*.md`"
    );
    assert!(
        ffs.files[0].to_string_lossy().contains("guide.md"),
        "matched file should be guide.md, got: {:?}",
        ffs.files
    );
}
