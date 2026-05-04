---
title: "Optional dual-backend pattern for library architectural limitations"
description: "When a dependency has an unfixable bottleneck in its core path, add an optional higher-performance backend with graceful fallback"
problem_type: "architectural_limitation"
category: "best-practices"
component: "src/parse/pdf.rs"
root_cause: "lopdf eagerly parses all xref entries serially in load_mem(), causing >20 min DNF for 104 MB PDFs — no downstream optimization can fix this"
resolution_type: "design_change"
severity: "high"
message: "DNF after 20+ minutes on large PDF ingestion"
file_path: "src/parse/pdf.rs"
citations:
  - "docs/decisions/2026-05-04-pdfium-dual-backend-spike.md"
  - "docs/decisions/2026-05-04-pdfium-dual-backend-deliberation.md"
  - "docs/exec-plans/2026-05-04-pdfium-dual-backend-plan.md"
  - "PR #25"
tags:
  - "pdf"
  - "pdfium"
  - "dual-backend"
  - "performance"
  - "optional-dependency"
  - "graceful-fallback"
---

## Problem

After shipping 023-F optimizations (PageTextAccumulator, histogram sampling, logging suppression), small/medium PDFs performed well but 104 MB PDFs still DNF'd after 20+ minutes. The bottleneck was `lopdf::Document::load_mem()` eagerly decompressing all 50K+ xref entries — an architectural limitation inside the dependency that no amount of wrapper optimization could address.

## Root Cause

When a library's **core loading path** has an architectural serial bottleneck, optimizing the code that runs *after* loading is futile. The prior spike (2026-05-03) correctly identified the bottleneck but incorrectly rejected pdfium-render because it "requires a runtime DLL." The reframe: an optional runtime library loaded via `dlopen`/`LoadLibrary` does NOT violate the "zero runtime dependencies" principle — the binary works identically without it.

## Resolution

Implemented a dual-backend pattern:

1. **Size-based routing**: PDFs ≥ 20 MiB → try `PdfiumBackend` first
2. **Error discrimination**: `PdfiumBindError::NotAvailable` (expected, fall back) vs `ExtractionFailed` (unexpected, log error + fall back)
3. **Three-tier DLL discovery**: env var → exe dir → system path
4. **Graceful degradation**: without the DLL, behavior is identical to before

Key design decisions:
- No shared trait between backends (premature with only 2)
- Page-based chunking only for pdfium path (heading-aware is follow-up)
- Match-based error handling instead of `From` impl (simpler for fallback pattern)

## Prevention

When a library dependency causes performance issues:

1. **Isolate the bottleneck** first (pdf_diag.rs proved it was `load_mem`, not text extraction)
2. **Check if the bottleneck is architectural** — if it's in the library's core loading/initialization path, optimizing around it won't help
3. **Consider optional dual-backend** with graceful fallback rather than replacing the dependency entirely (preserves existing quality for the common case)
4. **Distinguish "optional runtime enhancement" from "runtime dependency"** — dynamic loading of an optional library is acceptable under local-first principles
