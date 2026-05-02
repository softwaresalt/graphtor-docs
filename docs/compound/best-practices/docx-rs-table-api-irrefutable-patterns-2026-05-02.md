---
title: docx-rs 0.4 API — correct types for table traversal and chunk ID uniqueness
tags: [rust, docx-rs, parsing, chunk-id]
date: 2026-05-02
shipment: 011-S
---

## Problem

`docx-rs` 0.4's table API differs from naive expectations in two critical ways,
and the API surface is not well documented in crate docs.

### 1. `TableCellContent` not `TableCellChild`

`TableCell::children` is `Vec<TableCellContent>` — not `TableCellChild`. Using
`TableCellChild` produces a compile error (`error[E0412]: cannot find type`).

### 2. Irrefutable patterns for `TableChild` and `TableRowChild`

`TableChild` and `TableRowChild` are single-variant enums. Using a `let ... else`
refutable pattern triggers a Clippy error:

```
error: irrefutable `let...else` pattern
```

The correct pattern is a direct binding (no `else` branch):

```rust
let TableChild::TableRow(row) = child;     // irrefutable, single variant
let TableRowChild::TableCell(cell) = cell; // irrefutable, single variant
```

## Verified API Shape (docx-rs 0.4.20)

```text
docx_rs::Table
  rows: Vec<TableChild>
    TableChild::TableRow(TableRow)               — only variant

docx_rs::TableRow
  cells: Vec<TableRowChild>
    TableRowChild::TableCell(TableCell)           — only variant

docx_rs::TableCell
  children: Vec<TableCellContent>               — NOT TableCellChild
    TableCellContent::Paragraph(Paragraph)       — NOT Box<Paragraph>
```

`DocumentChild::Paragraph` IS `Box<Paragraph>`, but `TableCellContent::Paragraph`
is NOT boxed. This asymmetry causes subtle compile errors if you copy the
`DocumentChild` match arm.

## chunk_id Collision Prevention

When chunking DOCX documents, `generate_chunk_id(content, source_path)` will
collide if the same boilerplate text (e.g., a recurring heading) appears at
multiple positions in the document. Always include a position discriminator:

```rust
let id = generate_chunk_id(&text, &format!("{source_path}#section={position}"));
```

where `position` is a monotonically incrementing integer per document.

## Reference

Source at `D:\.cargo\registry\src\index.crates.io-*/docx-rs-0.4.20/src/documents/elements/`:
- `table.rs`, `table_row.rs`, `table_cell.rs`, `table_cell_content.rs`
