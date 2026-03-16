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
    let include_set = build_glob_set(include, "include")?;
    let exclude_set = build_glob_set(exclude, "exclude")?;

    let filtered: Vec<PathBuf> = files
        .iter()
        .filter(|path| {
            // Normalize to forward slashes for cross-platform glob matching.
            let s = path_to_forward_slash(path);
            // FR-009: no include patterns → include all files.
            let included = match &include_set {
                None => true,
                Some(set) => set.is_match(&s),
            };
            if !included {
                return false;
            }
            // FR-010: no exclude patterns → exclude nothing.
            match &exclude_set {
                None => true,
                Some(set) => !set.is_match(&s),
            }
        })
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
}
