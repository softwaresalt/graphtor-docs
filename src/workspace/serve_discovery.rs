//! Serve/status-scoped `.graphtor/` root discovery of dropped `*.db` files.
//!
//! [`discover_served_databases`] assembles the FULL set of databases that
//! `serve`/`status` should expose as a UNION that PRESERVES the caller's
//! existing candidate inputs — the configured source target db paths from
//! `discover_db_files` (including generation targets not yet created on
//! disk) and the explicit `--db-path` / no-config candidate — plus any
//! EXISTING `*.db` file dropped directly into the `.graphtor/` workspace
//! root.
//!
//! The auto-discovered (root-scan) subset is force-classified read-only by
//! callers and MUST NEVER be fed back into `discover_db_files` /
//! `split_plan_by_database` (the sync/write chokepoint in `main.rs`) —
//! this module only ASSEMBLES the served set; it does not decide posture
//! (that is `P1-T2`) and it never mutates or calls into the sync path.
//!
//! The root scan is intentionally NON-RECURSIVE: the `.graphtor/` layout is
//! flat (`bin/`, `data/`, `cache/`, `config/`, `logs/`, `models/` are all
//! direct children — see [`super::paths`]), so only files directly inside
//! the root are ever candidates. This also means the `models/` cache
//! directory (and any other subdirectory) is excluded structurally, not
//! merely by name.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use graphtor_core::path::validate_path;
use graphtor_core::GraphtorError;

/// The only file extension eligible for auto-discovery.
const DB_EXTENSION: &str = "db";

/// Discover the served database set for `root`.
///
/// Returns the canonical-path-deduplicated UNION of `existing_candidates`
/// (preserved in their given order, including candidates that do not yet
/// exist on disk) and any EXISTING `*.db` file found directly inside
/// `root`. Every returned path is canonicalized and guaranteed to be
/// contained within `root`; a root-scan entry that is a symlink, or that
/// would resolve outside `root` (a junction/reparse point target, or an
/// escaping `..`), is silently excluded from the served set rather than
/// served. Non-`.db` files (including `*.lock`, journal/WAL sidecars, and
/// anything inside a subdirectory such as `models/`) are never candidates.
///
/// The zero-database case is represented by an empty returned `Vec` —
/// callers decide how to react (for example, exiting with a "no databases
/// found to serve" message) only when this union is empty, never when the
/// root scan alone is empty.
///
/// # Errors
///
/// Returns [`GraphtorError::PathViolation`] if one of `existing_candidates`
/// escapes `root`, or [`GraphtorError::Io`] if `root` exists but cannot be
/// read.
// Not yet called from `main.rs` — `open_serve_databases`/`cmd_serve` are
// wired to this module by P1-T3 (050.001-T), which also decides read-only
// vs. generation posture per discovered path (P1-T2). This unit lands the
// discovery/union primitive and its own test coverage as an independent,
// atomic milestone first.
#[allow(dead_code)]
pub fn discover_served_databases(
    root: &Path,
    existing_candidates: &[PathBuf],
) -> Result<Vec<PathBuf>, GraphtorError> {
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut served: Vec<PathBuf> = Vec::new();

    // Preserve existing candidates FIRST and in their given order — this is
    // the union's stability guarantee: a fresh generation target that does
    // not exist yet, or an explicit `--db-path`, is never dropped even when
    // the root scan contributes nothing.
    for candidate in existing_candidates {
        let canonical = validate_path(candidate, root)?;
        if seen.insert(canonical.clone()) {
            served.push(canonical);
        }
    }

    for discovered in scan_root_for_db_files(root)? {
        // Containment is re-validated defensively even though `discovered`
        // was just read from inside `root`: a `.db`-suffixed junction or
        // reparse point could still resolve outside `root`. Such an entry
        // is EXCLUDED from the served set rather than served.
        let Ok(canonical) = validate_path(&discovered, root) else {
            continue;
        };
        if seen.insert(canonical.clone()) {
            served.push(canonical);
        }
    }

    Ok(served)
}

/// Non-recursive scan of `root` for `*.db` files.
///
/// Skips subdirectories entirely (so `models/` and any other nested
/// directory is structurally excluded), skips symlinks (a `.db`-suffixed
/// symlink is never trusted as a served file), and skips any entry whose
/// extension is not exactly `db` (so `*.lock`, `*.db-wal`, `*.db-shm`, and
/// `*.db-journal` sidecars are never candidates). Returns an empty `Vec`
/// when `root` does not exist yet.
fn scan_root_for_db_files(root: &Path) -> Result<Vec<PathBuf>, GraphtorError> {
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut found = Vec::new();
    for entry in std::fs::read_dir(root).map_err(GraphtorError::from)? {
        let entry = entry.map_err(GraphtorError::from)?;
        let file_type = entry.file_type().map_err(GraphtorError::from)?;

        // Directories (including junctions/reparse points, which report as
        // directories) are never descended into — the scan is flat.
        if file_type.is_dir() {
            continue;
        }
        // A file-level symlink or reparse point is never trusted as a
        // served database regardless of where it points.
        if file_type.is_symlink() {
            continue;
        }

        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some(DB_EXTENSION) {
            continue;
        }

        found.push(path);
    }
    found.sort();
    Ok(found)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn temp_root() -> TempDir {
        tempfile::tempdir().expect("failed to create temp root")
    }

    fn touch(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(&path, b"stub").expect("write stub file");
        path
    }

    // ── (a) union discovery / happy path ───────────────────────────────

    #[test]
    fn discovers_a_dropped_db_with_no_existing_candidates() {
        let root = temp_root();
        let dropped = touch(root.path(), "dropped.db");

        let served = discover_served_databases(root.path(), &[])
            .expect("discovery should succeed on a plain root");

        assert_eq!(served.len(), 1);
        assert_eq!(served[0], validate_path(&dropped, root.path()).unwrap());
    }

    #[test]
    fn served_set_is_canonical_deduped_union_of_candidates_and_root_scan() {
        let root = temp_root();
        let configured_target = touch(root.path(), "configured.db");
        let dropped = touch(root.path(), "dropped.db");

        let served =
            discover_served_databases(root.path(), std::slice::from_ref(&configured_target))
                .expect("discovery should succeed");

        assert_eq!(
            served.len(),
            2,
            "expected union of 1 candidate + 1 discovered db"
        );
        let expected_configured = validate_path(&configured_target, root.path()).unwrap();
        let expected_dropped = validate_path(&dropped, root.path()).unwrap();
        assert!(served.contains(&expected_configured));
        assert!(served.contains(&expected_dropped));
    }

    #[test]
    fn same_underlying_file_referenced_twice_collapses_to_one_entry() {
        let root = temp_root();
        let db = touch(root.path(), "same.db");

        // The exact same canonical path supplied both as an "existing
        // candidate" (as `discover_db_files` would) and discoverable via
        // the root scan must collapse to a single served entry.
        let served = discover_served_databases(root.path(), std::slice::from_ref(&db))
            .expect("discovery should succeed");

        assert_eq!(
            served.len(),
            1,
            "duplicate candidate/root-scan hit must dedupe"
        );
    }

    #[test]
    fn empty_union_returns_empty_vec_not_an_error() {
        let root = temp_root();
        // Root exists but is empty and no candidates were supplied.
        let served =
            discover_served_databases(root.path(), &[]).expect("empty root is not an error");
        assert!(served.is_empty());
    }

    // ── (b) safety / containment ────────────────────────────────────────

    #[test]
    fn skips_non_db_lock_and_tmp_files() {
        let root = temp_root();
        touch(root.path(), "advisory.lock");
        touch(root.path(), "scratch.tmp");
        touch(root.path(), "readme.txt");
        let db = touch(root.path(), "real.db");

        let served = discover_served_databases(root.path(), &[]).expect("discovery should succeed");

        assert_eq!(served, vec![validate_path(&db, root.path()).unwrap()]);
    }

    #[test]
    fn skips_db_files_nested_in_a_subdirectory_including_models() {
        let root = temp_root();
        touch(root.path(), "models/all-MiniLM-L6-v2/weights.db");
        touch(root.path(), "nested/other.db");
        let top_level = touch(root.path(), "top_level.db");

        let served = discover_served_databases(root.path(), &[]).expect("discovery should succeed");

        assert_eq!(
            served,
            vec![validate_path(&top_level, root.path()).unwrap()],
            "only the top-level .graphtor/*.db entry should be discovered"
        );
    }

    #[test]
    fn skips_wal_shm_and_journal_sidecars() {
        let root = temp_root();
        let db = touch(root.path(), "sidecar.db");
        touch(root.path(), "sidecar.db-wal");
        touch(root.path(), "sidecar.db-shm");
        touch(root.path(), "sidecar.db-journal");

        let served = discover_served_databases(root.path(), &[]).expect("discovery should succeed");

        assert_eq!(served, vec![validate_path(&db, root.path()).unwrap()]);
    }

    #[test]
    fn existing_candidate_escaping_root_via_dotdot_is_rejected() {
        let root = temp_root();
        let outside = root.path().join("..").join("outside.db");

        let error = discover_served_databases(root.path(), &[outside])
            .expect_err("a candidate escaping root must be rejected");
        assert!(
            matches!(error, GraphtorError::PathViolation { .. }),
            "expected PathViolation, got: {error:?}"
        );
    }

    #[test]
    fn root_scan_directory_junction_is_never_traversed() {
        let root = temp_root();
        let external = temp_root(); // a second, independent temp dir
        touch(external.path(), "external.db");

        let junction_path = root.path().join("escape_junction");
        let status = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                junction_path.to_str().unwrap(),
                external.path().to_str().unwrap(),
            ])
            .status();

        // `mklink /J` does not require elevated privileges on Windows, but
        // skip gracefully if the platform/environment cannot create one
        // rather than failing a security test on an environment quirk.
        match status {
            Ok(s) if s.success() => {}
            _ => {
                eprintln!(
                    "skipping junction test: unable to create a junction in this environment"
                );
                return;
            }
        }

        let served = discover_served_databases(root.path(), &[]).expect("discovery should succeed");
        assert!(
            served.is_empty(),
            "a directory junction inside root must never be traversed into: {served:?}"
        );
    }

    // ── (c) compatibility / regression ──────────────────────────────────

    #[test]
    fn not_yet_created_generation_target_still_reaches_the_union() {
        let root = temp_root();
        let future_target = root.path().join("future-generation-target.db");
        assert!(!future_target.exists());

        let served = discover_served_databases(root.path(), std::slice::from_ref(&future_target))
            .expect("a not-yet-created candidate must still validate");

        assert_eq!(
            served,
            vec![validate_path(&future_target, root.path()).unwrap()],
            "a fresh source-backed workspace's not-yet-created target must reach the union, \
             not the zero-db exit"
        );
    }

    #[test]
    fn explicit_db_path_candidate_is_served_even_with_no_root_scan_hit() {
        let root = temp_root();
        let explicit = touch(root.path(), "explicit-target.db");
        // No other `.db` files exist in root — the root scan alone would
        // find exactly this one file, but the candidate must be honoured
        // regardless (characterizes the "no-config --db-path" path).
        let served = discover_served_databases(root.path(), std::slice::from_ref(&explicit))
            .expect("discovery should succeed");
        assert_eq!(served, vec![validate_path(&explicit, root.path()).unwrap()]);
    }
}
