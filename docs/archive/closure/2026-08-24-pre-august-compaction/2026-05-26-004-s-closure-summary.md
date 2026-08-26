---
title: "Closure Summary: 004-S query and serve layer"
date: 2026-05-26
shipment: 004-S
compacted_from:
  - docs/closure/2026-04-30-004-s-query-serve-layer-closure.md
---

## Summary

Shipment `004-S` delivered incremental sync plus the MCP query and traversal
surface in PR `#10`. The closure record confirmed the sync-state, diff, and
reingest flow for local sources and the STDIO-only MCP server surface for
`search_local_docs` and `traverse_doc_links`.

## Consolidated verification

* Added sync-state persistence, git-diff detection, mtime scanning, and
  surgical reingest orchestration under `src/sync/`
* Added the MCP server, formatters, and async STDIO entry point under
  `src/mcp/` and `src/main.rs`
* Preserved the path invariant that stored document paths stay
  source-relative, with the key review fix converting strip-prefix fallback
  into an explicit pipeline error instead of silently storing absolute paths

## Readiness conditions

* The shipment was ready with conditions because backlog `013-F` retained the
  known follow-ups: broken `source_id` filtering, swallowed state-builder
  errors, dropped mtime walk errors, discarded embedding output during
  reingest, and one sync-doc mismatch
* These conditions did not block normal local use, but they remained the
  explicit follow-up queue for the next sync hardening pass

## Healthy and failure signals

* Healthy signals: `graphtor-docs sync` creates or updates `.sync_state.json`,
  MCP search returns chunk IDs, traversal returns related chunks, and stored
  paths remain relative to the source root
* Failure signals: startup `PathViolation`, schema errors against the local
  database, sync failures on files outside the configured root, or empty search
  results before any content has been indexed

## Rollback and follow-up

* Roll back by rebuilding a prior binary and recreating `.graphtor/graph.db`
  plus `.sync_state.json` when local state is suspect
* Continue follow-up in backlog `013-F` and the related documentation update
  for the newer rmcp API guidance

## Archived originals

The original detailed closure record was moved to `docs/archive/closure/2026-04-30/`.
