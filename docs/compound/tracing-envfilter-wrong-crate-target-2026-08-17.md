---
title: "tracing EnvFilter target must match the crate the warn!/info! call is actually compiled into"
description: "A scoped-subscriber test capture silently drops all events when the EnvFilter directive names the wrong crate (e.g. the library crate name instead of the binary crate it's actually compiled into)"
problem_type: "flaky_test"
category: "test-failures"
component: "src/workspace/serve_discovery.rs test module (tracing log-capture tests)"
root_cause: "graphtor-docs is a single package with BOTH a library crate (graphtor_core, src/lib.rs) and a binary crate (graphtor-docs / module path graphtor_docs, src/main.rs); tracing events default their `target` to the module path of the crate the call-site is compiled into, not the package name — an EnvFilter directive scoped to the wrong crate name matches nothing and silently drops every event"
resolution_type: "workaround"
severity: "medium"
message: "captured log buffer is empty even though tracing::subscriber::with_default was used, rebuild_interest_cache() was called, and the code under test definitely ran and definitely called warn!/info!"
file_path: "src/workspace/serve_discovery.rs"
citations:
  - "PR #101 (shipment 048-S) — commit 73454f4 introduced the affected test, fixed forward in the same PR"
  - "docs/compound/tracing-callsite-interest-cache-parallel-test-race.md (a distinct root cause with the same symptom)"
tags:
  - "tracing"
  - "test-flakiness"
  - "cargo-test"
  - "rust"
  - "envfilter"
  - "crate-boundary"
---

## Problem

A new unit test in `src/workspace/serve_discovery.rs` (compiled into the `graphtor-docs`
**binary** crate) needed to capture a `tracing::warn!` event and assert on its content, following
the established `capture_warn_logs_once`/`EnvFilter` pattern already used successfully in
`src/db/store.rs` (compiled into the `graphtor_core` **library** crate). The pattern was copied
verbatim, including `tracing_subscriber::EnvFilter::new("graphtor_core=info")` — except the level
was changed to `warn` for this new use. The resulting test's captured buffer was **empty on every
single run**, with no exceptions — not merely flaky, but 100% reproducible, even when run alone
with `--test-threads=1`.

## Root Cause

`store.rs`'s helper correctly uses `EnvFilter::new("graphtor_core=info")` because `store.rs` is
part of the `graphtor_core` **library** crate — the event's `target` field (which `tracing`
derives from the call-site's module path by default) is genuinely `graphtor_core::db::store::...`,
so the directive matches.

`serve_discovery.rs`, however, is declared via `mod workspace;` in `src/main.rs` — it is compiled
into the **binary** crate, whose crate name `graphtor-docs` normalizes to the module-path prefix
`graphtor_docs` (hyphens become underscores), NOT `graphtor_core`. The event's actual `target` was
`graphtor_docs::workspace::serve_discovery`, so the copied `EnvFilter::new("graphtor_core=warn")`
directive matched **nothing** — every event was filtered out before the subscriber ever received
it. This is a completely different failure mode from
`docs/compound/tracing-callsite-interest-cache-parallel-test-race.md` (a `tracing-core`
interest-caching race): that bug produces an **intermittent** empty capture that varies with test
parallelism and file/test ordering; this bug produces a **deterministic, 100%-reproducible** empty
capture regardless of how the test is run, because the filter directive is simply targeting a
crate name that never appears in this call-site's events.

## Resolution

Use the module-path prefix of the crate the call-site is **actually compiled into**, not the crate
where an analogous existing helper happens to live:

```rust
// WRONG — serve_discovery.rs is in the graphtor-docs BINARY crate, not graphtor_core:
let filter = tracing_subscriber::EnvFilter::new("graphtor_core=warn");

// RIGHT:
let filter = tracing_subscriber::EnvFilter::new("graphtor_docs=warn");
```

Alternatively — and preferred when the event's specific target matters for **operator-facing**
log filtering (e.g. an existing `RUST_LOG=graphtor_core=warn` convention an operator already
relies on) — override the event's target explicitly at the call-site with `tracing`'s `target:`
macro syntax, so the code's physical crate location and its logical tracing target can be
decoupled on purpose:

```rust
tracing::warn!(
    target: "graphtor_core::acquire::filter",
    input_files = count,
    "filter produced empty file set — all files were excluded"
);
```

When doing this, the test's `EnvFilter` directive must match the **explicit override target**,
not the module the code physically lives in.

## Prevention

* **Before copying a `tracing` test-capture helper to a new module, check which crate that module
  is compiled into** (in a single-package-with-lib-and-bin project like this one, check whether
  the file is reachable via `lib.rs`'s `pub mod` tree or `main.rs`'s `mod` tree) — do not assume
  the EnvFilter string from a donor module transfers unchanged.
* **Diagnose empty-vs-wrong-content separately, but don't stop at "it's the interest-cache race"**:
  if a capture is empty on literally every single run (including alone, including
  `--test-threads=1`), suspect a filter-target mismatch first — the interest-cache race
  (`docs/compound/tracing-callsite-interest-cache-parallel-test-race.md`) is inherently
  probabilistic and should not reproduce 100% of the time even unmitigated.
* **A quick sanity check**: temporarily widen the `EnvFilter` to a bare level (`EnvFilter::new("warn")`,
  no crate scoping) — if that captures the event, the crate-name-scoped directive was wrong; if it
  still captures nothing, look elsewhere (subscriber wiring, level filtering, or the interest-cache
  race).
