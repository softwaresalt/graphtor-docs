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
//! [`discover_served_databases`] only ASSEMBLES the served set; deciding
//! per-database posture is [`classify_serve_postures`], and neither
//! function mutates or calls into the sync path.
//!
//! The root scan is intentionally NON-RECURSIVE: the `.graphtor/` layout is
//! flat (`bin/`, `data/`, `cache/`, `config/`, `logs/`, `models/` are all
//! direct children — see [`super::paths`]), so only files directly inside
//! the root are ever candidates. This also means the `models/` cache
//! directory (and any other subdirectory) is excluded structurally, not
//! merely by name.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use graphtor_core::config::{resolve_source_db_path, LocalSource, Source, SourceConfig};
use graphtor_core::path::validate_path;
use graphtor_core::GraphtorError;

/// The only file extension eligible for auto-discovery.
const DB_EXTENSION: &str = "db";

/// Discover the served database set.
///
/// Returns the canonical-path-deduplicated UNION of `existing_candidates`
/// (preserved in their given order, including candidates that do not yet
/// exist on disk), any explicit workspace-contained `type: database` entry
/// in `explicit_sources` (P1-T6), and any EXISTING `*.db` file found
/// directly inside `scan_root` (the `.graphtor/` workspace directory).
/// Every returned path is canonicalized; an entry that is a symlink, or
/// that would resolve outside its authorized root (a junction/reparse
/// point target, or an escaping `..`), is silently excluded from the
/// served set rather than served. Non-`.db` files (including `*.lock`,
/// journal/WAL sidecars, and anything inside a subdirectory such as
/// `models/`) are never root-scan candidates.
///
/// `existing_candidates` are validated against the BROADER `candidate_root`:
/// today's `serve`/`status` already accept an explicit `--db-path` anywhere
/// within the overall project root, not only inside `.graphtor/`
/// (characterized by
/// `serve_explicit_db_path_without_registry_reaches_v4_gate` in
/// `tests/explicit_db_target_no_registry_test.rs`), and auto-discovery must
/// not narrow that existing contract. An explicit `type: database` entry
/// (P1-T6), by contrast, is validated against `scan_root` — the SAME
/// authorized root as auto-discovery, per the LOCKED plan requirement that
/// explicit entries stay workspace-contained and MUST NOT broaden the
/// authorized root beyond what auto-discovery itself scans. A `type:
/// database` entry whose canonical `served_db_path()` equals an existing
/// candidate or an auto-discovered entry collapses to the SAME single
/// served store (canonical-path dedup) rather than opening it twice.
/// Auto-discovery itself stays strictly scoped to `scan_root` — it never
/// widens to scan the full `candidate_root` project tree, and any
/// out-of-root entry (an explicit `type: database` entry outside
/// `scan_root`, or an `existing_candidate` outside `candidate_root`) is
/// REJECTED (not served) rather than broadening the authorized root —
/// external-path support is explicitly out of Phase-1 scope.
///
/// The zero-database case is represented by an empty returned `Vec` —
/// callers decide how to react (for example, exiting with a "no databases
/// found to serve" message) only when this union is empty, never when the
/// root scan alone is empty.
///
/// # Errors
///
/// Returns [`GraphtorError::PathViolation`] if one of `existing_candidates`
/// escapes `candidate_root`, or [`GraphtorError::Io`] if `scan_root` exists
/// but cannot be read. An explicit `type: database` entry that escapes
/// `scan_root` is silently excluded rather than propagated as an error — it
/// is operator-authored workspace configuration, not a programming-error
/// candidate, so a single out-of-root entry does not abort serving every
/// other database.
pub fn discover_served_databases(
    scan_root: &Path,
    candidate_root: &Path,
    existing_candidates: &[PathBuf],
    explicit_sources: Option<&SourceConfig>,
) -> Result<Vec<PathBuf>, GraphtorError> {
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut served: Vec<PathBuf> = Vec::new();

    // Preserve existing candidates FIRST and in their given order — this is
    // the union's stability guarantee: a fresh generation target that does
    // not exist yet, or an explicit `--db-path`, is never dropped even when
    // the root scan contributes nothing.
    for candidate in existing_candidates {
        let canonical = validate_path(candidate, candidate_root)?;
        if seen.insert(canonical.clone()) {
            served.push(canonical);
        }
    }

    // Merge explicit workspace-contained `type: database` entries (P1-T6).
    // Validated against `scan_root` (`.graphtor/`) — the SAME authorized
    // root as auto-discovery, per the LOCKED plan requirement that explicit
    // entries MUST NOT broaden the authorized root beyond what
    // auto-discovery itself scans. An out-of-root path (`..`, symlink,
    // Windows junction/reparse escape, or simply a path elsewhere in the
    // project tree outside `.graphtor/`) is REJECTED — never served.
    if let Some(config) = explicit_sources {
        for source in &config.sources {
            if let Some(path) = source.served_db_path() {
                let Ok(canonical) = validate_path(path, scan_root) else {
                    continue;
                };
                if seen.insert(canonical.clone()) {
                    served.push(canonical);
                }
            }
        }
    }

    for discovered in scan_root_for_db_files(scan_root)? {
        // Containment is re-validated defensively even though `discovered`
        // was just read from inside `scan_root`: a `.db`-suffixed junction
        // or reparse point could still resolve outside `scan_root`. Such an
        // entry is EXCLUDED from the served set rather than served.
        let Ok(canonical) = validate_path(&discovered, scan_root) else {
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

// ── P1-T2: content-derived posture classification ──────────────────────────

/// Per-database serve posture, derived from CONTENT — never from a
/// hardcoded path, an environment variable, or a hand-set flag.
///
/// See [`classify_serve_postures`] for the three-way classification rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServeMode {
    /// No resolvable real generation source targets this database (absent,
    /// empty, or stale `sources.yaml`, or an unrelated co-resident dropped
    /// db): served read-only, never background-synced. The fail-safe
    /// default on any ambiguity.
    ReadOnly,
    /// A `local` source with a real, existing, non-empty target directory
    /// resolves its target database to this path: full read-write
    /// generate-and-serve behaviour is retained.
    Generation,
}

/// Result of [`classify_serve_postures`]: the per-database posture
/// assignment, plus the FILTERED subset of `sources.yaml` entries whose
/// resolved target reached [`ServeMode::Generation`].
///
/// `generation_sources` is deliberately NEVER the full [`SourceConfig`] —
/// callers (the write-path preflight and `spawn_background_sync`) must
/// receive only this filtered subset, so a stale or read-only-classified
/// source group can never be re-split into a background write (INV-7).
#[derive(Debug, Clone, PartialEq)]
pub struct ClassifiedServeSet {
    /// Every database from the `served` union, in the same order, paired
    /// with its resolved posture.
    pub postures: Vec<(PathBuf, ServeMode)>,
    /// Only the `sources.yaml` entries whose resolved target database is
    /// `Generation` — never the full, unfiltered source list.
    pub generation_sources: Vec<Source>,
}

/// Classify every database in `served` into a per-database [`ServeMode`]
/// using a three-way, fail-safe rule:
///
/// 1. `source_config` is `None` (absent registry, or the no-config
///    `--db-path` path) — every database stays [`ServeMode::ReadOnly`].
/// 2. A `local` source whose `path` exists AND contains at least one file
///    matching its configured `formats`/`include`/`exclude` promotes ONLY
///    the database whose resolved (canonicalized) target equals that
///    source's target to [`ServeMode::Generation`] — a resolvable source
///    NEVER promotes an unrelated co-resident dropped database.
/// 3. Every other database (absent/empty/stale/unresolvable source, or no
///    source targets it at all) stays [`ServeMode::ReadOnly`].
///
/// Malformed/unparseable `sources.yaml` is NOT this function's concern —
/// that fails closed with a hard [`GraphtorError`] upstream, at parse time
/// (`SourceConfig::parse`/`load_source_config`), before a caller ever has an
/// `Option<&SourceConfig>` to pass in here; this function only ever sees
/// the already-validated `Some(config)` or the `None`/absent case.
#[must_use]
pub fn classify_serve_postures(
    served: &[PathBuf],
    source_config: Option<&SourceConfig>,
    base_db_path: &Path,
    root: &Path,
) -> ClassifiedServeSet {
    let mut generation_targets: BTreeMap<PathBuf, Vec<Source>> = BTreeMap::new();

    if let Some(config) = source_config {
        for source in &config.sources {
            let Some(local) = source.as_local() else {
                continue;
            };
            // Validate the source's content path against the SAME
            // authorized root the real background acquisition plan
            // enforces (`acquire::plan::plan`/`validate_sources` both
            // validate `local.path` against this identical `root`/
            // `allowed_root` value) BEFORE trusting its on-disk state. An
            // out-of-root `local.path` must never promote its target
            // database to `Generation` (read-write) here — even though
            // the real sync pipeline would separately reject the same
            // path, that later rejection must not be preceded by a
            // read-write open decided from unvalidated filesystem state.
            if validate_path(&local.path, root).is_err() {
                continue;
            }
            if !local.path.exists() || !source_has_ingestible_content(local) {
                // Absent, empty, or stale — this source resolves no
                // database to `Generation`; fail-safe default applies.
                continue;
            }

            let target = resolve_source_db_path(base_db_path, source);
            // Canonicalize the target the SAME way `served` entries were
            // canonicalized (`validate_path` against the same `root`) so
            // both sides compare equal. Fail-safe: if the target cannot be
            // validated for any reason, this source promotes nothing
            // rather than risking an incorrect `Generation` classification.
            if let Ok(canonical_target) = validate_path(&target, root) {
                generation_targets
                    .entry(canonical_target)
                    .or_default()
                    .push(source.clone());
            }
        }
    }

    let mut postures = Vec::with_capacity(served.len());
    let mut generation_sources = Vec::new();
    for db_path in served {
        if let Some(sources) = generation_targets.get(db_path) {
            postures.push((db_path.clone(), ServeMode::Generation));
            generation_sources.extend(sources.iter().cloned());
        } else {
            postures.push((db_path.clone(), ServeMode::ReadOnly));
        }
    }

    ClassifiedServeSet {
        postures,
        generation_sources,
    }
}

/// Returns `true` when `local`'s directory exists and recursively contains
/// at least one file that would actually be ingested: its extension
/// matches one of `local`'s configured `formats` (honoring the
/// `"markdown"` → `md` alias) AND it survives `local`'s `include`/`exclude`
/// glob filters. Read-only — never creates or modifies anything, unlike
/// the full acquisition `plan`/`execute` pipeline (which would create
/// `data_root` as a side effect).
fn source_has_ingestible_content(local: &LocalSource) -> bool {
    if !local.path.is_dir() {
        return false;
    }

    let mut relative_candidates: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(&local.path)
        .into_iter()
        .filter_map(Result::ok)
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(extension) = entry.path().extension().and_then(|ext| ext.to_str()) else {
            continue;
        };
        let matches_format = local
            .formats
            .iter()
            .any(|fmt| canonicalize_format_alias(fmt).eq_ignore_ascii_case(extension));
        if !matches_format {
            continue;
        }
        if let Ok(relative) = entry.path().strip_prefix(&local.path) {
            relative_candidates.push(relative.to_path_buf());
        }
    }

    if relative_candidates.is_empty() {
        return false;
    }

    graphtor_core::acquire::filter_files(&relative_candidates, &local.include, &local.exclude)
        .is_ok_and(|filtered| !filtered.is_empty())
}

/// Canonicalize a configured format alias to its canonical file extension.
///
/// Mirrors `crate::config::source::canonicalize_format_ext`, which is
/// `pub(crate)` to the library crate and therefore not reachable from this
/// binary-crate module; the mapping itself is a single, stable alias
/// (`"markdown"` → `"md"`) and is trivial to keep in sync.
fn canonicalize_format_alias(fmt: &str) -> &str {
    if fmt.eq_ignore_ascii_case("markdown") {
        "md"
    } else {
        fmt
    }
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

        let served = discover_served_databases(root.path(), root.path(), &[], None)
            .expect("discovery should succeed on a plain root");

        assert_eq!(served.len(), 1);
        assert_eq!(served[0], validate_path(&dropped, root.path()).unwrap());
    }

    #[test]
    fn served_set_is_canonical_deduped_union_of_candidates_and_root_scan() {
        let root = temp_root();
        let configured_target = touch(root.path(), "configured.db");
        let dropped = touch(root.path(), "dropped.db");

        let served = discover_served_databases(
            root.path(),
            root.path(),
            std::slice::from_ref(&configured_target),
            None,
        )
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
        let served =
            discover_served_databases(root.path(), root.path(), std::slice::from_ref(&db), None)
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
        let served = discover_served_databases(root.path(), root.path(), &[], None)
            .expect("empty root is not an error");
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

        let served = discover_served_databases(root.path(), root.path(), &[], None)
            .expect("discovery should succeed");

        assert_eq!(served, vec![validate_path(&db, root.path()).unwrap()]);
    }

    #[test]
    fn skips_db_files_nested_in_a_subdirectory_including_models() {
        let root = temp_root();
        touch(root.path(), "models/all-MiniLM-L6-v2/weights.db");
        touch(root.path(), "nested/other.db");
        let top_level = touch(root.path(), "top_level.db");

        let served = discover_served_databases(root.path(), root.path(), &[], None)
            .expect("discovery should succeed");

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

        let served = discover_served_databases(root.path(), root.path(), &[], None)
            .expect("discovery should succeed");

        assert_eq!(served, vec![validate_path(&db, root.path()).unwrap()]);
    }

    #[test]
    fn existing_candidate_escaping_root_via_dotdot_is_rejected() {
        let root = temp_root();
        let outside = root.path().join("..").join("outside.db");

        let error = discover_served_databases(root.path(), root.path(), &[outside], None)
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

        let served = discover_served_databases(root.path(), root.path(), &[], None)
            .expect("discovery should succeed");
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

        let served = discover_served_databases(
            root.path(),
            root.path(),
            std::slice::from_ref(&future_target),
            None,
        )
        .expect("a not-yet-created candidate must still validate");

        assert_eq!(
            served,
            vec![validate_path(&future_target, root.path()).unwrap()],
            "a fresh source-backed workspace's not-yet-created target must reach the union, \
             not the zero-db exit"
        );
    }

    #[test]
    fn explicit_db_path_outside_graphtor_root_but_within_project_root_is_honoured() {
        // Characterizes `serve_explicit_db_path_without_registry_reaches_v4_gate`
        // (tests/explicit_db_target_no_registry_test.rs): an explicit
        // `--db-path` may live anywhere under the project root, not only
        // inside `.graphtor/`. Auto-discovery must still validate it against
        // the broader `candidate_root`, not the narrower `scan_root`.
        let project_root = temp_root();
        let graphtor_dir = project_root.path().join(".graphtor");
        fs::create_dir_all(&graphtor_dir).unwrap();
        let explicit_outside_graphtor = touch(project_root.path(), "explicit-pre-v4.db");

        let served = discover_served_databases(
            &graphtor_dir,
            project_root.path(),
            std::slice::from_ref(&explicit_outside_graphtor),
            None,
        )
        .expect(
            "an explicit candidate outside .graphtor/ but inside the project root must validate",
        );

        assert_eq!(
            served,
            vec![validate_path(&explicit_outside_graphtor, project_root.path()).unwrap()],
        );
    }

    #[test]
    fn explicit_db_path_candidate_is_served_even_with_no_root_scan_hit() {
        let root = temp_root();
        let explicit = touch(root.path(), "explicit-target.db");
        // No other `.db` files exist in root — the root scan alone would
        // find exactly this one file, but the candidate must be honoured
        // regardless (characterizes the "no-config --db-path" path).
        let served = discover_served_databases(
            root.path(),
            root.path(),
            std::slice::from_ref(&explicit),
            None,
        )
        .expect("discovery should succeed");
        assert_eq!(served, vec![validate_path(&explicit, root.path()).unwrap()]);
    }

    // ── P1-T6: explicit `type: database` entry merge ────────────────────

    fn database_source(id: &str, path: &Path) -> Source {
        Source::Database(graphtor_core::DatabaseSource {
            id: id.to_string(),
            path: path.to_path_buf(),
        })
    }

    #[test]
    fn explicit_database_entry_is_merged_into_the_served_union() {
        let root = temp_root();
        let db = touch(root.path(), "legacy.db");
        let config = config_with(vec![database_source("legacy", &db)]);

        let served = discover_served_databases(root.path(), root.path(), &[], Some(&config))
            .expect("discovery should succeed");

        assert_eq!(served, vec![validate_path(&db, root.path()).unwrap()]);
    }

    #[test]
    fn explicit_database_entry_matching_an_auto_discovered_file_collapses_to_one_entry() {
        let root = temp_root();
        let db = touch(root.path(), "shared.db");
        // The SAME underlying file is BOTH auto-discoverable (dropped in
        // root) AND explicitly declared — canonical-path dedup must
        // collapse these to one served store, not two.
        let config = config_with(vec![database_source("shared-alias", &db)]);

        let served = discover_served_databases(root.path(), root.path(), &[], Some(&config))
            .expect("discovery should succeed");

        assert_eq!(
            served.len(),
            1,
            "explicit entry + auto-discovery for the same file must collapse"
        );
    }

    #[test]
    fn explicit_database_entry_escaping_root_via_dotdot_is_rejected_not_served() {
        let root = temp_root();
        let outside = root.path().join("..").join("outside.db");
        let config = config_with(vec![database_source("escaping", &outside)]);

        // Unlike an escaping `existing_candidate` (which is a hard Err —
        // upstream code produced it), an operator-authored explicit entry
        // that escapes the root is silently excluded so a single bad entry
        // does not abort serving every other database.
        let served = discover_served_databases(root.path(), root.path(), &[], Some(&config))
            .expect("discovery must not hard-fail on an escaping explicit entry");
        assert!(
            served.is_empty(),
            "an out-of-root explicit entry must never be served"
        );
    }

    #[test]
    fn explicit_database_entry_outside_graphtor_but_inside_project_root_is_rejected() {
        // LOCKED plan requirement (P1-T6): an explicit `type: database` entry
        // must stay within the SAME authorized root as auto-discovery
        // (`.graphtor/` — `scan_root`), not merely the broader project root
        // (`candidate_root`) that `existing_candidates`/`--db-path` are
        // allowed to use. A db file dropped in the project root but OUTSIDE
        // `.graphtor/` must be rejected, never served, even though it is
        // still fully within `candidate_root`.
        let project_root = temp_root();
        let scan_root = project_root.path().join(".graphtor");
        fs::create_dir_all(&scan_root).expect("create .graphtor");
        let outside_graphtor_db = touch(project_root.path(), "outside.db");
        let config = config_with(vec![database_source("outside-alias", &outside_graphtor_db)]);

        let served = discover_served_databases(&scan_root, project_root.path(), &[], Some(&config))
            .expect("discovery must not hard-fail on a rejected explicit entry");

        assert!(
            served.is_empty(),
            "an explicit type: database entry outside .graphtor/ (but inside the project \
             root) must be rejected — it must stay within the same authorized root as \
             auto-discovery, not the broader project root"
        );
    }

    #[test]
    fn explicit_database_entry_via_windows_junction_is_rejected_not_served() {
        let root = temp_root();
        let external = temp_root();
        let external_db = touch(external.path(), "external.db");

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
        match status {
            Ok(s) if s.success() => {}
            _ => {
                eprintln!(
                    "skipping junction test: unable to create a junction in this environment"
                );
                return;
            }
        }

        // The explicit entry's path traverses THROUGH the in-root junction
        // to reach a file that canonicalizes outside `root` — rejected.
        let escaping_path = junction_path.join(external_db.file_name().unwrap());
        let config = config_with(vec![database_source("via-junction", &escaping_path)]);

        let served = discover_served_databases(root.path(), root.path(), &[], Some(&config))
            .expect("discovery must not hard-fail on a junction-escaping explicit entry");
        assert!(
            served.is_empty(),
            "an explicit entry resolving outside root via a junction must never be served"
        );
    }

    #[test]
    fn explicit_database_entry_is_always_classified_read_only() {
        let root = temp_root();
        let db = touch(root.path(), "legacy.db");
        let config = config_with(vec![database_source("legacy", &db)]);

        let served = discover_served_databases(root.path(), root.path(), &[], Some(&config))
            .expect("discovery should succeed");
        let classified = classify_serve_postures(&served, Some(&config), &db, root.path());

        assert_eq!(
            classified.postures,
            vec![(served[0].clone(), ServeMode::ReadOnly)],
            "an explicit type: database entry is inherently read-only and never Generation"
        );
        assert!(classified.generation_sources.is_empty());
    }

    #[test]
    fn mixed_local_and_database_config_serves_database_read_only_via_served_db_path() {
        let root = temp_root();
        let docs_dir = root.path().join("docs");
        touch(&docs_dir, "guide.md");
        let generation_db = root.path().join("graph.db");
        let legacy_db = touch(root.path(), "legacy.db");

        let config = config_with(vec![
            local_source("docs", &docs_dir, None),
            database_source("legacy", &legacy_db),
        ]);

        let served = discover_served_databases(
            root.path(),
            root.path(),
            std::slice::from_ref(&generation_db),
            Some(&config),
        )
        .expect("discovery should succeed");
        assert_eq!(
            served.len(),
            2,
            "expected the generation target + the explicit database entry"
        );

        let classified =
            classify_serve_postures(&served, Some(&config), &generation_db, root.path());
        let legacy_path = validate_path(&legacy_db, root.path()).unwrap();
        let mode_of_legacy = classified
            .postures
            .iter()
            .find(|(p, _)| *p == legacy_path)
            .map(|(_, mode)| *mode)
            .unwrap();
        assert_eq!(mode_of_legacy, ServeMode::ReadOnly);
    }

    // ── P1-T2: classify_serve_postures ──────────────────────────────────

    fn local_source(id: &str, path: &Path, database: Option<&str>) -> Source {
        Source::Local(LocalSource {
            id: id.to_string(),
            path: path.to_path_buf(),
            include: vec![],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: database.map(str::to_string),
        })
    }

    fn config_with(sources: Vec<Source>) -> SourceConfig {
        SourceConfig { sources }
    }

    #[test]
    fn no_source_config_stays_read_only() {
        let root = temp_root();
        let db = touch(root.path(), "graph.db");
        let served = vec![validate_path(&db, root.path()).unwrap()];

        let classified = classify_serve_postures(&served, None, &db, root.path());

        assert_eq!(
            classified.postures,
            vec![(served[0].clone(), ServeMode::ReadOnly)]
        );
        assert!(classified.generation_sources.is_empty());
    }

    #[test]
    fn real_non_empty_source_promotes_its_target_to_generation() {
        let root = temp_root();
        let docs_dir = root.path().join("docs");
        touch(&docs_dir, "guide.md");
        let db = root.path().join("graph.db");
        // `graph.db` need not exist yet for a fresh generation workspace.
        let served = vec![validate_path(&db, root.path()).unwrap()];
        let config = config_with(vec![local_source("docs", &docs_dir, None)]);

        let classified = classify_serve_postures(&served, Some(&config), &db, root.path());

        assert_eq!(
            classified.postures,
            vec![(served[0].clone(), ServeMode::Generation)]
        );
        assert_eq!(classified.generation_sources.len(), 1);
    }

    #[test]
    fn local_source_path_outside_root_never_promotes_to_generation() {
        // A `local` source whose content `path` escapes `root` must never
        // promote its target to `Generation`, even when that external
        // directory genuinely exists and has ingestible content — the same
        // containment boundary the real background acquisition plan
        // (`acquire::plan::validate_sources`) enforces on `local.path` must
        // also gate this posture decision, not just the eventual sync.
        let root = temp_root();
        let outside = temp_root();
        touch(outside.path(), "guide.md");
        let db = root.path().join("graph.db");
        let served = vec![validate_path(&db, root.path()).unwrap()];
        let config = config_with(vec![local_source("docs", outside.path(), None)]);

        let classified = classify_serve_postures(&served, Some(&config), &db, root.path());

        assert_eq!(
            classified.postures,
            vec![(served[0].clone(), ServeMode::ReadOnly)],
            "an out-of-root local source path must never grant Generation posture"
        );
        assert!(
            classified.generation_sources.is_empty(),
            "an out-of-root source must not be surfaced as a generation source either"
        );
    }

    #[test]
    fn existing_but_empty_source_path_stays_read_only() {
        let root = temp_root();
        let docs_dir = root.path().join("empty-docs");
        fs::create_dir_all(&docs_dir).unwrap();
        let db = touch(root.path(), "graph.db");
        let served = vec![validate_path(&db, root.path()).unwrap()];
        let config = config_with(vec![local_source("docs", &docs_dir, None)]);

        let classified = classify_serve_postures(&served, Some(&config), &db, root.path());

        assert_eq!(
            classified.postures,
            vec![(served[0].clone(), ServeMode::ReadOnly)]
        );
        assert!(classified.generation_sources.is_empty());
    }

    #[test]
    fn stale_source_path_that_does_not_exist_stays_read_only() {
        let root = temp_root();
        let missing_dir = root.path().join("never-existed");
        let db = touch(root.path(), "graph.db");
        let served = vec![validate_path(&db, root.path()).unwrap()];
        let config = config_with(vec![local_source("docs", &missing_dir, None)]);

        let classified = classify_serve_postures(&served, Some(&config), &db, root.path());

        assert_eq!(
            classified.postures,
            vec![(served[0].clone(), ServeMode::ReadOnly)]
        );
    }

    #[test]
    fn source_backed_target_and_co_resident_dropped_db_classified_independently() {
        let root = temp_root();
        let docs_dir = root.path().join("docs");
        touch(&docs_dir, "guide.md");
        let generation_db = root.path().join("graph.db");
        let dropped_db = touch(root.path(), "dropped.db");

        let served = discover_served_databases(
            root.path(),
            root.path(),
            std::slice::from_ref(&generation_db),
            None,
        )
        .expect("union should include both the configured target and the dropped db");
        assert_eq!(
            served.len(),
            2,
            "expected both the target and the dropped db in the union"
        );

        let config = config_with(vec![local_source("docs", &docs_dir, None)]);
        let classified =
            classify_serve_postures(&served, Some(&config), &generation_db, root.path());

        let generation_path = validate_path(&generation_db, root.path()).unwrap();
        let dropped_path = validate_path(&dropped_db, root.path()).unwrap();
        let mode_of = |p: &PathBuf| {
            classified
                .postures
                .iter()
                .find(|(path, _)| path == p)
                .map(|(_, mode)| *mode)
                .unwrap()
        };
        assert_eq!(mode_of(&generation_path), ServeMode::Generation);
        assert_eq!(
            mode_of(&dropped_path),
            ServeMode::ReadOnly,
            "an unrelated co-resident dropped db must never be promoted by an unrelated source"
        );
    }

    #[test]
    fn mixed_valid_and_stale_sources_targeting_different_dbs_only_returns_the_valid_groups() {
        let root = temp_root();
        let valid_docs = root.path().join("valid-docs");
        touch(&valid_docs, "guide.md");
        let stale_docs = root.path().join("stale-docs-that-does-not-exist");

        let valid_db = root.path().join("valid.db");
        let stale_db = root.path().join("stale.db");
        let base = root.path().join("graph.db");

        let served = vec![
            validate_path(&valid_db, root.path()).unwrap(),
            validate_path(&stale_db, root.path()).unwrap(),
        ];
        let config = config_with(vec![
            local_source("valid-src", &valid_docs, Some("valid.db")),
            local_source("stale-src", &stale_docs, Some("stale.db")),
        ]);

        let classified = classify_serve_postures(&served, Some(&config), &base, root.path());

        assert_eq!(
            classified.generation_sources.len(),
            1,
            "only the valid source group must be returned"
        );
        let only = classified.generation_sources[0]
            .as_local()
            .expect("generation source is local");
        assert_eq!(only.id, "valid-src");
    }

    #[test]
    fn source_with_include_filtered_to_zero_files_stays_read_only() {
        let root = temp_root();
        let docs_dir = root.path().join("docs");
        touch(&docs_dir, "guide.md");
        let db = touch(root.path(), "graph.db");
        let served = vec![validate_path(&db, root.path()).unwrap()];
        let source = Source::Local(LocalSource {
            id: "docs".to_string(),
            path: docs_dir.clone(),
            include: vec!["no-match-*.md".to_string()],
            exclude: vec![],
            formats: vec!["md".to_string()],
            database: None,
        });
        let config = config_with(vec![source]);

        let classified = classify_serve_postures(&served, Some(&config), &db, root.path());

        assert_eq!(
            classified.postures,
            vec![(served[0].clone(), ServeMode::ReadOnly)],
            "an include filter that matches nothing must not promote the target to Generation"
        );
    }

    #[test]
    fn malformed_sources_yaml_is_a_hard_error_upstream_of_classification() {
        // Characterizes the "(unchanged behaviour)" fail-closed contract:
        // classification is never reached for malformed YAML because
        // parsing itself already fails closed.
        let malformed = "sources:\n  - type: local\n    id: broken\n    path: [unterminated\n";
        let result: Result<SourceConfig, _> = serde_yaml::from_str(malformed);
        assert!(result.is_err(), "malformed sources.yaml must fail to parse");
    }
}
