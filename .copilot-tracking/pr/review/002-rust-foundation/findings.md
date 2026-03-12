# PR Review Findings — `002-rust-foundation`

Branch: `002-rust-foundation` → `main`  
Commits ahead: 21 (8 feature commits, 13 chore/spec)  
Rust files reviewed: 19 source + integration test files  
Tests at review time: 70 passing

---

## BLOCKER: Dead Dependency (`serde_json`)

**File:** `Cargo.toml:24`  
**Pattern:** `serde_json = "1"` is declared in `[dependencies]` but is not imported or used in any source file.  
**Risk:** Dead code in production binary; unnecessary build-time and binary-size cost. May trigger future `cargo deny` / audit warnings.  
**Fix:** Remove `serde_json = "1"` from `[dependencies]`. Re-add it when JSON schema enforcement is needed (Phase 002 spec calls for LLM JSON validation — that work belongs in a future spec).

---

## MEDIUM: Weakened Security Test (`validate_path_rejects_absolute_path_outside_root`)

**Files:** `tests/path_security_test.rs:74-79`, `src/path/security.rs` (unit test equivalent)  
**Pattern:** The integration test for path-outside-root acceptance accepts both `Err(PathViolation)` AND `Err(Io)`:
```rust
match result {
    Err(GraphtorError::PathViolation { .. } | GraphtorError::Io(_)) => { /* OK */ }
    Ok(_) | Err(_) => panic!("path outside root must be rejected"),
}
```
**Risk:** If path resolution fails for an unrelated reason (e.g., permission error, filesystem quirk), the test passes even though `PathViolation` was never raised. A path-outside-root that the OS happens to reject with EACCES would slip through and be wrongly classified as "correctly rejected."  
**Fix:** The test path (`tempdir.parent().join("other_project")`) doesn't exist. After creating it explicitly with `fs::create_dir_all`, the result should be `Err(PathViolation)` reliably. Then assert `PathViolation` specifically.

---

## MEDIUM: `LocalSource::path` is `String`, not `PathBuf`

**File:** `src/config/source.rs`  
**Pattern:** `path: String` in `LocalSource`. All other path-bearing APIs in the codebase use `std::path::Path` / `PathBuf`.  
**Risk:** Callers must manually convert to `PathBuf` before passing to `validate_path`. There is no compile-time guarantee the string is a valid path. Cross-platform path separator handling is also lost.  
**Discussion:** This is a deliberate YAML/serde trade-off — `PathBuf` deserializes from YAML strings correctly, so the fix has no downside.  
**Fix:** Change `path: String` → `path: std::path::PathBuf` in `LocalSource`. `serde` will deserialize a YAML string into `PathBuf` without extra code. Update test YAML fixtures as needed (no format change required).

---

## MEDIUM: Missing Hash Regression Anchor in Chunk ID Test

**File:** `src/chunk/id.rs`  
**Test:** `known_input_produces_deterministic_64_char_hex`  
**Pattern:** The test checks length (64) and character set (hex), and that two calls with the same input produce the same result — but does NOT assert the *exact* SHA-256 value.  
**Risk:** If the hashing algorithm or separator changes silently (e.g., null byte removed, different byte order), the test passes. Chunk IDs are a correlation key across vector and graph stores; silent algorithm drift would silently break cross-store integrity.  
**Fix:** Add one assertion for the expected hex digest of a fixed input:
```rust
// SHA-256("hello\0docs/guide.md") — computed once, pinned forever
assert_eq!(digest, "3f3b...expected_hash...");
```
Compute the expected value offline and hardcode it. A comment should explain how to recompute.

---

## LOW: `as_tracing_level()` Could Be Private

**File:** `src/logging/init.rs`  
**Pattern:** `pub(crate) fn as_tracing_level(&self) -> tracing::Level` is used only within `init.rs`. No code in `src/` outside this file calls it.  
**Risk:** Minimal — just API surface pollution within the crate. Reduces the `pub(crate)` API footprint unnecessarily.  
**Fix:** Change `pub(crate)` → `fn` (fully private). No callers need updating.

---

## LOW: Duplicated `\\?\` Stripping in Integration Tests

**File:** `tests/path_security_test.rs:19-27` and `:86-94`  
**Pattern:** The Windows verbatim prefix stripping block is copy-pasted twice:
```rust
let canonical_root = {
    let c = std::fs::canonicalize(root.path()).unwrap();
    let s = c.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        std::path::PathBuf::from(stripped)
    } else { c }
};
```
**Risk:** Maintenance burden — any future change to this logic must be applied in both places.  
**Fix:** Extract into a private test helper `fn strip_verbatim(p: &std::path::Path) -> std::path::PathBuf` at the top of the test file.

---

## INFO: Undocumented TOCTOU Limitation in `validate_path`

**File:** `src/path/security.rs`  
**Pattern:** `resolve_path` calls `path.exists()` to decide whether to canonicalize or use the walk-up algorithm. Between the `exists()` check and the subsequent `canonicalize()`, the filesystem can change (file created/deleted/replaced with symlink).  
**Risk:** This is a known, inherent limitation of any filesystem-based path security check. It cannot be eliminated with `std::fs`. The impact is low for this use case (pipeline ingestion, not a web server).  
**Action:** Add a `# Limitations` section to the `validate_path` doc comment noting the TOCTOU window. No code change needed.

---

## INFO: No Symlink Traversal Test

**File:** `tests/path_security_test.rs`  
**Pattern:** None of the 5 integration tests exercise symlinks (e.g., symlink inside root pointing outside root).  
**Risk:** The existing implementation handles symlinks for *existing* paths correctly (canonicalize follows them), but the non-existent-path walk-up algorithm does not. A symlink at an intermediate component that points outside root would not be detected.  
**Action:** Track as a known limitation. Document in `validate_path` doc comment. No code change required now; a future security-hardening spec should add `O_PATH`/`openat`-based validation.

---

## SUMMARY

| # | Severity | File | Title |
|---|----------|------|-------|
| 1 | BLOCKER | `Cargo.toml` | Remove unused `serde_json` dependency |
| 2 | MEDIUM | `tests/path_security_test.rs` | Strengthen outside-root rejection test |
| 3 | MEDIUM | `src/config/source.rs` | `LocalSource::path` → `PathBuf` |
| 4 | MEDIUM | `src/chunk/id.rs` | Pin exact SHA-256 regression anchor |
| 5 | LOW | `src/logging/init.rs` | `as_tracing_level` → private |
| 6 | LOW | `tests/path_security_test.rs` | Extract `\\?\` stripping into helper |
| 7 | INFO | `src/path/security.rs` | Document TOCTOU limitation |
| 8 | INFO | `tests/path_security_test.rs` | Note missing symlink test coverage |

Findings 1, 3, 4, 5, 6, 7 can all be fixed on this branch before merge.  
Finding 2 requires careful thought (creating the "other_project" dir may be OS-dependent).
