---
date: 2026-05-05
session: pr28-pr31-review-followups
prs: [30, 31]
---

# Session Memory: PR #28–#31 Copilot Review Follow-ups

## What Was Done

Applied Copilot review follow-up fixes across two PRs:

### PR #30 — Five fixes for PR #28 review comments

All five Copilot review threads on the merged PR #28 (PDF Pass 2 `output_doc_page` refactor):

1. **Weak test assertion** — strengthened `parse_pdf_heading_aware_real_pdf` to assert
   `heading_hierarchy` contains `"Introduction"` (not just non-empty). The prior check
   would pass even if heading detection regressed to the uniform-font fallback path,
   which produces `["Page N"]` hierarchies.

2. **Broken script reference** — removed `scripts/gen_test_pdf.py` reference from test
   doc comment; replaced with accurate hand-crafted fixture description.

3. **Missing `page_num` in uniform-font error** — error message now includes
   `at page {page_num}` for per-page attribution.

4. **Missing `page_num` in heading-aware error** — same fix for the heading-aware path.

5. **Stale legacy API section headers** — updated `extract_text_from_mem` and
   `extract_text_from_mem_by_pages` section headers to read
   "not used in current implementation".

**Root cause discovered during fixes**: `tests/fixtures/sample_heading.pdf` had no
`.gitattributes` binary marker. Git applied CRLF conversion on Windows, corrupting the PDF
xref byte offsets. `pdf_extract` rejected it with "invalid file trailer". Fix: created
`.gitattributes` with `tests/fixtures/*.pdf binary`; regenerated fixture via Python `'wb'`
mode with verified-LF-only bytes.

### PR #31 — One fix for PR #30 review comment

Copilot caught that the `OutputDev trait` code example in the compound doc still had
`// Process all pages (used for both passes)` on `output_doc`, directly contradicting
the Two-Pass Architecture section. Updated inline comments to accurately reflect that
`output_doc` is not used and `output_doc_page` is the active API.

## Files Changed

| File | Change |
|------|--------|
| `tests/parse_pdf_test.rs` | Strengthened heading assertion; removed broken script ref |
| `src/parse/pdf.rs` | Added `page_num` to both per-page error messages |
| `docs/compound/pdf-extract-api-usage-pattern-2026-05-01.md` | Updated legacy API headers + OutputDev example comments |
| `tests/fixtures/sample_heading.pdf` | Regenerated with correct LF-only bytes |
| `.gitattributes` | Created; marks `tests/fixtures/*.pdf` as binary |

## Compound Learnings Written

- `docs/compound/best-practices/git-binary-test-fixtures-gitattributes-2026-05-05.md`
  — Pattern for preventing git CRLF corruption of binary test fixtures
- `docs/compound/best-practices/pdf-pass2-output-doc-page-loop-2026-05-05.md`
  — Updated error message examples to include `page_num`

## Key Insight

Binary test fixtures committed on Windows without a `.gitattributes` `binary` attribute
will be silently CRLF-corrupted. The resulting parse error (e.g., "invalid file trailer"
for PDF) gives no hint that line endings are the cause. Always add `.gitattributes` entries
for binary formats before committing the first fixture of that type.

## Process Notes

- All 6 Copilot review threads replied to and resolved via `gh api graphql resolveReviewThread`
- Both PRs merged after CI pass (~6 min each)
- No tracked shipments involved — these were ad-hoc review follow-up fixes
