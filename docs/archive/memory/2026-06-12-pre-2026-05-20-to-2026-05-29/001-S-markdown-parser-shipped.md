---
shipment: 001-S
feature: 003-F
pr: 4
merge_commit: 1612dbb
shipped_at: 2026-04-29
---

# Shipment 001-S — Markdown Parser Shipped

## What Was Built

Full pulldown-cmark based markdown parsing pipeline (`src/parse/`):

| Module | Responsibility |
|---|---|
| `ast.rs` | Stack-based pulldown-cmark event walker → `Vec<AstNode>` |
| `frontmatter.rs` | YAML frontmatter strip (byte-0 only, no offset drift) |
| `chunker.rs` | H1/H2/H3 heading-based splitter → `Vec<Chunk>` |
| `links.rs` | Hyperlink extractor with anchor splitting |
| `code.rs` | Code block isolator with composite snippet IDs |
| `types.rs` | All pipeline data structures |
| `mod.rs` | `parse_document()` unified entry point |

177 tests, all passing. 6 integration, 41 unit tests across 6 test files.

## Hard-Won Solutions

### AST Walker — Stack-Based State Machine

**Problem**: Initial `collect_text_until_end` with index-based iteration consumed
`Start(Paragraph)` events before `Start(Link)` was reached, so link text was lost.

**Solution**: Stack-based state machine with `heading_stack`, `para_stack`,
`link_stack`, `code_stack`. `Event::Text` appends to ALL active non-code
containers simultaneously. Code blocks are exclusive (text not shared).

### Frontmatter Panic on `"---"` (No Trailing `\n`)

**Problem**: Guard `|| trimmed == "---"` admitted a 3-byte string then sliced
at `"---\n".len()` (offset 4) — out-of-bounds panic.

**Solution**: Removed that branch entirely. Frontmatter now requires
`content.starts_with("---\n")` at byte 0. All offset arithmetic is relative
to `content` directly — no `trim_start_matches` that would shift offsets.

### Empty Fenced Code Block Snippet IDs

**Problem**: `generate_chunk_id("", chunk_id)` errors because `content` must
be non-empty.

**Solution**: Composite key `format!("{chunk_id}\0{lang}\0{content}")` as the
content argument — always non-empty, and language-discriminated so two empty
blocks with different language tags produce distinct IDs.

### `cargo fmt` CI Failure on Long Assertions

**Problem**: New regression tests had `assert_eq!(s.id.len(), 64, "long message")`
and `assert_ne!(id_rs, id_py, "long message")` exceeding the line length.
CI fmt gate failed even though local check passed (CRLF line endings masked it).

**Solution**: Run `cargo fmt --all` locally before every push. The reformatted
multi-line style is what CI requires.

### `flush_chunk` Closure Limitation

**Problem**: Cannot use `?` inside a `FnMut` closure returning `()` to propagate
errors up to the outer `Result` function.

**Solution**: Extracted `flush_chunk()` as a named helper function returning
`Result<Option<Chunk>, GraphtorError>` called from a `for` loop.

## `generate_chunk_id` Contract

- Returns `Result<String, GraphtorError>` — rejects empty `content` OR empty `source_path`
- Separator is `\0` (null byte), NOT `|`
- Chunker and code extractor both return `Result` because of this

## What `char_offset` Actually Is

`Chunk::char_offset` is computed from accumulated rendered line lengths after
AST normalization. It is **not** the byte offset in the original source file.
Callers needing precise source positions must track from raw pulldown-cmark events.

## Chunk Content Is Reconstructed

`Chunk::content` is rebuilt from `AstNode` stream — whitespace may differ
from original source. It is normalized markdown, not a raw excerpt.
