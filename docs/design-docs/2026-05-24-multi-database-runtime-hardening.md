---
title: Multi-database runtime hardening
description: "Per-database advisory locks, read-only runtime handles, and graceful lock contention for multi-db access"
---

## Context

Multi-database file support let `sync`, `status`, and the MCP server route work
across multiple `.db` files, but concurrent access could still trip SQLite or
CozoDB contention in production-shaped workflows. A sync writer on one database
could block unrelated readers or fail with low-level locking errors.

## Decision

We harden the runtime around database-scoped access instead of a single
workspace-wide lock.

## Runtime rules

* write paths acquire a per-database advisory lock before mutating a database
* read paths open dedicated read-only handles with immutable semantics
* lock files live beside the target database and are scoped by database name
* stale lock files and stale replacement markers are recoverable
* lock contention surfaces as a typed `DatabaseLocked` error instead of a panic

## Surface behavior

* `sync` holds a database lock only for the active database being processed
* `status` can read while a writer holds another database lock
* MCP search and lookup tools use read-only handles so query traffic does not
  contend with write-mode stores
* custom `--db-path` values must stay inside the workspace root before lock
  files are created

## Consequences

Multi-database deployments keep the routing flexibility from shipment `029-S`
while gaining safer concurrent runtime behavior. Operators now get predictable
lock errors, read-only query surfaces stay available during writes, and stale
lock recovery no longer depends on manual cleanup after a crashed process.
