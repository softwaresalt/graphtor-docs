//! Glob pattern filtering — include/exclude file set reduction.
//!
//! Provides [`filter_files`] which applies include and exclude glob patterns
//! to a list of discovered files, producing a filtered set ready for ingestion
//! (FR-006–FR-010).

use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};
use tracing::warn;

use crate::error::GraphtorError;

/// Apply include/exclude glob patterns to a list of file paths.
///
/// Returns only files that match at least one include pattern **and** do not
/// match any exclude pattern. Include is applied before exclude (FR-008).
///
/// Defaults:
/// - No include patterns → all files pass the include check (FR-009).
/// - No exclude patterns → no files are removed (FR-010).
///
/// A warning is logged when the result is empty but the input was non-empty
/// (scenario S032).
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] if any glob pattern string is syntactically
/// invalid (FR-013).
pub fn filter_files(
    files: &[PathBuf],
    include: &[String],
    exclude: &[String],
) -> Result<Vec<PathBuf>, GraphtorError> {
    let matcher = FileFilter::new(include, exclude)?;

    let filtered: Vec<PathBuf> = files
        .iter()
        .filter(|path| matcher.is_match(path))
        .cloned()
        .collect();

    // S032: warn when filtering produced an empty set from non-empty input.
    if filtered.is_empty() && !files.is_empty() {
        warn!(
            input_files = files.len(),
            "filter produced empty file set — all files were excluded"
        );
    }

    Ok(filtered)
}

/// A compiled, reusable include/exclude glob matcher.
///
/// [`filter_files`] builds the equivalent of one internally for a single
/// batch call. Callers that need to test many individual paths one at a
/// time — for example, a streaming boolean classifier walking a directory
/// tree — should build a single [`FileFilter`] up front with [`FileFilter::new`]
/// and reuse it via [`FileFilter::is_match`], rather than calling
/// [`filter_files`] once per path (which would recompile the glob sets on
/// every call and defeat the point of streaming).
///
/// `filter_files` itself is implemented in terms of `FileFilter` so there is
/// a single source of truth for include/exclude semantics.
#[derive(Debug)]
pub struct FileFilter {
    include: Option<GlobSet>,
    exclude: Option<GlobSet>,
}

impl FileFilter {
    /// Compile `include`/`exclude` glob pattern lists into a reusable matcher.
    ///
    /// Defaults mirror [`filter_files`]:
    /// - No include patterns → every path passes the include check (FR-009).
    /// - No exclude patterns → no path is excluded (FR-010).
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Config`] if any pattern string is
    /// syntactically invalid (FR-013).
    pub fn new(include: &[String], exclude: &[String]) -> Result<Self, GraphtorError> {
        Ok(Self {
            include: build_glob_set(include, "include")?,
            exclude: build_glob_set(exclude, "exclude")?,
        })
    }

    /// Returns `true` when `path` matches at least one include pattern (or
    /// there are no include patterns) **and** does not match any exclude
    /// pattern (or there are no exclude patterns).
    ///
    /// Mirrors the per-file semantics [`filter_files`] applies to each
    /// element of its input slice: include is checked before exclude, and
    /// paths are matched on their forward-slash-normalized form for
    /// cross-platform consistency.
    #[must_use]
    pub fn is_match(&self, path: &Path) -> bool {
        // Normalize to forward slashes for cross-platform glob matching.
        let s = path_to_forward_slash(path);
        // FR-009: no include patterns → include all files.
        let included = match &self.include {
            None => true,
            Some(set) => set.is_match(&s),
        };
        if !included {
            return false;
        }
        // FR-010: no exclude patterns → exclude nothing.
        match &self.exclude {
            None => true,
            Some(set) => !set.is_match(&s),
        }
    }
}

/// Build a [`GlobSet`] from a slice of pattern strings.
///
/// Returns `None` when the pattern list is empty (caller treats `None` as
/// "match everything" for include, or "match nothing" for exclude).
///
/// # Errors
///
/// Returns [`GraphtorError::Config`] for any invalid pattern.
fn build_glob_set(patterns: &[String], kind: &str) -> Result<Option<GlobSet>, GraphtorError> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|e| GraphtorError::Config {
            message: format!("invalid {kind} pattern '{pattern}': {e}"),
            field: None,
        })?;
        builder.add(glob);
    }
    let set = builder.build().map_err(|e| GraphtorError::Config {
        message: format!("failed to compile {kind} glob set: {e}"),
        field: None,
    })?;
    Ok(Some(set))
}

/// Convert a path to a forward-slash string for cross-platform glob matching.
fn path_to_forward_slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(strs: &[&str]) -> Vec<PathBuf> {
        strs.iter().map(|s| PathBuf::from(*s)).collect()
    }

    fn include(patterns: &[&str]) -> Vec<String> {
        patterns.iter().map(|s| (*s).to_owned()).collect()
    }

    fn exclude(patterns: &[&str]) -> Vec<String> {
        patterns.iter().map(|s| (*s).to_owned()).collect()
    }

    // ── S026: Include only markdown files ────────────────────────────────

    #[test]
    fn s026_include_md_only_returns_md_files() {
        let files = paths(&["a.md", "b.txt", "c.rs"]);
        let result = filter_files(&files, &include(&["**/*.md"]), &[]).unwrap();
        assert_eq!(
            result,
            paths(&["a.md"]),
            "only .md files should be included"
        );
    }

    // ── S027: Include multiple patterns (union) ───────────────────────────

    #[test]
    fn s027_multiple_include_patterns_union() {
        let files = paths(&["a.md", "b.txt", "c.rs"]);
        let result = filter_files(&files, &include(&["**/*.md", "**/*.txt"]), &[]).unwrap();
        assert!(
            result.contains(&PathBuf::from("a.md")),
            "a.md should be included: {result:?}"
        );
        assert!(
            result.contains(&PathBuf::from("b.txt")),
            "b.txt should be included: {result:?}"
        );
        assert!(
            !result.contains(&PathBuf::from("c.rs")),
            "c.rs should be excluded: {result:?}"
        );
        assert_eq!(result.len(), 2);
    }

    // ── S028: Exclude removes from included set ───────────────────────────

    #[test]
    fn s028_exclude_removes_from_included_set() {
        let files = paths(&["a.md", "drafts/b.md", "c.md"]);
        let result =
            filter_files(&files, &include(&["**/*.md"]), &exclude(&["**/drafts/**"])).unwrap();
        assert!(
            result.contains(&PathBuf::from("a.md")),
            "a.md should remain: {result:?}"
        );
        assert!(
            result.contains(&PathBuf::from("c.md")),
            "c.md should remain: {result:?}"
        );
        assert!(
            !result.contains(&PathBuf::from("drafts/b.md")),
            "drafts/b.md should be excluded: {result:?}"
        );
        assert_eq!(result.len(), 2);
    }

    // ── S029: Include before exclude (exclude wins when both match) ───────

    #[test]
    fn s029_exclude_wins_when_both_patterns_match() {
        // file matches both include and exclude — exclude should win (FR-008)
        let files = paths(&["docs/api.md"]);
        let result = filter_files(&files, &include(&["**/*.md"]), &exclude(&["docs/**"])).unwrap();
        assert!(
            result.is_empty(),
            "excluded file should not appear: {result:?}"
        );
    }

    // ── S030: No patterns means all files ────────────────────────────────

    #[test]
    fn s030_no_patterns_returns_all_files() {
        let files = paths(&["a.md", "b.txt"]);
        let result = filter_files(&files, &[], &[]).unwrap();
        assert_eq!(result.len(), 2, "all files should be returned: {result:?}");
        assert!(result.contains(&PathBuf::from("a.md")));
        assert!(result.contains(&PathBuf::from("b.txt")));
    }

    // ── S031: No exclude patterns means nothing excluded ─────────────────

    #[test]
    fn s031_no_exclude_means_all_included_pass() {
        let files = paths(&["a.md", "b.md"]);
        let result = filter_files(&files, &include(&["**/*.md"]), &[]).unwrap();
        assert_eq!(
            result.len(),
            2,
            "all .md files should be returned: {result:?}"
        );
    }

    // ── S032: All files excluded produces empty set + (implicit) warn ─────

    #[test]
    fn s032_all_excluded_returns_empty_vec() {
        let files = paths(&["a.md", "b.md"]);
        let result = filter_files(&files, &include(&["**/*.md"]), &exclude(&["**/*.md"])).unwrap();
        assert!(
            result.is_empty(),
            "all files excluded should yield empty result: {result:?}"
        );
    }

    // ── S033: Path-specific include pattern ──────────────────────────────

    #[test]
    fn s033_path_specific_include_pattern() {
        let files = paths(&["docs/guide.md", "api/ref.md"]);
        let result = filter_files(&files, &include(&["docs/**/*.md"]), &[]).unwrap();
        assert_eq!(
            result,
            paths(&["docs/guide.md"]),
            "only docs/ files should match: {result:?}"
        );
    }

    // ── S034: Both README.md and readme.md match **/*.md ─────────────────

    #[test]
    fn s034_md_extension_matches_both_case_variants() {
        let files = paths(&["README.md", "readme.md"]);
        let result = filter_files(&files, &include(&["**/*.md"]), &[]).unwrap();
        assert_eq!(
            result.len(),
            2,
            "both files should match **/*.md: {result:?}"
        );
    }

    // ── Error cases ───────────────────────────────────────────────────────

    #[test]
    fn invalid_include_pattern_returns_config_error() {
        let files = paths(&["a.md"]);
        let result = filter_files(&files, &include(&["[invalid"]), &[]);
        assert!(
            matches!(result, Err(GraphtorError::Config { .. })),
            "invalid include pattern must return Config error: {result:?}"
        );
    }

    #[test]
    fn invalid_exclude_pattern_returns_config_error() {
        let files = paths(&["a.md"]);
        let result = filter_files(&files, &[], &exclude(&["[bad"]));
        assert!(
            matches!(result, Err(GraphtorError::Config { .. })),
            "invalid exclude pattern must return Config error: {result:?}"
        );
    }

    #[test]
    fn empty_file_list_returns_empty_result() {
        let result = filter_files(&[], &include(&["**/*.md"]), &[]).unwrap();
        assert!(result.is_empty());
    }

    // ── FileFilter: reusable compiled matcher (055.001.001-ST) ────────────
    //
    // RED-FIRST: FileFilter is a brand-new public API with no prior
    // behavior to characterize (Constitution Principle II) — these tests
    // are written before the implementation and must fail (via the
    // unimplemented! stub) before FileFilter is implemented.

    #[test]
    fn file_filter_include_only_matches_pattern() {
        let filter = FileFilter::new(&include(&["**/*.md"]), &[]).unwrap();
        assert!(filter.is_match(Path::new("a.md")));
        assert!(!filter.is_match(Path::new("b.txt")));
    }

    #[test]
    fn file_filter_multiple_include_patterns_union() {
        let filter = FileFilter::new(&include(&["**/*.md", "**/*.txt"]), &[]).unwrap();
        assert!(filter.is_match(Path::new("a.md")));
        assert!(filter.is_match(Path::new("b.txt")));
        assert!(!filter.is_match(Path::new("c.rs")));
    }

    #[test]
    fn file_filter_exclude_wins_when_both_match() {
        let filter = FileFilter::new(&include(&["**/*.md"]), &exclude(&["docs/**"])).unwrap();
        assert!(!filter.is_match(Path::new("docs/api.md")));
    }

    #[test]
    fn file_filter_no_include_patterns_matches_all() {
        let filter = FileFilter::new(&[], &[]).unwrap();
        assert!(filter.is_match(Path::new("a.md")));
        assert!(filter.is_match(Path::new("b.txt")));
    }

    #[test]
    fn file_filter_no_exclude_patterns_excludes_nothing() {
        let filter = FileFilter::new(&include(&["**/*.md"]), &[]).unwrap();
        assert!(filter.is_match(Path::new("a.md")));
        assert!(filter.is_match(Path::new("b.md")));
    }

    #[test]
    fn file_filter_invalid_include_pattern_returns_config_error() {
        let result = FileFilter::new(&include(&["[invalid"]), &[]);
        assert!(
            matches!(result, Err(GraphtorError::Config { .. })),
            "invalid include pattern must return Config error: {result:?}"
        );
    }

    #[test]
    fn file_filter_invalid_exclude_pattern_returns_config_error() {
        let result = FileFilter::new(&[], &exclude(&["[bad"]));
        assert!(
            matches!(result, Err(GraphtorError::Config { .. })),
            "invalid exclude pattern must return Config error: {result:?}"
        );
    }

    #[test]
    fn file_filter_is_reusable_across_many_is_match_calls() {
        // Build once, query many times — proves the matcher is a stable,
        // reusable value rather than something that must be reconstructed
        // per call, which is the entire point of exposing it separately
        // from `filter_files` (a streaming caller compiles once and tests
        // many candidate paths one at a time).
        let filter = FileFilter::new(&include(&["**/*.md"]), &exclude(&["**/drafts/**"])).unwrap();
        let candidates = ["a.md", "drafts/b.md", "c.md", "d.txt"];
        let results: Vec<bool> = candidates
            .iter()
            .map(|p| filter.is_match(Path::new(p)))
            .collect();
        assert_eq!(results, vec![true, false, true, false]);
    }

    #[test]
    fn file_filter_matches_filter_files_semantics_on_representative_paths() {
        // Differential check: applying FileFilter::is_match per-file must
        // agree with filter_files applied to the whole batch, for
        // representative include/exclude combinations (nested paths,
        // exclude-wins, non-matching files).
        let files = paths(&["a.md", "drafts/b.md", "c.md", "docs/api.md", "e.txt"]);
        let include_patterns = include(&["**/*.md"]);
        let exclude_patterns = exclude(&["**/drafts/**"]);
        let batch = filter_files(&files, &include_patterns, &exclude_patterns).unwrap();

        let filter = FileFilter::new(&include_patterns, &exclude_patterns).unwrap();
        let streamed: Vec<PathBuf> = files
            .iter()
            .filter(|p| filter.is_match(p))
            .cloned()
            .collect();

        assert_eq!(
            batch, streamed,
            "FileFilter::is_match applied per-file must equal filter_files applied as a batch"
        );
    }
}
