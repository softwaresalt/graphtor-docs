---
type: session-memory
timestamp: 2026-05-02T20:09:00Z
agent: Ship
shipment: 012-S
pr: 19
branch: feat/multi-format-source-registry
merge_sha: 92358b1
---

## Session: 012-S Multi-Format Pipeline Maturity — Ship & Merge

### Outcome

Shipment 012-S shipped successfully. PR #19 merged to `main` at `92358b1`.
All 349 tests passing. 012-S archived.

### What Was Shipped

**Feature 021-F** (Multi-Format Source Registry) decomposed into 4 tasks:

| Task | Description | Status |
|------|-------------|--------|
| 021.002-T | `formats` field in config schema (`GitSource`, `LocalSource`, `UrlSource`) | done |
| 021.003-T | Config validation (`VALID_FORMATS`, `validate_formats()`) | done |
| 021.004-T | Pipeline format filtering (`is_format_allowed()`, `skipped_by_format`) | done |
| 021.001-T | Integration tests (6 tests in `tests/pipeline_format_test.rs`) | done |

26 files changed, 836 insertions.

### Copilot Review Remediation (PR #19)

4 comments addressed in commit `ce25f99`:

1. **`VALID_FORMATS` exhaustiveness** — Added `"markdown"` to `VALID_FORMATS` in `config/validation.rs`
2. **`validate_format_list()` case sensitivity** — Added `to_ascii_lowercase()` normalization in `acquire/plan.rs`
3. **`validate_formats()` case sensitivity** — Added `to_ascii_lowercase()` in `config/validation.rs`
4. **`.markdown` extension filtering** — Added `ext == "markdown" → "md"` canonicalization in `pipeline/mod.rs` before allow-list check

All 4 threads replied to and resolved via `gh api graphql resolveReviewThread`.

### New Stash Entry

`2428F33E` (feature, medium) — Full documentation coverage for graphtor-docs.
Awaiting Stage triage.

### Decisions

- `.markdown` → `"md"` canonicalization done pre-allow-list (not in dispatch arm) to keep the filtering stage authoritative
- `"markdown"` added to `VALID_FORMATS` so users can write `formats: [markdown]` in `sources.yaml`
- `--admin` merge required because branch protection requires an approving review; operator explicitly approved in chat

### Next Steps

- Stage stash entry `2428F33E` (full documentation coverage)
- Consider adding branch protection rule to accept owner comments as approval, or add auto-approve workflow
