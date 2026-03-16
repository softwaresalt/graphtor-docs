<!-- markdownlint-disable-file -->
# PR Review Handoff: 003-source-management

## PR Overview

Implements the Source Registry & Acquisition feature (spec 003 / FG-002). Adds `src/acquire/` with six modules covering result types, glob filtering, Git shallow-clone, local directory scanning, acquisition planning, and top-level orchestration. All 50 spec tasks are marked done across 9 build phases.

* Branch: `003-source-management`
* Base Branch: `main`
* Total Files Changed: 39 (4395 insertions, 6 deletions)
* Total Review Comments: 6 substantive findings + 3 test-gap notes

---

## PR Comments Ready for Submission

---

### File: `src/acquire/filter.rs` + `src/acquire/mod.rs`

#### Comment 1 (filter.rs lines 38–58; mod.rs lines 202–215) — HIGH ⚠️

* Category: Correctness / API Semantics
* Severity: HIGH — silent wrong-answer bug for path-specific glob patterns

**Finding**

`filter_files` receives the raw file list from `scan_local_source`, which returns **absolute paths** (e.g. `/data/sources/my-repo/docs/guide.md`). The function matches each absolute path against user-supplied glob patterns by converting only backslashes, then calling `GlobSet::is_match`:

```rust
// filter.rs, lines 42–54
let s = path_to_forward_slash(path); // "/data/sources/my-repo/docs/guide.md"
let included = match &include_set {
    None => true,
    Some(set) => set.is_match(&s),   // pattern: "docs/**/*.md"
};
```

Patterns that start with `**` (e.g. `**/*.md`, `**/drafts/**`) happen to work correctly because globset's `**` matches any path prefix including an absolute one. However, patterns that start with a literal directory component — the documented idiomatic form for scoping to a subtree — fail silently:

| Pattern | Absolute path | Expected | Actual |
|---------|--------------|----------|--------|
| `**/*.md` | `/data/sources/repo/guide.md` | ✅ match | ✅ match |
| `docs/**/*.md` | `/data/sources/repo/docs/guide.md` | ✅ match | ❌ no match |
| `**/drafts/**` | `/data/sources/repo/drafts/old.md` | ✅ exclude | ✅ exclude |
| `drafts/**` | `/data/sources/repo/drafts/old.md` | ✅ exclude | ❌ no match |

When an include pattern like `docs/**/*.md` silently fails to match any file, `FilteredFileSet.filtered_count` is 0. The `warn!` in `filter_files` fires ("filter produced empty file set"), giving the user a runtime hint but no indication of the root cause. With an exclude pattern, the failure is even harder to notice — the pattern simply excludes nothing.

The unit tests in `filter.rs` use relative path strings and pass. The integration tests in `acquire_filter_test.rs` only exercise `**`-prefixed patterns. This combination means the bug is not caught by the current test suite.

**Suggested Fix**

Strip the source root prefix from each file path before applying glob filters. `scan_and_filter` in `mod.rs` has access to both the source root and the file list:

```rust
// mod.rs — updated scan_and_filter
fn scan_and_filter(
    source: &LocalSource,
    acq_plan: &AcquisitionPlan,
) -> Result<FilteredFileSet, GraphtorError> {
    let files = scan_local_source(source, &acq_plan.allowed_root)?;

    // Strip the source root to get root-relative paths for glob matching.
    // Fall back to the full path if stripping fails (e.g. on path component mismatch).
    let relative_files: Vec<PathBuf> = files
        .iter()
        .map(|f| f.strip_prefix(&source.path).unwrap_or(f).to_path_buf())
        .collect();

    let ffs = apply_source_filter(&source.id, &relative_files, &source.include, &source.exclude)?;

    // ... warn, return
}
```

Alternatively, move the strip into `filter_files` itself by accepting an optional `root` parameter and stripping before matching.

The `FilteredFileSet.files` field should then store root-relative paths — or the calling code must re-join them with the source root for downstream stages. Clarify and document whichever convention is chosen.

**Test to add** (integration, `acquire_filter_test.rs`):

```rust
#[test]
fn e2e_path_specific_include_pattern_matches_subtree_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    // docs/guide.md  api/ref.md
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::create_dir_all(root.join("api")).unwrap();
    fs::write(root.join("docs/guide.md"), "").unwrap();
    fs::write(root.join("api/ref.md"), "").unwrap();

    let source = make_local_source("src", root);
    let all_files = scan_local_source(&source, root).unwrap();

    let result = apply_source_filter("src", &all_files, &["docs/**/*.md".to_string()], &[])
        .expect("filter");

    assert_eq!(result.filtered_count, 1, "only docs/guide.md should match");
    assert!(result.files[0].ends_with("docs/guide.md"));
}
```

---

### File: `src/acquire/plan.rs`

#### Comment 2 (lines 152–161) — MEDIUM-HIGH 🔒

* Category: Path Security / Design
* Severity: MEDIUM-HIGH — canonical path returned by security check is silently discarded

**Finding**

In `resolve_source_action`, the validated canonical path is computed but thrown away:

```rust
// plan.rs, lines 152–161
Source::Git(git) => {
    let target_dir = canonical_data_root.join(&git.id);  // raw join, not canonical
    // Security: validate even though target may not exist yet
    crate::path::validate_path(&target_dir, allowed_root)?;  // ← return value discarded
    let action = if target_dir.join(".git").exists() {
        SourceAction::SkipGit
    } else {
        SourceAction::CloneGit
    };
    Ok((action, target_dir))  // ← returns the raw, potentially non-canonical path
}
```

`validate_path` returns `Result<PathBuf, _>`, where the `Ok` variant is the fully canonical, `..`-resolved path. Discarding it means the `PlannedSource.target_dir` stored in the plan may contain unresolved `..` components if `git.id` was supplied with path-like characters (e.g. `"my/nested-repo"` or `"../escape"`).

Config validation (`config/validation.rs`) does **not** reject source IDs that contain `/` or `..` — only empty IDs and duplicate IDs are caught. So `git.id = "../../etc"` passes config validation, and `canonical_data_root.join("../../etc")` stores an unresolved path in the plan.

**The security gate does hold** — the `?` propagates `PathViolation` if the resolved path escapes `allowed_root`. But if the traversal stays within `allowed_root` (e.g. `data_root/../other-project`), the plan records the non-canonical path and the clone silently targets an unintended location within the allowed boundary.

**Suggested Fix**

Use the return value of `validate_path` as `target_dir`:

```rust
Source::Git(git) => {
    let raw_target = canonical_data_root.join(&git.id);
    let target_dir = crate::path::validate_path(&raw_target, allowed_root)?;  // canonical
    let action = if target_dir.join(".git").exists() {
        SourceAction::SkipGit
    } else {
        SourceAction::CloneGit
    };
    Ok((action, target_dir))
}
```

Additionally, add a check in `config/validation.rs` to reject source IDs containing path separators:

```rust
if id.contains('/') || id.contains('\\') || id.contains("..") {
    return Err(GraphtorError::Config {
        message: format!("source id '{id}' must not contain path separators"),
        field: Some("id".to_string()),
    });
}
```

---

#### Comment 3 (line 70) — LOW-MEDIUM ⚠️

* Category: Design Consistency
* Severity: LOW-MEDIUM — `AcquisitionPlan.allowed_root` is the only non-canonical path in the struct

**Finding**

`AcquisitionPlan` stores `data_root` in canonical form but `allowed_root` in the raw caller-supplied form:

```rust
// plan.rs, lines 68–75
Ok(AcquisitionPlan {
    data_root: canonical_data_root,          // ✅ canonical
    allowed_root: allowed_root.to_path_buf(), // ❌ raw — not canonicalized
    sources,
    ...
})
```

`scan_local_source` later passes `acq_plan.allowed_root` to `validate_path`, which re-canonicalizes it, so the security boundary is preserved. However, consumers of `AcquisitionPlan` who inspect `.allowed_root` directly receive an unresolved path, which is surprising given that `data_root` is canonical.

**Suggested Fix**

Canonicalize `allowed_root` at plan construction time:

```rust
// plan.rs — near top of plan()
let canonical_root = crate::path::validate_path(allowed_root, allowed_root)
    .map_err(|_| crate::path::canonicalize_clean(allowed_root).map_err(GraphtorError::Io))??;

// Or simpler — since allowed_root must exist:
let canonical_allowed_root = crate::path::canonicalize_clean(allowed_root)
    .map_err(GraphtorError::Io)?;

Ok(AcquisitionPlan {
    data_root: canonical_data_root,
    allowed_root: canonical_allowed_root,
    ...
})
```

---

### File: `src/acquire/git.rs`

#### Comment 4 (lines 51–53) — MEDIUM ⚠️

* Category: Reliability
* Severity: MEDIUM — fragile string match ties fallback to a specific libgit2 error message

**Finding**

The shallow-clone fallback condition relies on an internal libgit2 message string:

```rust
// git.rs, lines 51–53
.or_else(|e| {
    if e.class() == git2::ErrorClass::Net && e.message().contains("shallow") {
```

The word `"shallow"` is an implementation detail of libgit2's error reporting for local transports. If a future version of libgit2 changes the message text (e.g. to `"depth not supported"` or `"transport does not support shallow fetch"`), the condition silently fails to match. The fallback is never triggered, and all `file://` clones in test environments begin to fail with an unhelpful error.

The condition is also too narrow in the opposite direction: other transport types that genuinely don't support shallow clones would only be retried if they happen to produce an error message containing `"shallow"`.

**Suggested Fix**

Broaden the condition to match any `Net`-class error when the URL is a local `file://` transport, or log the matched error text in a `DEBUG` span to make version-dependent behavior visible:

```rust
.or_else(|e| {
    let is_local_url = source.url.starts_with("file://");
    let is_shallow_unsupported = e.class() == git2::ErrorClass::Net
        && (e.message().contains("shallow") || e.message().contains("depth"));

    if is_local_url && is_shallow_unsupported {
        tracing::debug!(
            source_id = %source.id,
            git_message = e.message(),
            "retrying without shallow clone due to transport limitation"
        );
        // ... cleanup and retry
    } else {
        Err(e)
    }
})
```

Or add a comment citing the exact libgit2 version and the error string this was verified against, so a future maintainer knows it needs revisiting after a libgit2 upgrade.

---

#### Comment 5 (lines 57–60) — MEDIUM ⚠️

* Category: Reliability / Debuggability
* Severity: MEDIUM — silent discard of cleanup error obscures the real failure on retry

**Finding**

In the shallow-clone fallback path, the directory removal error is silently discarded:

```rust
// git.rs, lines 57–60
if target_dir.exists() {
    let _ = std::fs::remove_dir_all(target_dir);  // ← error silently dropped
}
let mut builder2 = git2::build::RepoBuilder::new();
builder2.branch(&source.branch);
builder2.clone(&source.url, target_dir)  // ← may fail opaquely if removal failed
```

If `remove_dir_all` fails (e.g. permission denied, file lock on Windows), the second clone attempt will fail because the target directory still exists. The error the caller sees is from the clone, not from the cleanup — making the root cause invisible. The outer error handler (lines 82–89) correctly logs and handles the final error, but the intermediate cause is lost.

**Suggested Fix**

Log the cleanup failure at `WARN` level before retrying:

```rust
if target_dir.exists() {
    if let Err(rm_err) = std::fs::remove_dir_all(target_dir) {
        warn!(
            source_id = %source.id,
            remove_error = %rm_err,
            "could not remove partial clone before retry; full clone may fail"
        );
    }
}
```

---

#### Comment 6 (lines 105–111) — LOW 💡

* Category: API Design
* Severity: LOW — `pub` visibility leaks `git2::Error` into the public API surface

**Finding**

`git_error_to_pipeline` is declared `pub`, making it part of the crate's public API:

```rust
// git.rs, lines 105–111
#[must_use]
pub fn git_error_to_pipeline(e: &git2::Error, source_id: &str) -> GraphtorError {
```

Any caller outside the crate who wants to use this function must also depend on `git2` and import `git2::Error`. There is no documented external use case for this function — it exists to translate `git2` errors into `GraphtorError::Pipeline` within the acquire module. Exposing it forces `git2` into the compile-time interface contract of `graphtor-core`.

**Suggested Fix**

Change the visibility to `pub(crate)`:

```rust
#[must_use]
pub(crate) fn git_error_to_pipeline(e: &git2::Error, source_id: &str) -> GraphtorError {
```

---

## Test Gap Notes (Non-blocking)

These are not individual PR comments but warrant tracking for follow-up:

1. **RI-007 — Source ID path separator validation gap** (`src/config/validation.rs`): Source IDs containing `/`, `\\`, or `..` pass config validation and become directory-name components. A simple check in `validate()` that rejects IDs with path separators would close this gap cleanly.

2. **RI-008 — Integration test gap for non-`**`-prefixed glob patterns** (`tests/acquire_filter_test.rs`): All integration filter tests use `**`-prefixed patterns. Once Comment 1 is addressed, add a test for `docs/**/*.md` and `drafts/**` applied to files returned from `scan_local_source`.

3. **RI-009 — `validate_sources` skips traversal check for non-existent local paths** (`src/acquire/plan.rs`, lines 116–122): `validate_path` is only called when `local.path.exists()`. A path like `../../etc/passwd` that doesn't exist only reports "path does not exist" without also flagging the traversal. Minor — the acquisition step itself enforces the boundary — but the validation report would be more complete if it also checked reachability for non-existent paths.

---

## Review Summary by Category

| Category | Count |
|----------|-------|
| Correctness | 1 (RI-001, HIGH) |
| Security / Path Handling | 1 (RI-002, MEDIUM-HIGH) |
| Reliability | 2 (RI-003, RI-004, MEDIUM) |
| Design Consistency | 1 (RI-005, LOW-MEDIUM) |
| API Design | 1 (RI-006, LOW) |
| Test Gaps (non-blocking) | 3 (RI-007, RI-008, RI-009) |

## Instruction Compliance

* ✅ `#![forbid(unsafe_code)]` — present in `lib.rs`; no unsafe in any new file
* ✅ No bare `unwrap()` in production code paths
* ✅ All public functions have doc comments with `# Errors` sections
* ✅ `GraphtorError` used consistently; no raw `std::io::Error` escapes
* ✅ `build.rs` Windows MSVC linker fix is correct and scoped to target OS
* ⚠️ `filter_files` applies globs to absolute paths — patterns without `**` prefix fail silently (RI-001)
* ⚠️ `resolve_source_action` discards canonical path from `validate_path` (RI-002)
* ⚠️ `git_error_to_pipeline` visibility is `pub` instead of `pub(crate)` (RI-006)
