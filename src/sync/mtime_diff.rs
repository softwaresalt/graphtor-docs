//! Local file modification-time change detection.
//!
//! Compares current filesystem mtimes against stored values to identify which
//! Markdown files have been added, modified, or deleted since the last sync
//! of a local directory source.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use tracing::debug;
use walkdir::WalkDir;

use super::git_diff::ChangedFiles;
use crate::error::GraphtorError;

/// Compute which Markdown files in `source_root` changed relative to
/// `stored_mtimes`.
///
/// Classification:
/// - Present in `stored_mtimes` but **missing** from the filesystem → `deleted`.
/// - Present in the filesystem but **absent** from `stored_mtimes` → `added`.
/// - Present in both but with a **newer** mtime → `modified`.
///
/// All returned paths are relative to `source_root`, using forward-slash
/// separators for cross-platform consistency.
///
/// # Errors
///
/// Returns [`GraphtorError::Pipeline`] if the source root cannot be walked
/// or a file's mtime cannot be retrieved.
pub fn compute_mtime_diff<S: std::hash::BuildHasher>(
    source_root: &Path,
    stored_mtimes: &std::collections::HashMap<String, u64, S>,
) -> Result<ChangedFiles, GraphtorError> {
    let current = scan_mtimes(source_root)?;
    let mut result = ChangedFiles::default();

    // Added and modified.
    for (rel_path, &current_mtime) in &current {
        match stored_mtimes.get(rel_path) {
            None => result.added.push(PathBuf::from(rel_path)),
            Some(&stored_mtime) if current_mtime > stored_mtime => {
                result.modified.push(PathBuf::from(rel_path));
            }
            _ => {}
        }
    }

    // Deleted.
    for stored_path in stored_mtimes.keys() {
        if !current.contains_key(stored_path) {
            result.deleted.push(PathBuf::from(stored_path));
        }
    }

    debug!(
        added = result.added.len(),
        modified = result.modified.len(),
        deleted = result.deleted.len(),
        "mtime diff computed"
    );
    Ok(result)
}

/// Recursively scan `root` and return a map of relative path → mtime (seconds
/// since the Unix epoch) for all Markdown files.
///
/// Keys use forward-slash separators on all platforms.
///
/// # Errors
///
/// Returns [`GraphtorError::Pipeline`] if an entry cannot be stat'd or its
/// mtime cannot be converted.
pub fn scan_mtimes(root: &Path) -> Result<HashMap<String, u64>, GraphtorError> {
    let mut result = HashMap::new();
    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !is_markdown(path) {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|e| GraphtorError::Pipeline {
                message: format!("failed to relativize path {}: {e}", path.display()),
                stage: "mtime_diff".to_string(),
            })?;
        let meta = entry.metadata().map_err(|e| GraphtorError::Pipeline {
            message: format!("failed to stat {}: {e}", path.display()),
            stage: "mtime_diff".to_string(),
        })?;
        let mtime = meta
            .modified()
            .map_err(|e| GraphtorError::Pipeline {
                message: format!("mtime unavailable for {}: {e}", path.display()),
                stage: "mtime_diff".to_string(),
            })?
            .duration_since(UNIX_EPOCH)
            .map_err(|e| GraphtorError::Pipeline {
                message: format!("mtime before Unix epoch for {}: {e}", path.display()),
                stage: "mtime_diff".to_string(),
            })?
            .as_secs();
        // Normalize to forward slashes for cross-platform keys.
        result.insert(rel.to_string_lossy().replace('\\', "/"), mtime);
    }
    Ok(result)
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread;
    use std::time::Duration;
    use tempfile::TempDir;

    fn temp_dir() -> TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn new_files_detected_as_added() {
        let dir = temp_dir();
        fs::write(dir.path().join("a.md"), "# A").expect("write");

        let stored: HashMap<String, u64> = HashMap::new();
        let diff = compute_mtime_diff(dir.path(), &stored).expect("diff");

        assert_eq!(diff.added.len(), 1, "expected 1 added");
        assert!(diff.modified.is_empty());
        assert!(diff.deleted.is_empty());
    }

    #[test]
    fn removed_files_detected_as_deleted() {
        let dir = temp_dir();
        let mut stored = HashMap::new();
        stored.insert("old.md".to_string(), 1_000_000_u64);

        let diff = compute_mtime_diff(dir.path(), &stored).expect("diff");

        assert_eq!(diff.deleted.len(), 1, "expected 1 deleted");
        assert!(diff.added.is_empty());
        assert!(diff.modified.is_empty());
    }

    #[test]
    fn modified_files_detected_as_modified() {
        let dir = temp_dir();
        let file = dir.path().join("b.md");
        fs::write(&file, "# B").expect("write");

        // Get actual mtime then store a much older value.
        let meta = fs::metadata(&file).expect("meta");
        let mtime = meta
            .modified()
            .expect("mtime")
            .duration_since(UNIX_EPOCH)
            .expect("epoch")
            .as_secs();

        let mut stored = HashMap::new();
        stored.insert("b.md".to_string(), mtime.saturating_sub(100));

        let diff = compute_mtime_diff(dir.path(), &stored).expect("diff");

        assert_eq!(diff.modified.len(), 1, "expected 1 modified");
        assert!(diff.added.is_empty());
        assert!(diff.deleted.is_empty());
    }

    #[test]
    fn unchanged_files_produce_empty_diff() {
        let dir = temp_dir();
        let file = dir.path().join("c.md");
        fs::write(&file, "# C").expect("write");

        // Allow mtime to settle.
        thread::sleep(Duration::from_millis(50));

        let current = scan_mtimes(dir.path()).expect("scan");
        let diff = compute_mtime_diff(dir.path(), &current).expect("diff");

        assert!(diff.is_empty(), "expected empty diff for unchanged files");
    }

    #[test]
    fn non_markdown_files_are_ignored() {
        let dir = temp_dir();
        fs::write(dir.path().join("readme.txt"), "text").expect("write");
        fs::write(dir.path().join("script.rs"), "fn main() {}").expect("write");

        let stored: HashMap<String, u64> = HashMap::new();
        let diff = compute_mtime_diff(dir.path(), &stored).expect("diff");

        assert!(diff.is_empty(), "non-markdown files should be ignored");
    }
}
