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
    let result1 = execute(&plan1);
    assert_eq!(result1.succeeded, 1, "first run: succeeded=1");
    assert_eq!(result1.skipped, 0, "first run: skipped=0");

    // Second run: must SkipGit
    let plan2 = plan(&config, &data_root, root).expect("plan2");
    assert_eq!(
        plan2.sources[0].action,
        SourceAction::SkipGit,
        "second run must SkipGit"
    );
    let result2 = execute(&plan2);
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
    let result1 = execute(&plan1);
    let files1 = match &result1.outcomes[0] {
        SourceOutcome::Success(ffs) => ffs.files.len(),
        other => panic!("expected Success, got: {other:?}"),
    };
    assert_eq!(files1, 2, "first scan should find 2 files");

    // Add a new file
    fs::write(local_dir.join("c.md"), "# c").expect("write c.md");

    // Second run: 3 files (re-scan picks up new file)
    let plan2 = plan(&config, &data_root, root).expect("plan2");
    let result2 = execute(&plan2);
    let files2 = match &result2.outcomes[0] {
        SourceOutcome::Success(ffs) => ffs.files.len(),
        other => panic!("expected Success, got: {other:?}"),
    };
    assert_eq!(
        files2, 3,
        "second scan should find 3 files (including c.md)"
    );
}

// ── T036: S035 — Valid config produces empty ValidationReport ──────────────────

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
