---
title: "Mark Binary Test Fixtures as Binary in .gitattributes to Prevent CRLF Corruption"
description: "Pattern for preventing git line-ending conversion from silently corrupting binary test fixtures (PDFs, images, etc.) on Windows by declaring them binary in .gitattributes"
date: 2026-05-05
tags: [git, testing, fixtures, windows, pdf, binary]
---

## Context

PR #28 added a minimal valid PDF fixture (`tests/fixtures/sample_heading.pdf`) as a hand-crafted
binary file. No `.gitattributes` entry was created for it. On the next `git add` + `git commit`
on Windows (with `core.autocrlf = true`), git converted all `\n` bytes to `\r\n` in the file,
making the xref table offsets wrong and producing a file that `pdf_extract::Document::load_mem`
rejected with:

```
Parse { message: "pdf load failed: couldn't parse input: invalid file trailer", ... }
```

The test `parse_pdf_heading_aware_real_pdf` had been passing before the commit but failed after
checkout because the stored fixture was CRLF-corrupted. `git check-attr text` showed the file
had `text: unspecified`, meaning git applied its default CRLF handling.

## Diagnosis

```powershell
$bytes = [System.IO.File]::ReadAllBytes("tests\fixtures\sample_heading.pdf")
$crlf = 0
for ($i = 0; $i -lt $bytes.Length - 1; $i++) {
    if ($bytes[$i] -eq 13 -and $bytes[$i+1] -eq 10) { $crlf++ }
}
Write-Host "CRLF count: $crlf"   # Expected: 0 for a valid PDF; was: 43
```

## Pattern

### Step 1 — Add `.gitattributes` at the repository root

```
# .gitattributes
# Binary file declarations — prevent git line-ending conversion
tests/fixtures/*.pdf    binary
tests/fixtures/*.png    binary
tests/fixtures/*.jpg    binary
tests/fixtures/*.docx   binary
tests/fixtures/*.xlsx   binary
```

Add entries for every binary format used as a test fixture. The `binary` attribute is shorthand
for `-text -diff` — it disables line-ending normalization and diff text generation.

### Step 2 — Regenerate any already-corrupted fixtures

If the fixture was already committed with CRLF, it must be regenerated from source. For PDFs:

```python
# generate_fixture.py — always open with 'wb' (binary write)
import io

def build_pdf_bytes() -> bytes:
    LF = b'\n'
    # ... construct content with explicit LF separators only ...
    return content

with open("tests/fixtures/sample.pdf", "wb") as f:
    f.write(build_pdf_bytes())
```

Key rules for generating binary fixtures:
- Always use `open(..., "wb")` — never `"w"` (which applies platform line endings)
- Never pass PDF content through a Python `str` — keep it as `bytes` throughout
- Verify zero CRLF sequences before committing: `content.count(b'\r\n') == 0`

### Step 3 — Verify after `git add`

```bash
git check-attr text tests/fixtures/sample.pdf
# Expected: tests/fixtures/sample.pdf: text: unset   (binary attribute applied)
```

If you see `text: unspecified` the `.gitattributes` entry is not matching.

## Why This Matters

Binary formats like PDF, PNG, DOCX, and XLSX have byte-level invariants:

- **PDF**: xref table byte offsets must point to exact byte positions. One extra `\r` per line
  shifts every offset by the number of preceding line breaks, invalidating the entire xref.
- **PNG**: the IHDR chunk's CRC is computed over exact bytes; CRLF conversion corrupts it.
- **DOCX/XLSX**: ZIP archives; any byte change breaks the zip central directory.

The error produced (e.g., "invalid file trailer" for PDF) is cryptic and does not mention
line endings. Always check `git check-attr` and inspect the file for `\r\n` sequences when a
binary fixture load fails unexpectedly after a commit.

## Evidence

- PR #28: `tests/fixtures/sample_heading.pdf` committed without binary marker
- PR #30 (fix): Added `.gitattributes`, regenerated fixture — `parse_pdf_heading_aware_real_pdf`
  passed after fix
- `git check-attr text tests/fixtures/sample_heading.pdf` returned `text: unspecified` before fix
