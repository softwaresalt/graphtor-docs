---
title: "BaseFrontmatter JSON Schema export workflow"
description: "How to regenerate the docline BaseFrontmatter v1 JSON Schema and how graphtor-docs consumes it."
date: "2026-06-02"
status: "current"
tags:
  - "schema"
  - "frontmatter"
  - "graphtor-docs"
  - "contract"
---

## Purpose

docline emits document frontmatter using the Pydantic model `BaseFrontmatter`
(`src/docline/schema/models.py`). Downstream consumers — primarily
`graphtor-docs` — validate ingested documents against a published JSON Schema
representation of that model.

The exported schema is **generated on demand** from the live Pydantic model.
A snapshot is committed alongside this workflow doc at
[`base-frontmatter-v1.schema.json`](./base-frontmatter-v1.schema.json) for
convenience — graphtor-docs can either consume that file directly or
regenerate from the live model. The snapshot is byte-stable across
successive exports (the JSON serializer uses `sort_keys=True` and two-space
indentation).

## Surfaces

| Surface | Invocation | Output |
|---|---|---|
| CLI | `docline export-schema` | JSON Schema document on stdout |
| MCP | `export_schema()` method on `DoclineMcpServer` | JSON Schema document as a string |
| Python | `docline.schema.export.export_base_frontmatter_schema_json()` | JSON Schema document as a string |
| Python (dict) | `docline.schema.export.export_base_frontmatter_schema()` | JSON Schema as a `dict[str, Any]` |

All four surfaces emit the same Draft 2020-12 document with the stable
contract identifier:

```text
$schema: https://json-schema.org/draft/2020-12/schema
$id:     https://docline.softwaresalt.dev/schema/base-frontmatter/v1.json
```

## Regenerating the schema

To capture the current schema as a file (for example, to publish it alongside
`graphtor-docs` or to diff it against a prior version):

```bash
docline export-schema > base-frontmatter-v1.schema.json
```

Or from Python:

```python
from pathlib import Path

from docline.schema.export import export_base_frontmatter_schema_json

Path("base-frontmatter-v1.schema.json").write_text(
    export_base_frontmatter_schema_json(),
    encoding="utf-8",
)
```

Or from MCP:

```python
schema_text = server.export_schema()
```

No build step or filesystem side effect is required. The schema is always a
deterministic function of the installed `docline` version.

## How graphtor-docs consumes the schema

`graphtor-docs` (Rust MCP server, CozoDB chunk store) validates incoming
docline-emitted markdown frontmatter against this schema during ingest. The
contract guarantees:

* Required fields: `description`, `content_sha256`, `source_path`,
  `chunk_strategy` (default `"h1-h2-h3"`), `schema_version` (initial `"1.0"`).
* `source_path` is always a forward-slash POSIX string (see PA-2 in the
  shipment plan).
* `content_sha256` is the SHA-256 hex digest of the emitted markdown body
  bytes.
* docline-specific fields are namespaced under `docline:` and are advisory
  rather than required for graphtor-docs ingest.

`graphtor-docs` should pin against a specific `schema_version`. The published
`$id` ends in `/v1.json`; future incompatible revisions will publish a `/v2.json`
sibling rather than mutating `/v1.json`.

## Schema versioning policy

Initial value: `schema_version: "1.0"`. SemVer additive-minor policy:

| Change | Bump |
|---|---|
| Optional field added | MINOR |
| Field type widened (back-compat) | MINOR |
| Required field added | MAJOR |
| Field removed | MAJOR |
| Field semantics changed | MAJOR |

A MAJOR bump publishes a new `$id` (for example `v2.json`); `graphtor-docs`
treats unknown major versions as ingest errors.

## Regression test

The repository includes `tests/schema/test_schema_export.py` which exercises
the export surface and asserts determinism across successive invocations.
There is no committed schema artifact to drift against; the live model is the
source of truth.

## Related artifacts

* `src/docline/schema/models.py` — `BaseFrontmatter` definition
* `src/docline/schema/export.py` — export helpers
* `src/docline/cli.py` — `export-schema` subcommand
* `src/docline/mcp/server.py` — `export_schema()` MCP method
* `docs/plans/2026-06-02-docline-graphtor-alignment-plan.md` — shipment plan
* `docs/decisions/2026-06-02-docline-graphtor-alignment-deliberation.md` — contract decision
