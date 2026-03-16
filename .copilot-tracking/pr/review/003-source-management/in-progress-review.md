<!-- markdownlint-disable-file -->
# PR Review Status: 003-source-management

## Review Status

* Phase: 4 — Finalized
* Last Updated: 2026-06-10
* Summary: Source Registry & Acquisition PR — 6 genuine findings across correctness, security, and reliability; 4 test-gap observations; no blocking security holes but 1 high-severity correctness bug and 1 medium-security path handling issue.

## Branch and Metadata

* Normalized Branch: `003-source-management`
* Source Branch: `003-source-management`
* Base Branch: `main`
* Commits: 11
* Files Changed: 39 (4395 insertions, 6 deletions)
* Linked Work Items: FG-002 — Source Registry & Acquisition

## Actions Taken

1. Read all 6 `src/acquire/*.rs` source files directly
2. Read all 4 `tests/acquire_*.rs` integration test files
3. Read `src/path/security.rs` — path validation implementation
4. Read `src/error/types.rs` — error hierarchy
5. Read `src/config/source.rs` — source configuration types
6. Read `src/config/validation.rs` — config validation rules
7. Read `src/lib.rs`, `build.rs`, `Cargo.toml`
8. Read `.github/copilot-instructions.md` — project conventions

## Diff Mapping

| File | Type | New Lines | Notes |
|------|------|-----------|-------|
| `src/acquire/result.rs` | New | 1–366 | Data types; no logic bugs |
| `src/acquire/filter.rs` | New | 1–277 | **RI-001 — glob vs absolute paths** |
| `src/acquire/git.rs` | New | 1–111 | RI-003, RI-004, RI-006 |
| `src/acquire/local.rs` | New | 1–76 | Clean; fail-fast on walkdir error (by design) |
| `src/acquire/plan.rs` | New | 1–219 | **RI-002 — discarded canonical path**, RI-005, RI-007, RI-009 |
| `src/acquire/mod.rs` | New | 1–216 | Clean orchestration |
| `build.rs` | New | 1–9 | Correct Windows MSVC advapi32 pragma |
| `Cargo.toml` | Modified | — | Adds `git2`, `walkdir`, `globset` |
| `src/lib.rs` | Modified | — | Re-exports acquire types |
| `tests/acquire_filter_test.rs` | New | 1–133 | RI-008 gap noted |
| `tests/acquire_git_test.rs` | New | 1–177 | RI-003 gap noted |
| `tests/acquire_local_test.rs` | New | 1–123 | Clean |
| `tests/acquire_plan_test.rs` | New | 1–292 | RI-009 gap noted |

## Instruction Files Reviewed

* `.github/copilot-instructions.md`: Python-centric guidelines (project was originally Python). Rust-specific rules stated in PR description (doc comments with `# Errors`, `GraphtorError`, no `unwrap` in production). Applied those as the authoritative Rust convention set.

## Review Items

### ✅ Approved for PR Comment

All 6 review items below are approved for submission.

---

#### RI-001 — Glob filtering applied to absolute paths; path-specific patterns silently fail ⚠️

* File: `src/acquire/filter.rs` + `src/acquire/mod.rs`
* Lines: filter.rs 38–58; mod.rs 202–215
* Category: Correctness / API Semantics
* Severity: HIGH

See handoff for full details.

---

#### RI-002 — `resolve_source_action` discards the canonical path returned by `validate_path` 🔒

* File: `src/acquire/plan.rs`
* Lines: 152–161
* Category: Security / Path Handling
* Severity: MEDIUM-HIGH

See handoff for full details.

---

#### RI-003 — Shallow clone fallback uses fragile libgit2 message string matching ⚠️

* File: `src/acquire/git.rs`
* Lines: 51–53
* Category: Reliability
* Severity: MEDIUM

See handoff for full details.

---

#### RI-004 — Silent error discard in shallow-clone fallback cleanup ⚠️

* File: `src/acquire/git.rs`
* Lines: 57–60
* Category: Reliability / Debuggability
* Severity: MEDIUM

See handoff for full details.

---

#### RI-005 — `AcquisitionPlan.allowed_root` stored without canonicalization ⚠️

* File: `src/acquire/plan.rs`
* Lines: 68–75
* Category: Design Consistency
* Severity: LOW-MEDIUM

See handoff for full details.

---

#### RI-006 — `git_error_to_pipeline` is unnecessarily `pub` 💡

* File: `src/acquire/git.rs`
* Lines: 105–110
* Category: API Design
* Severity: LOW

See handoff for full details.

---

### 🔍 Test Gap Observations (Non-blocking)

* **RI-007**: No test for source IDs containing path separators (e.g. `"my/repo"` or `"../escape"`) — config validation doesn't reject them, and they produce non-canonical target paths in the plan.
* **RI-008**: Integration tests for `filter_files` only use `**`-prefixed patterns. No test exercises a non-`**`-prefixed pattern (e.g. `docs/**/*.md`) against absolute paths from `scan_local_source`.
* **RI-009**: `validate_sources` does not check path traversal for non-existent local paths — only reports "path does not exist" rather than also flagging traversal attempts.

### ❌ Rejected / No Action

* `walkdir` fail-fast on first error — intentional design choice; best-effort scanning not in scope.
* `execute` signature and `#[must_use]` placement — correct as implemented.
* Error message formatting — not commented on (style, not correctness).

## Next Steps

* [x] All review items documented and approved for handoff
* [x] `handoff.md` generated with PR-ready comments
