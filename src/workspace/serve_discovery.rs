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
use graphtor_core::path::{is_reparse_point, validate_path};
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
/// Every returned path is canonicalized. An AUTO-DISCOVERED root-scan entry
/// that is a symlink, or that would resolve outside its authorized root (a
/// junction/reparse point target, or an escaping `..`), is silently excluded
/// from the served set rather than served; an EXPLICIT `type: database`
/// entry that fails the same containment check instead PROPAGATES a
/// validation error (see Errors below). Non-`.db` files (including `*.lock`,
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
/// widens to scan the full `candidate_root` project tree. Any out-of-root
/// EXPLICIT `type: database` entry (outside `scan_root`) is REJECTED by
/// PROPAGATING the validation error (050.006-T (b)) rather than being
/// silently dropped — an operator-configured path that escapes containment
/// is invalid config that must surface, not hide behind a misleading "no
/// databases found". An `existing_candidate` outside `candidate_root` is
/// likewise a hard error. Only the opportunistic AUTO-DISCOVERY root scan
/// silently skips an out-of-root `.db`-suffixed junction. External-path
/// support is explicitly out of Phase-1 scope.
///
/// The zero-database case is represented by an empty returned `Vec` —
/// callers decide how to react (for example, exiting with a "no databases
/// found to serve" message) only when this union is empty, never when the
/// root scan alone is empty.
///
/// # Errors
///
/// Returns [`GraphtorError::PathViolation`] if one of `existing_candidates`
/// escapes `candidate_root`, or if an EXPLICIT `type: database` entry
/// escapes `scan_root` — an operator-authored path that fails containment is
/// invalid config and is surfaced, not silently dropped (050.006-T (b)).
/// Returns [`GraphtorError::Io`] if `scan_root` exists but cannot be read.
/// Note the AUTO-DISCOVERY root scan, by contrast, silently skips an
/// out-of-root `.db`-suffixed junction/reparse point rather than erroring —
/// an opportunistic scan hit is not operator-declared config.
pub fn discover_served_databases(
    scan_root: &Path,
    candidate_root: &Path,
    existing_candidates: &[PathBuf],
    explicit_sources: Option<&SourceConfig>,
) -> Result<Vec<PathBuf>, GraphtorError> {
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut served: Vec<PathBuf> = Vec::new();

    // Workspace-containment guard (Constitution III/IV): the `.graphtor`
    // `scan_root` is its OWN trust anchor for the flat auto-discovery scan and
    // the explicit `type: database` containment check below. If that root is a
    // symlink/junction — or otherwise resolves outside the project
    // `candidate_root` — the scan (and the subsequent read-only permission
    // normalization in `open_engine_readonly`) would reach files OUTSIDE the
    // workspace. Reject such a root before any read. A missing root is fine:
    // no scan happens and only preserved existing candidates are returned.
    if scan_root.exists() {
        if is_reparse_point(scan_root) {
            return Err(GraphtorError::PathViolation {
                attempted: scan_root.to_path_buf(),
                allowed_root: candidate_root.to_path_buf(),
            });
        }
        validate_path(scan_root, candidate_root)?;
    }

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
    // project tree outside `.graphtor/`) is REJECTED by PROPAGATING the
    // validation error (050.006-T (b)) — an EXPLICIT operator-configured
    // path is a config error that must surface, not be silently dropped
    // into a misleading "no databases found".
    if let Some(config) = explicit_sources {
        for source in &config.sources {
            if let Some(path) = source.served_db_path() {
                let canonical = validate_path(path, scan_root)?;
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
///
/// Streams the boolean over the walk via [`stream_ingestible`] instead of
/// accumulating a `Vec` of every matching relative path — memory no longer
/// scales with document count. The entire `WalkDir` is still traversed and
/// any walk error still fails closed to `false` (fail-closed, unchanged):
/// this function is only a memory optimization, never a traversal
/// short-circuit. Reuses the SAME compiled include/exclude matcher
/// `graphtor_core::acquire::filter_files` uses (via
/// [`graphtor_core::acquire::FileFilter`]) so classification stays
/// identical to the pre-refactor batch behavior. An invalid include/exclude
/// glob pattern fails closed to `false`, matching the pre-refactor
/// `filter_files(...).is_ok_and(...)` behavior.
fn source_has_ingestible_content(local: &LocalSource) -> bool {
    if !local.path.is_dir() {
        return false;
    }

    let Ok(matcher) = graphtor_core::acquire::FileFilter::new(&local.include, &local.exclude)
    else {
        return false;
    };

    let steps = walkdir::WalkDir::new(&local.path).into_iter().map(|entry| {
        entry.map(|e| {
            // Fail closed on any walk error (mapped as `Err` and propagated
            // unchanged below). An unreadable subtree — or an entry that
            // vanished mid-walk — means the REAL acquisition walk (which
            // propagates `WalkDir` errors, see `graphtor_core::acquire::local`)
            // would also fail: `stream_ingestible` returns `false` for the
            // whole call on the first `Err`, regardless of any earlier
            // eligible candidate.
            if !e.file_type().is_file() {
                return None;
            }
            let extension = e.path().extension().and_then(|ext| ext.to_str())?;
            let matches_format = local
                .formats
                .iter()
                .any(|fmt| canonicalize_format_alias(fmt).eq_ignore_ascii_case(extension));
            if !matches_format {
                return None;
            }
            e.path()
                .strip_prefix(&local.path)
                .ok()
                .map(Path::to_path_buf)
        })
    });

    stream_ingestible(steps, &matcher)
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

/// Stream an "has ingestible content" boolean over a sequence of walk
/// steps without accumulating a `Vec` of matching paths — O(1) additional
/// memory beyond a `found` flag and a format-candidate counter, regardless
/// of how many files the walk visits.
///
/// Each step is `Ok(Some(relative_path))` for a format-matching candidate
/// file, `Ok(None)` for any other walk entry (a directory, a non-matching
/// extension, or a file whose relative path could not be computed), or
/// `Err` for a walk error. This abstraction decouples the aggregation
/// logic from `walkdir::DirEntry` (which cannot be constructed outside the
/// `walkdir` crate) so tests can drive an explicit, deterministic sequence
/// — including a walk error at a specific position — without depending on
/// real, Unix-only, racy-under-CI filesystem permission tricks.
///
/// Returns `false` immediately on the first `Err` step (fail-closed — an
/// unreadable subtree anywhere in the walk means the real acquisition walk,
/// which also propagates `WalkDir` errors, would fail too). A traversal
/// short-circuit that returns `true` on the first eligible candidate is
/// deliberately REJECTED: it would skip a later walk error and could
/// incorrectly promote a partially-unreadable source's target database to
/// the read-write `Generation` posture.
///
/// Emits the same aggregate "all files excluded" warning `filter_files`
/// emits for a fully materialized batch — under the same `input_files`
/// field name, carrying the scalar format-candidate count — exactly once,
/// and only when at least one format-matching candidate was observed but
/// none of them passed `matcher`.
fn stream_ingestible<I, E>(steps: I, matcher: &graphtor_core::acquire::FileFilter) -> bool
where
    I: IntoIterator<Item = Result<Option<PathBuf>, E>>,
{
    let mut found = false;
    let mut format_candidate_count: usize = 0;

    for step in steps {
        let Ok(candidate) = step else {
            return false;
        };
        let Some(relative_path) = candidate else {
            continue;
        };
        format_candidate_count += 1;
        if matcher.is_match(&relative_path) {
            found = true;
        }
    }

    // Parity with filter_files's own S032 warning: emit the SAME message
    // and the SAME `input_files` field name, carrying the scalar
    // format-candidate count (not a per-file Vec) — exactly once, and only
    // when candidates existed but every one was excluded.
    if format_candidate_count > 0 && !found {
        tracing::warn!(
            input_files = format_candidate_count,
            "filter produced empty file set — all files were excluded"
        );
    }

    found
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

        // Per 050.006-T (b): an EXPLICIT operator-configured `type: database`
        // path that escapes the authorized root must PROPAGATE the validation
        // error, not be silently dropped — silently ignoring it would surface a
        // misleading "no databases found" while hiding invalid config.
        let err = discover_served_databases(root.path(), root.path(), &[], Some(&config))
            .expect_err("an out-of-root explicit entry must surface a validation error");
        assert!(
            matches!(err, GraphtorError::PathViolation { .. }),
            "expected a PathViolation for the escaping explicit entry, got: {err:?}"
        );
    }

    #[test]
    fn explicit_database_entry_outside_graphtor_but_inside_project_root_is_rejected() {
        // LOCKED plan requirement (P1-T6): an explicit `type: database` entry
        // must stay within the SAME authorized root as auto-discovery
        // (`.graphtor/` — `scan_root`), not merely the broader project root
        // (`candidate_root`) that `existing_candidates`/`--db-path` are
        // allowed to use. A db file dropped in the project root but OUTSIDE
        // `.graphtor/` must be rejected with a validation error (050.006-T (b)),
        // never silently ignored, even though it is still fully within
        // `candidate_root`.
        let project_root = temp_root();
        let scan_root = project_root.path().join(".graphtor");
        fs::create_dir_all(&scan_root).expect("create .graphtor");
        let outside_graphtor_db = touch(project_root.path(), "outside.db");
        let config = config_with(vec![database_source("outside-alias", &outside_graphtor_db)]);

        let err = discover_served_databases(&scan_root, project_root.path(), &[], Some(&config))
            .expect_err(
                "an explicit type: database entry outside .graphtor/ (but inside the project \
                 root) must be rejected with a validation error — it must stay within the same \
                 authorized root as auto-discovery, not the broader project root",
            );
        assert!(
            matches!(err, GraphtorError::PathViolation { .. }),
            "expected a PathViolation for the out-of-root explicit entry, got: {err:?}"
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
        // to reach a file that canonicalizes outside `root` — rejected with a
        // validation error (050.006-T (b)), never silently dropped.
        let escaping_path = junction_path.join(external_db.file_name().unwrap());
        let config = config_with(vec![database_source("via-junction", &escaping_path)]);

        let err = discover_served_databases(root.path(), root.path(), &[], Some(&config))
            .expect_err(
                "an explicit entry resolving outside root via a junction must surface a \
                 validation error",
            );
        assert!(
            matches!(err, GraphtorError::PathViolation { .. }),
            "expected a PathViolation for the junction-escaping explicit entry, got: {err:?}"
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

    #[cfg(unix)]
    #[test]
    fn source_with_an_unreadable_subtree_stays_read_only() {
        // A readable matching file exists at the top level, but a subtree is
        // unreadable so the walk yields an error. The real acquisition walk
        // would fail on that subtree, so posture classification must fail
        // closed to ReadOnly rather than opening the target read-write on the
        // strength of the readable file.
        use std::os::unix::fs::PermissionsExt as _;

        let root = temp_root();
        let docs_dir = root.path().join("docs");
        touch(&docs_dir, "guide.md");
        let locked = docs_dir.join("locked");
        fs::create_dir_all(&locked).expect("create locked subdir");
        touch(&locked, "inner.md");
        // Drop read+exec so descending into `locked` yields a WalkDir error.
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).expect("chmod 000");

        // If the environment does not enforce the 0o000 restriction (e.g. the
        // test runs as root in a CI container, where DAC is bypassed), the walk
        // would NOT error and this test cannot exercise the fail-closed path.
        // Probe with a read_dir and self-skip rather than emit a false failure.
        if fs::read_dir(&locked).is_ok() {
            let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o755));
            return;
        }

        let db = touch(root.path(), "graph.db");
        let served = vec![validate_path(&db, root.path()).unwrap()];
        let source = local_source("docs", &docs_dir, None);
        let config = config_with(vec![source]);

        let classified = classify_serve_postures(&served, Some(&config), &db, root.path());

        // Restore perms so TempDir cleanup can remove the tree.
        let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o755));

        assert_eq!(
            classified.postures,
            vec![(served[0].clone(), ServeMode::ReadOnly)],
            "an unreadable subtree must fail closed to ReadOnly, matching the acquisition walk"
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

    // ── workspace containment: symlinked `.graphtor` scan root (X1) ─────────

    /// Create a directory symlink cross-platform, returning `Err` when the
    /// platform refuses (e.g. Windows without the symlink privilege) so the
    /// caller can self-skip rather than fail.
    fn try_symlink_dir(target: &Path, link: &Path) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(target, link)
        }
    }

    #[test]
    fn symlinked_graphtor_scan_root_pointing_outside_project_is_rejected() {
        // A `.graphtor` root that is a symlink/junction pointing OUTSIDE the
        // project is its own trust anchor for the flat scan, so scanning it and
        // normalizing permissions on what it finds would reach external files.
        // Discovery must reject such a root before scanning (Constitution
        // III/IV), not silently serve an external database.
        let project = temp_root();
        let external = temp_root();
        // A `.db` sitting in the external target the link points at.
        touch(external.path(), "escaped.db");

        let scan_root = project.path().join(".graphtor");
        if try_symlink_dir(external.path(), &scan_root).is_err() {
            return; // platform refused symlink creation — skip
        }

        let result = discover_served_databases(&scan_root, project.path(), &[], None);
        assert!(
            matches!(result, Err(GraphtorError::PathViolation { .. })),
            "a symlinked .graphtor root escaping the project must be rejected: {result:?}"
        );
    }

    #[test]
    fn symlinked_graphtor_scan_root_pointing_inside_project_is_still_rejected() {
        // Isolation guard for the `is_reparse_point` check: a `.graphtor` root
        // that is a symlink/junction whose target is INSIDE the project would
        // PASS `validate_path` (canonicalization stays within candidate_root),
        // so ONLY the `is_reparse_point` guard rejects it. This test fails if
        // that guard line is ever dropped, unlike the outside-pointing case
        // which `validate_path` also rejects.
        let project = temp_root();
        // A real in-project directory the link will point at, holding a `.db`.
        let real_dir = project.path().join(".graphtor_real");
        fs::create_dir_all(&real_dir).expect("create in-project real dir");
        touch(&real_dir, "inside.db");

        let scan_root = project.path().join(".graphtor");
        if try_symlink_dir(&real_dir, &scan_root).is_err() {
            return; // platform refused symlink creation — skip
        }

        // Precondition: the link target is inside the project, so `validate_path`
        // alone accepts it — proving this case isolates the reparse-point guard.
        assert!(
            validate_path(&scan_root, project.path()).is_ok(),
            "precondition: an in-project symlink target must pass validate_path"
        );

        let result = discover_served_databases(&scan_root, project.path(), &[], None);
        assert!(
            matches!(result, Err(GraphtorError::PathViolation { .. })),
            "a symlinked .graphtor root must be rejected even when its target is in-project: {result:?}"
        );
    }

    #[test]
    fn real_graphtor_scan_root_still_discovers_after_containment_guard() {
        // Regression guard: the containment check must NOT reject a normal,
        // real `.graphtor` directory that lives inside the project.
        let project = temp_root();
        let scan_root = project.path().join(".graphtor");
        fs::create_dir_all(&scan_root).expect("create real .graphtor");
        let dropped = touch(&scan_root, "real.db");

        let served = discover_served_databases(&scan_root, project.path(), &[], None)
            .expect("a real in-project .graphtor root must be accepted");
        assert_eq!(served.len(), 1);
        assert_eq!(served[0], validate_path(&dropped, project.path()).unwrap());
    }

    // ── source_has_ingestible_content streaming refactor (055.001.002-ST) ──
    //
    // Characterization-first (Constitution Principle II): these tests pin
    // the CURRENT batch-Vec-then-filter_files behavior of
    // `source_has_ingestible_content` before it is refactored into a
    // streaming boolean. They must pass UNCHANGED both before and after the
    // refactor — that equality is the proof the refactor is
    // behavior-preserving.

    fn make_local_source(
        path: &Path,
        include: &[&str],
        exclude: &[&str],
        formats: &[&str],
    ) -> LocalSource {
        LocalSource {
            id: "characterization".to_string(),
            path: path.to_path_buf(),
            include: include.iter().map(|s| (*s).to_string()).collect(),
            exclude: exclude.iter().map(|s| (*s).to_string()).collect(),
            formats: formats.iter().map(|s| (*s).to_string()).collect(),
            database: None,
        }
    }

    #[test]
    fn characterization_non_directory_path_is_not_ingestible() {
        let root = temp_root();
        let not_a_dir = root.path().join("missing");
        let local = make_local_source(&not_a_dir, &[], &[], &["md"]);
        assert!(!source_has_ingestible_content(&local));
    }

    #[test]
    fn characterization_ingestible_tree_returns_true() {
        let root = temp_root();
        touch(root.path(), "guide.md");
        let local = make_local_source(root.path(), &[], &[], &["md"]);
        assert!(source_has_ingestible_content(&local));
    }

    #[test]
    fn characterization_zero_format_candidate_tree_returns_false() {
        let root = temp_root();
        touch(root.path(), "notes.txt");
        let local = make_local_source(root.path(), &[], &[], &["md"]);
        assert!(!source_has_ingestible_content(&local));
    }

    #[test]
    fn characterization_excluded_only_tree_returns_false() {
        let root = temp_root();
        touch(root.path(), "guide.md");
        let local = make_local_source(root.path(), &["**/*.md"], &["**/*.md"], &["md"]);
        assert!(!source_has_ingestible_content(&local));
    }

    #[test]
    fn characterization_include_filtered_to_zero_stays_false() {
        let root = temp_root();
        touch(root.path(), "guide.md");
        let local = make_local_source(root.path(), &["no-match-*.md"], &[], &["md"]);
        assert!(!source_has_ingestible_content(&local));
    }

    #[test]
    fn characterization_differential_matches_batch_filter_files_across_representative_trees() {
        // Reproduce the PRE-refactor algorithm inline (walk + collect
        // relative candidates + one batch filter_files call) and assert the
        // classifier's boolean equals `!filtered.is_empty()` for each case —
        // the exact invariant the streaming refactor must preserve.
        fn batch_reference(local: &LocalSource) -> bool {
            let mut candidates = Vec::new();
            for entry in walkdir::WalkDir::new(&local.path) {
                let entry = entry.expect("fixture walk must not error");
                if !entry.file_type().is_file() {
                    continue;
                }
                let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) else {
                    continue;
                };
                let matches_format = local
                    .formats
                    .iter()
                    .any(|fmt| canonicalize_format_alias(fmt).eq_ignore_ascii_case(ext));
                if !matches_format {
                    continue;
                }
                if let Ok(rel) = entry.path().strip_prefix(&local.path) {
                    candidates.push(rel.to_path_buf());
                }
            }
            graphtor_core::acquire::filter_files(&candidates, &local.include, &local.exclude)
                .is_ok_and(|filtered| !filtered.is_empty())
        }

        let root = temp_root();
        touch(root.path(), "top.md");
        touch(root.path(), "nested/deep/guide.md");
        touch(root.path(), "drafts/wip.md");
        touch(root.path(), "notes.txt");

        let cases: &[(&[&str], &[&str])] = &[
            (&[], &[]),
            (&["**/*.md"], &[]),
            (&["**/*.md", "**/*.txt"], &[]),
            (&["**/*.md"], &["**/drafts/**"]),
            (&["nested/**/*.md"], &[]),
            (&["no-match-*.md"], &[]),
        ];

        for (include, exclude) in cases {
            let local = make_local_source(root.path(), include, exclude, &["md"]);
            assert_eq!(
                source_has_ingestible_content(&local),
                batch_reference(&local),
                "classifier must match the pre-refactor batch filter_files result for \
                 include={include:?} exclude={exclude:?}"
            );
        }
    }

    // ── stream_ingestible: RED-FIRST new streaming abstraction ──────────
    //
    // `stream_ingestible` has no prior behavior to characterize — it is a
    // brand-new O(1)-memory streaming helper introduced BY this refactor.
    // Per Constitution Principle II, its tests are written and observed to
    // fail (via the `unimplemented!` stub) before the function exists.

    /// Minimal in-memory sink for a scoped `tracing` capture, matching the
    /// established pattern in `src/main.rs`'s `sync_progress_tests::
    /// capture_warn_logs` and `src/db/store.rs`'s `capture_info_logs_once`
    /// helpers (each module keeps its own small private copy rather than
    /// sharing one across `#[cfg(test)]` module boundaries).
    struct CapturedLogWriter {
        output: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    }

    impl std::io::Write for CapturedLogWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.output
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Run `operation` once under a scoped `tracing` subscriber capturing
    /// WARN-and-above events from this crate to an in-memory buffer.
    fn capture_warn_logs_once<F, T>(operation: F) -> (T, String)
    where
        F: FnOnce() -> T,
    {
        let output = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        // NOTE: target crate is `graphtor_docs` (this module is compiled
        // into the BINARY crate, not the `graphtor_core` library crate —
        // see the module doc comment at the top of this file) so the
        // EnvFilter directive must match that target, not the library's.
        let filter = tracing_subscriber::EnvFilter::new("graphtor_docs=warn");
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .without_time()
            .with_env_filter(filter)
            .with_writer({
                let output = std::sync::Arc::clone(&output);
                move || CapturedLogWriter {
                    output: std::sync::Arc::clone(&output),
                }
            })
            .finish();

        let result = tracing::subscriber::with_default(subscriber, || {
            tracing::callsite::rebuild_interest_cache();
            operation()
        });

        let bytes = output
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let logs = String::from_utf8(bytes).expect("tracing output should be valid utf-8");
        (result, logs)
    }

    /// Retry [`capture_warn_logs_once`] until it observes ANY captured
    /// output, or a bounded attempt count is exhausted.
    ///
    /// See `docs/compound/tracing-callsite-interest-cache-parallel-test-race.md`:
    /// a `tracing` call-site's subscriber `Interest` is decided and cached
    /// process-wide the first time it fires. Several sibling tests below
    /// exercise `stream_ingestible`'s `warn!` call-site with NO subscriber
    /// active (they only assert on the returned boolean); under default
    /// parallel `cargo test`, one of those could win the race and cache
    /// `Interest::never()`, silently dropping this test's own event even
    /// though its scoped subscriber is genuinely active. Retrying only on
    /// "captured nothing at all" (never on "captured the wrong thing")
    /// preserves genuine regression detection while absorbing the race.
    fn capture_warn_logs_retrying<F, T>(mut make_operation: impl FnMut() -> F) -> (T, String)
    where
        F: FnOnce() -> T,
    {
        const MAX_ATTEMPTS: u32 = 25;
        let mut last = None;
        for _ in 0..MAX_ATTEMPTS {
            let (result, logs) = capture_warn_logs_once(make_operation());
            if !logs.is_empty() {
                return (result, logs);
            }
            last = Some((result, logs));
        }
        last.expect("MAX_ATTEMPTS is greater than zero")
    }

    fn matcher(include: &[&str], exclude: &[&str]) -> graphtor_core::acquire::FileFilter {
        let include: Vec<String> = include.iter().map(|s| (*s).to_string()).collect();
        let exclude: Vec<String> = exclude.iter().map(|s| (*s).to_string()).collect();
        graphtor_core::acquire::FileFilter::new(&include, &exclude).expect("valid glob patterns")
    }

    #[test]
    fn stream_ingestible_true_when_a_candidate_matches() {
        let m = matcher(&["**/*.md"], &[]);
        let steps: Vec<Result<Option<PathBuf>, ()>> = vec![Ok(Some(PathBuf::from("a.md")))];
        assert!(stream_ingestible(steps, &m));
    }

    #[test]
    fn stream_ingestible_false_on_immediate_error() {
        let m = matcher(&[], &[]);
        let steps: Vec<Result<Option<PathBuf>, ()>> = vec![Err(())];
        assert!(!stream_ingestible(steps, &m));
    }

    #[test]
    fn stream_ingestible_false_when_error_follows_an_eligible_candidate() {
        // THE regression guard: a candidate that would make the classifier
        // eligible is observed BEFORE a later walk error. The full,
        // error-observing walk contract means the later error must still
        // force `false` — a traversal short-circuit that returned `true` at
        // the first eligible candidate would incorrectly escalate a
        // partially-unreadable source to the Generation posture.
        let m = matcher(&["**/*.md"], &[]);
        let steps: Vec<Result<Option<PathBuf>, ()>> =
            vec![Ok(Some(PathBuf::from("eligible.md"))), Err(())];
        assert!(
            !stream_ingestible(steps, &m),
            "a later walk error must override an earlier eligible candidate"
        );
    }

    #[test]
    fn stream_ingestible_false_with_no_candidates_and_no_warning() {
        let (result, logs) = capture_warn_logs_once(|| {
            let m = matcher(&[], &[]);
            let steps: Vec<Result<Option<PathBuf>, ()>> = vec![Ok(None), Ok(None)];
            stream_ingestible(steps, &m)
        });
        assert!(!result, "a walk with no candidates at all must be false");
        assert!(
            logs.is_empty(),
            "a zero-candidate walk must not emit the aggregate warning: {logs:?}"
        );
    }

    #[test]
    fn stream_ingestible_true_with_no_warning_when_ingestible() {
        let (result, logs) = capture_warn_logs_once(|| {
            let m = matcher(&["**/*.md"], &[]);
            let steps: Vec<Result<Option<PathBuf>, ()>> = vec![Ok(Some(PathBuf::from("a.md")))];
            stream_ingestible(steps, &m)
        });
        assert!(result);
        assert!(
            logs.is_empty(),
            "an ingestible tree must not emit the aggregate warning: {logs:?}"
        );
    }

    #[test]
    fn stream_ingestible_false_with_no_warning_on_walk_error() {
        let (result, logs) = capture_warn_logs_once(|| {
            let m = matcher(&["**/*.md"], &[]);
            let steps: Vec<Result<Option<PathBuf>, ()>> =
                vec![Ok(Some(PathBuf::from("eligible.md"))), Err(())];
            stream_ingestible(steps, &m)
        });
        assert!(!result);
        assert!(
            logs.is_empty(),
            "a walk error must never emit the misleading aggregate 'all excluded' warning: \
             {logs:?}"
        );
    }

    #[test]
    fn stream_ingestible_warns_exactly_once_with_scalar_candidate_count_when_all_excluded() {
        let (result, logs) = capture_warn_logs_retrying(|| {
            let m = matcher(&["**/*.md"], &["**/*.md"]);
            let steps: Vec<Result<Option<PathBuf>, ()>> = vec![
                Ok(Some(PathBuf::from("a.md"))),
                Ok(Some(PathBuf::from("b.md"))),
            ];
            move || stream_ingestible(steps, &m)
        });
        assert!(!result, "all-excluded candidates must classify as false");
        assert!(
            !logs.is_empty(),
            "capture never observed ANY event for the aggregate warning across the retry \
             budget — this indicates a capture-seam regression, not a wording mismatch"
        );
        assert_eq!(
            logs.matches("all files were excluded").count(),
            1,
            "exactly one aggregate warning must be emitted, not one per candidate: {logs:?}"
        );
        assert!(
            logs.contains("input_files=2"),
            "warning must carry the scalar format-candidate count (2), not a per-file list: \
             {logs}"
        );
        assert!(
            logs.contains("filter produced empty file set — all files were excluded"),
            "warning wording must match filter_files's own S032 message for parity: {logs}"
        );
    }
}
