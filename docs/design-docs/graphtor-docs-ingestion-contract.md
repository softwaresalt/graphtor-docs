---
title: "docline → graphtor-docs ingestion contract"
description: "Stable v1 contract surface that docline emits and graphtor-docs ingests."
date: "2026-06-03"
status: "current"
tags:
  - "contract"
  - "frontmatter"
  - "graphtor-docs"
  - "ingestion"
  - "schema"
---

## Purpose

docline produces normalized Markdown documents that downstream consumers — primarily
[`graphtor-docs`](https://github.com/softwaresalt/graphtor-docs) — ingest into a
documentation graph. This document defines the **v1 ingestion contract**: the
exact frontmatter surface, chunk-boundary rules, hashing algorithm, path
normalization, schema-versioning policy, supported Markdown features, and the
stability guarantees that ingestion consumers may rely on.

This contract is the durable interface between the two repositories. Changes to
the surfaces below MUST follow the SemVer rules in
[Stability guarantees](#stability-guarantees).

## Frontmatter v1 surface

Every document docline emits has YAML frontmatter conforming to the
`BaseFrontmatter` Pydantic model in `src/docline/schema/models.py`. The
authoritative machine-readable form is the JSON Schema exported via the
[schema export workflow](schema-export-workflow.md).

| Field | Type | Required | Default | Description |
|---|---|---|---|---|
| `title` | string | yes | — | Human-readable document title. Must be non-empty. |
| `source` | string | yes | — | Origin URI or path of the source document. Must be non-empty. |
| `ingested_at` | datetime (ISO 8601) | yes | — | UTC timestamp when docline ingested the source. |
| `doc_type` | string | yes | — | Document-type identifier (for example, `pdf`, `docx`, `vtt`, `html`). |
| `description` | string | no | `""` | Short human-readable description. |
| `content_sha256` | string (64-char hex) | no | `""` | See [content_sha256 algorithm](#content_sha256-algorithm). |
| `source_path` | string (POSIX) | no | `""` | See [source_path normalization](#source_path-normalization). |
| `chunk_strategy` | string | no | `"h1-h2-h3"` | Chunk-boundary strategy identifier; see [Chunk-boundary rules](#chunk-boundary-rules). |
| `schema_version` | string (SemVer) | no | `"1.0"` | Contract version this document conforms to. |
| `docline` | object \| null | no | `null` | Docline-only namespace; see [`docline` namespace](#docline-namespace). |

### `docline` namespace

The `docline` field is an optional object that holds metadata internal to
docline. Keys placed inside this namespace are **intentionally not promoted**
to the top-level frontmatter surface so they cannot be mistaken for contract
fields. graphtor-docs MAY read the namespace for diagnostic purposes but MUST
NOT treat its contents as part of the stable contract.

## Chunk-boundary rules

The default `chunk_strategy` is `"h1-h2-h3"`. Under this strategy a new chunk
begins at every ATX heading at level 1, 2, or 3 (`#`, `##`, `###`). Headings at
level 4 and deeper (`####` and beyond) are content within the enclosing chunk
and do not introduce a new chunk boundary.

Constraints:

* Heading hierarchy is validated by `validate_heading_hierarchy` in
  `src/docline/process/heading_validation.py`. A document MUST go H1 → H2 → H3
  without skipping a level. Disorder raises a validation error unless the
  assembler is invoked with `allow_heading_disorder=True` (test-only escape).
* Headings inside fenced code blocks (` ``` ` or `~~~`) are content and never
  introduce a chunk boundary.

When the assembler is invoked with `emit_chunk_anchors=True`, docline injects
an HTML anchor immediately before each H1/H2/H3 heading:

```html
<a id="chunk-NNNN"></a>
```

`NNNN` is a 1-based, zero-padded four-digit counter that increases
monotonically across the document body. Anchors are emitted only for H1/H2/H3
headings and are skipped inside fenced code blocks. Default is `False` so
existing output is byte-stable for consumers that do not need anchors.

## content_sha256 algorithm

`content_sha256` is the SHA-256 hex digest of the assembled Markdown **body**
bytes, UTF-8 encoded. The implementation lives in
`src/docline/process/hashing.py::compute_content_sha256` and is equivalent to:

```python
hashlib.sha256(body.encode("utf-8")).hexdigest()
```

Notes:

* The digest covers the body only. The YAML frontmatter is **not** included.
* The output is always 64 lowercase hex characters.
* The body bytes are the exact bytes emitted between the closing `---` line
  and the end of the document, including the trailing newline.
* Hashes are deterministic across operating systems because the body is
  serialized with `\n` line endings before the digest is computed.

## source_path normalization

`source_path` is the project-relative path of the source artifact, normalized
to POSIX form (forward slashes). The normalization is performed by
`src/docline/paths.py::posixify_path` and is idempotent: applying it twice
yields the same result.

Rules:

* Path separators are converted from any platform form (for example, Windows
  `\\`) to `/`.
* Paths are relative to the repository or workspace root, not absolute.
* Empty input yields the empty string.
* Drive letters and absolute prefixes are rejected upstream by
  `safe_workspace_path` to keep ingestion within workspace bounds.

## schema_version policy

`schema_version` is a SemVer string identifying the contract revision the
document conforms to.

| Change | Version bump |
|---|---|
| Adding a new optional field with a safe default | MINOR (`1.0` → `1.1`) |
| Documenting a previously implicit rule | PATCH (`1.0` → `1.0.1`) |
| Removing, renaming, or retyping a field | MAJOR (`1.x` → `2.0`) |
| Changing the meaning of an existing field | MAJOR |
| Changing the default `chunk_strategy` | MAJOR |

The 1.x line is **additive only**. Consumers written against `1.0` MUST remain
forward-compatible with any `1.y` document. Breaking changes require a `2.0`
release and a documented migration path.

## Supported Markdown features

docline emits CommonMark with the following GitHub-Flavored Markdown (GFM)
extensions:

* Tables
* Footnotes
* Strikethrough
* Fenced code blocks with language identifiers

Unsupported Markdown features (for example, raw HTML beyond the chunk-anchor
elements, custom directives, mathematics blocks) are not part of the v1
contract and MAY appear pass-through but are not guaranteed to render
correctly downstream.

## Stability guarantees

The v1 contract carries the following guarantees:

* **Additive evolution.** New optional fields with safe defaults MAY be added
  within the 1.x line. Consumers that ignore unknown fields will continue to
  function.
* **Byte-stability of defaults.** Documents emitted with all defaults remain
  byte-identical across patch and minor releases. Opt-in features such as
  `emit_chunk_anchors=True` do not affect default output.
* **Deprecation policy.** A field marked deprecated in `1.y` MUST remain
  present and behaviorally unchanged through the remainder of the 1.x line.
  Removal is permitted only at the next MAJOR boundary.
* **Schema export determinism.** Successive runs of the schema export workflow
  produce byte-identical JSON. See
  [schema-export-workflow.md](schema-export-workflow.md).

## Cross-references

* [graphtor-docs repository](https://github.com/softwaresalt/graphtor-docs) —
  ingestion-side implementation of this contract.
* [BaseFrontmatter JSON Schema export workflow](schema-export-workflow.md) —
  how to regenerate the machine-readable contract surface.
* [Document ingestion and validation pipeline design](DocumentIngestion&ValidationPipelineDesign.md) —
  internal pipeline that produces contract-conformant output.
