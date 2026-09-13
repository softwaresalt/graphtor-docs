//! Black-box self-tests for `mcp_probe::workspace`, covering `056.021-T`'s
//! acceptance criteria: exclusive creation, containment/reparse rejection
//! (both directly on the leaf and via a tampered ancestor component), and
//! control/treatment/ancestor fixture content correctness.
//!
//! Every test builds its own isolated fake "repository root" under the OS
//! temp directory rather than touching this crate's own working tree or
//! the real repository's `logs/probe/`, mirroring `process_test.rs`'s
//! `unique_temp_evidence_path()` hygiene.

use mcp_probe::workspace::{create_probe_workspace, McpServerEntrySpec, WorkspaceError};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

fn probe_bin() -> &'static str {
    env!("CARGO_BIN_EXE_mcp-probe")
}

/// A unique, freshly created directory under the OS temp directory to
/// stand in as a fake "repository root" for one test. `workspace.rs`
/// only ever writes inside `<this>/logs/probe/<nonce>`, so this keeps
/// every test fully self-contained.
fn fresh_fake_repo_root(label: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "mcp-probe-workspace-test-{label}-{}-{n}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("create fake repo root");
    root
}

fn fixture_entry() -> McpServerEntrySpec {
    McpServerEntrySpec {
        entry_name: "probe-target".to_string(),
        wrapper_exe: probe_bin().to_string(),
        inner_exe: probe_bin().to_string(),
        inner_args: vec!["__echo".to_string()],
    }
}

fn read_json(path: &std::path::Path) -> serde_json::Value {
    let bytes = std::fs::read(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    serde_json::from_slice(&bytes).unwrap_or_else(|err| panic!("parse {}: {err}", path.display()))
}

fn server_entry<'a>(document: &'a serde_json::Value, entry_name: &str) -> &'a serde_json::Value {
    document
        .get("mcpServers")
        .and_then(|servers| servers.get(entry_name))
        .unwrap_or_else(|| panic!("mcpServers.{entry_name} missing from {document}"))
}

#[test]
fn create_probe_workspace_produces_the_expected_layout() {
    let repo_root = fresh_fake_repo_root("layout");
    let entry = fixture_entry();

    let workspace = create_probe_workspace(&repo_root, "nonce-layout", &entry)
        .expect("create_probe_workspace should succeed for a fresh nonce");

    assert!(workspace.root().is_dir());
    assert!(workspace.control_dir().is_dir());
    assert!(workspace.treatment_dir().is_dir());
    assert!(workspace.ancestor_dir().is_dir());
    assert!(workspace.ancestor_run_dir().is_dir());
    assert!(workspace.control_config_path().is_file());
    assert!(workspace.treatment_config_path().is_file());
    assert!(workspace.ancestor_config_path().is_file());
    assert!(workspace.ancestor_run_config_path().is_file());
    assert_eq!(workspace.nonce(), "nonce-layout");

    // The workspace root itself must be exactly logs/probe/<nonce> under
    // the CANONICAL repo root (U-7: every returned `ProbeWorkspace` path
    // is built from `canonical_repo_root`, not the raw, possibly
    // relative/symlinked `repo_root` argument, so it stays portable
    // regardless of what form the caller's `repo_root` took).
    let canonical_repo_root = std::fs::canonicalize(&repo_root).expect("canonicalize repo root");
    let canonical_workspace_root =
        std::fs::canonicalize(workspace.root()).expect("canonicalize workspace root");
    assert!(canonical_workspace_root.starts_with(&canonical_repo_root));
    assert_eq!(
        workspace.root(),
        canonical_repo_root
            .join("logs")
            .join("probe")
            .join("nonce-layout")
    );

    let control_doc = read_json(workspace.control_config_path());
    let control_entry = server_entry(&control_doc, &entry.entry_name);
    assert_eq!(control_entry["type"], "stdio");
    assert_eq!(control_entry["command"], entry.wrapper_exe);
    assert!(
        control_entry.get("cwd").is_none(),
        "control leg must never carry a cwd key"
    );

    let control_args = control_entry["args"]
        .as_array()
        .expect("control args must be an array");
    assert_eq!(control_args[0], "wrapper");
    assert!(control_args.iter().any(|value| value == "--inner-exe"));
    assert!(control_args.iter().any(|value| value == "--inner-arg"));
    assert!(control_args
        .iter()
        .any(|value| value == "--evidence-output"));
    assert!(control_args.iter().any(|value| value == "--run-nonce"));
    assert!(control_args.iter().any(|value| value == "nonce-layout"));

    let _ = std::fs::remove_dir_all(&repo_root);
}

#[test]
fn control_and_treatment_wrapper_args_are_byte_identical_except_for_cwd() {
    let repo_root = fresh_fake_repo_root("parity");
    let entry = fixture_entry();

    let workspace = create_probe_workspace(&repo_root, "nonce-parity", &entry)
        .expect("create_probe_workspace should succeed");

    let control_doc = read_json(workspace.control_config_path());
    let treatment_doc = read_json(workspace.treatment_config_path());
    let control_entry = server_entry(&control_doc, &entry.entry_name);
    let treatment_entry = server_entry(&treatment_doc, &entry.entry_name);

    assert_eq!(
        control_entry["args"], treatment_entry["args"],
        "control and treatment wrapper args must be byte-identical, including \
         --evidence-output and --run-nonce values"
    );
    assert_eq!(control_entry["command"], treatment_entry["command"]);
    assert_eq!(control_entry["type"], treatment_entry["type"]);

    assert!(
        control_entry.get("cwd").is_none(),
        "control leg must never carry a cwd key"
    );
    let treatment_cwd = treatment_entry
        .get("cwd")
        .expect("treatment leg must carry a cwd key")
        .as_str()
        .expect("cwd must be a string");
    let canonical_repo_root = std::fs::canonicalize(&repo_root).expect("canonicalize repo root");
    assert_eq!(treatment_cwd, canonical_repo_root.to_string_lossy());
    assert_eq!(workspace.treatment_cwd(), canonical_repo_root);

    let _ = std::fs::remove_dir_all(&repo_root);
}

#[test]
fn create_probe_workspace_rejects_a_pre_existing_nonce_directory() {
    let repo_root = fresh_fake_repo_root("exclusive");
    let entry = fixture_entry();

    let _first = create_probe_workspace(&repo_root, "nonce-reused", &entry)
        .expect("first creation at a fresh nonce must succeed");

    let second = create_probe_workspace(&repo_root, "nonce-reused", &entry);
    match second {
        Err(WorkspaceError::AlreadyExists(path)) => {
            assert!(path.ends_with("nonce-reused"));
        }
        other => panic!("expected AlreadyExists for a reused nonce, got {other:?}"),
    }

    let _ = std::fs::remove_dir_all(&repo_root);
}

#[test]
fn ancestor_fixture_is_invalid_and_ancestor_run_fixture_is_valid() {
    let repo_root = fresh_fake_repo_root("ancestor");
    let entry = fixture_entry();

    let workspace = create_probe_workspace(&repo_root, "nonce-ancestor", &entry)
        .expect("create_probe_workspace should succeed");

    let ancestor_bytes =
        std::fs::read(workspace.ancestor_config_path()).expect("read ancestor sentinel config");
    let ancestor_parse: Result<serde_json::Value, _> = serde_json::from_slice(&ancestor_bytes);
    assert!(
        ancestor_parse.is_err(),
        "ancestor config must be deliberately invalid JSON, a CLI that reads it must fail loudly"
    );

    let ancestor_run_doc = read_json(workspace.ancestor_run_config_path());
    let ancestor_run_entry = server_entry(&ancestor_run_doc, &entry.entry_name);
    assert_eq!(ancestor_run_entry["type"], "stdio");
    assert!(
        ancestor_run_entry.get("cwd").is_none(),
        "the ancestor-run fixture is its own separate Gate-1 proof, not part of the \
         control/treatment cwd contrast"
    );

    // The ancestor-run leg owns its own separate evidence_output, distinct
    // from the shared control/treatment one.
    assert_ne!(
        workspace.ancestor_run_evidence_output(),
        workspace.evidence_output(),
        "the ancestor-run leg must not share the control/treatment evidence_output path"
    );
    assert!(workspace
        .ancestor_run_dir()
        .starts_with(workspace.ancestor_dir()));

    let _ = std::fs::remove_dir_all(&repo_root);
}

#[test]
fn create_probe_workspace_rejects_unsafe_nonces_before_touching_the_filesystem() {
    let repo_root = fresh_fake_repo_root("unsafe-nonce");
    let entry = fixture_entry();

    let unsafe_nonces: &[&str] = &[
        "",
        ".",
        "..",
        "../escape",
        "a/b",
        "a\\b",
        "/absolute",
        "\\absolute",
    ];

    for nonce in unsafe_nonces {
        let result = create_probe_workspace(&repo_root, nonce, &entry);
        match result {
            Err(WorkspaceError::InvalidNonce(rejected)) => {
                assert_eq!(&rejected, nonce);
            }
            other => panic!("expected InvalidNonce for nonce {nonce:?}, got {other:?}"),
        }
    }

    // Nothing must have been created at all -- an unsafe nonce is
    // rejected strictly BEFORE any join/create, unlike the containment
    // check (which can only run after a path already exists).
    assert!(
        !repo_root.join("logs").exists(),
        "an unsafe nonce must never cause even the shared logs/probe parent chain to be created"
    );

    let _ = std::fs::remove_dir_all(&repo_root);
}

#[test]
fn create_probe_workspace_rejects_a_traversal_nonce_without_ever_escaping_logs_probe() {
    let repo_root = fresh_fake_repo_root("traversal-escape");
    let entry = fixture_entry();

    // Without the pre-join/pre-create nonce validation, `probe_root
    // .join("../escape-nonce")` would resolve (at the OS level, when
    // `fs::create_dir` is actually invoked) to a directory as a SIBLING
    // of `logs/probe/` -- i.e. `repo_root/logs/escape-nonce` -- fully
    // outside the intended `logs/probe/<nonce>` containment boundary,
    // and it would exist on disk BEFORE `validate_containment` ever ran.
    let result = create_probe_workspace(&repo_root, "../escape-nonce", &entry);
    assert!(
        matches!(result, Err(WorkspaceError::InvalidNonce(_))),
        "expected InvalidNonce for a traversal nonce, got {result:?}"
    );

    let would_be_escape_path = repo_root.join("logs").join("escape-nonce");
    assert!(
        !would_be_escape_path.exists(),
        "a traversal nonce must never cause a directory to be created outside logs/probe/: {}",
        would_be_escape_path.display()
    );
    assert!(
        !repo_root.join("logs").exists(),
        "a traversal nonce must never cause even the shared logs/probe parent chain to be \
         created, since validation runs before any join/create"
    );

    let _ = std::fs::remove_dir_all(&repo_root);
}

#[test]
fn create_probe_workspace_rejects_a_pre_existing_junction_at_the_exact_workspace_path() {
    let repo_root = fresh_fake_repo_root("leaf-junction");
    let entry = fixture_entry();
    let nonce = "nonce-leaf-junction";

    // A junction target must already exist and must live outside the
    // fake repo root so a successful "escape" would be observable.
    let junction_target = fresh_fake_repo_root("leaf-junction-target");
    let intended_workspace_root = repo_root.join("logs").join("probe").join(nonce);
    std::fs::create_dir_all(intended_workspace_root.parent().unwrap())
        .expect("create logs/probe parent chain");

    let mklink_ok = std::process::Command::new("cmd")
        .args([
            "/C",
            "mklink",
            "/J",
            &intended_workspace_root.to_string_lossy(),
            &junction_target.to_string_lossy(),
        ])
        .status()
        .is_ok_and(|status| status.success());
    if !mklink_ok {
        eprintln!("skipping: mklink /J failed to create a junction in this environment");
        let _ = std::fs::remove_dir_all(&repo_root);
        let _ = std::fs::remove_dir_all(&junction_target);
        return;
    }

    let result = create_probe_workspace(&repo_root, nonce, &entry);
    match result {
        Err(WorkspaceError::ReparsePoint(path)) => {
            assert!(path.ends_with(nonce));
        }
        other => panic!("expected ReparsePoint for a pre-existing junction, got {other:?}"),
    }

    // Removing a junction removes only the reparse point itself, never
    // the target's contents.
    let _ = std::fs::remove_dir(&intended_workspace_root);
    let _ = std::fs::remove_dir_all(&repo_root);
    let _ = std::fs::remove_dir_all(&junction_target);
}

#[test]
fn create_probe_workspace_rejects_an_ancestor_component_redirected_outside_the_repo_root() {
    let repo_root = fresh_fake_repo_root("ancestor-escape");
    let entry = fixture_entry();

    // Redirect `repo_root/logs` itself to a junction pointing outside
    // the fake repo root, before create_probe_workspace ever runs.
    let escape_target = fresh_fake_repo_root("ancestor-escape-target");
    let logs_path = repo_root.join("logs");

    let mklink_ok = std::process::Command::new("cmd")
        .args([
            "/C",
            "mklink",
            "/J",
            &logs_path.to_string_lossy(),
            &escape_target.to_string_lossy(),
        ])
        .status()
        .is_ok_and(|status| status.success());
    if !mklink_ok {
        eprintln!("skipping: mklink /J failed to create a junction in this environment");
        let _ = std::fs::remove_dir_all(&repo_root);
        let _ = std::fs::remove_dir_all(&escape_target);
        return;
    }

    let result = create_probe_workspace(&repo_root, "nonce-ancestor-escape", &entry);
    match result {
        Err(WorkspaceError::ContainmentEscape { .. } | WorkspaceError::ReparsePoint(_)) => {}
        other => panic!(
            "expected ContainmentEscape or ReparsePoint for a redirected ancestor component, got {other:?}"
        ),
    }

    // Nothing must ever have been created inside the escape target: the
    // per-component containment check on `logs` itself must reject the
    // junction before `probe` is ever joined onto or created beneath it.
    assert!(
        !escape_target.join("probe").exists(),
        "a redirected `logs` ancestor component must be rejected before anything is ever \
         created inside the escape target"
    );

    // `logs` is itself the junction; removing it removes only the
    // reparse point, never the escape target's contents.
    let _ = std::fs::remove_dir(&logs_path);
    let _ = std::fs::remove_dir_all(&repo_root);
    let _ = std::fs::remove_dir_all(&escape_target);
}

#[test]
fn create_probe_workspace_rejects_a_redirected_probe_component_under_a_genuine_logs_dir() {
    let repo_root = fresh_fake_repo_root("probe-component-escape");
    let entry = fixture_entry();

    // `logs` itself is a genuine, ordinary directory (as it would be
    // after a prior, legitimate probe run) but `logs/probe` -- the NEXT
    // component down -- has been redirected to a junction pointing
    // outside the repo root. This is the scenario
    // create_shared_dir_component_validated exists to catch: a single
    // fs::create_dir_all(repo_root/logs/probe) call would see `logs`
    // already exists and transparently traverse through it, only to
    // find `probe` is itself the tampered junction -- but by validating
    // `logs` and `probe` as two SEPARATE components in turn, the `probe`
    // junction must be caught at that exact component, never followed.
    let escape_target = fresh_fake_repo_root("probe-component-escape-target");
    let logs_path = repo_root.join("logs");
    std::fs::create_dir_all(&logs_path).expect("create genuine logs dir");
    let probe_path = logs_path.join("probe");

    let mklink_ok = std::process::Command::new("cmd")
        .args([
            "/C",
            "mklink",
            "/J",
            &probe_path.to_string_lossy(),
            &escape_target.to_string_lossy(),
        ])
        .status()
        .is_ok_and(|status| status.success());
    if !mklink_ok {
        eprintln!("skipping: mklink /J failed to create a junction in this environment");
        let _ = std::fs::remove_dir_all(&repo_root);
        let _ = std::fs::remove_dir_all(&escape_target);
        return;
    }

    let result = create_probe_workspace(&repo_root, "nonce-probe-component-escape", &entry);
    match result {
        Err(WorkspaceError::ReparsePoint(path)) => {
            assert!(path.ends_with("probe"));
        }
        other => panic!("expected ReparsePoint for a redirected probe component, got {other:?}"),
    }

    // Nothing must have been created inside the escape target: `probe`
    // being a junction must be caught at that exact component, before
    // any nonce leaf is ever joined onto or created beneath it.
    assert!(
        !escape_target.join("nonce-probe-component-escape").exists(),
        "a redirected `probe` component must be rejected before a nonce leaf is ever created \
         inside the escape target"
    );

    // `probe` is itself the junction; removing it removes only the
    // reparse point, never the escape target's contents.
    let _ = std::fs::remove_dir(&probe_path);
    let _ = std::fs::remove_dir_all(&repo_root);
    let _ = std::fs::remove_dir_all(&escape_target);
}

/// Security Reviewer finding (`056` shipment review): every generated
/// `.mcp.json` fixture embeds the real, unredacted production
/// `--inner-exe`/`--inner-arg` values by design -- redacting them would
/// break the differential reproduction itself, since these files are the
/// actual launch config the exact-CLI runner points a real Copilot CLI
/// process at (see `workspace.rs`'s module docs, "Real, unredacted args
/// are written by design"). The compensating control is owner-only
/// (`0600`) file permissions from the very first inode, not content
/// redaction -- mirroring `workspace::mcp_config`'s identical rationale
/// for the same real `.mcp.json` file kind. Unix-only: Windows
/// `std::fs::Permissions` tracks only a readonly bit with no owner-only
/// mode concept.
#[test]
#[cfg(unix)]
fn generated_mcp_json_fixtures_are_created_owner_only() {
    use std::os::unix::fs::PermissionsExt;

    let repo_root = fresh_fake_repo_root("owner-only-perms");
    let entry = fixture_entry();

    let workspace = create_probe_workspace(&repo_root, "nonce-owner-only", &entry)
        .expect("create_probe_workspace should succeed");

    for path in [
        workspace.control_config_path(),
        workspace.treatment_config_path(),
        workspace.ancestor_run_config_path(),
    ] {
        let mode = std::fs::metadata(path)
            .unwrap_or_else(|err| panic!("metadata {}: {err}", path.display()))
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(
            mode,
            0o600,
            "{} must be created owner-only (0600), not umask-default -- it carries the \
             real, unredacted production inner-exe/inner-args verbatim",
            path.display()
        );
    }

    // The deliberately-invalid ancestor sentinel carries no real
    // production data (a fixed, never-meant-to-parse constant), so it is
    // NOT subject to this owner-only requirement.
    assert!(workspace.ancestor_config_path().is_file());

    let _ = std::fs::remove_dir_all(&repo_root);
}
