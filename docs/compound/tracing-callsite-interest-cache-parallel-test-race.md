---
title: "tracing callsite Interest cache silently drops scoped-subscriber test capture under parallel cargo test"
description: "A tracing::info! call-site shared with many sibling tests that never install a subscriber can permanently cache Interest::never() and silently drop a later test's scoped subscriber under parallel cargo test"
problem_type: "flaky_test"
category: "test-failures"
component: "src/db/store.rs test module (tracing log-capture tests)"
root_cause: "known open tracing-core bug (tokio-rs/tracing#2874, still present in tracing-core 0.1.36): a macro call-site's Interest::never() gets cached the first time it fires with no dispatcher active, and that negative cache is never invalidated once a real dispatcher later becomes active elsewhere in the process"
resolution_type: "workaround"
severity: "medium"
message: "captured log buffer is empty even though tracing::subscriber::with_default was used and the code under test definitely ran"
file_path: "src/db/store.rs"
citations:
  - "docs/archive/memory/2026-08-17/047-s-build-checkpoint-pre-pr.md"
  - "PR #97 (shipment 047-S) — commit adcd1ee introduced the affected test"
  - "https://github.com/tokio-rs/tracing/issues/2874"
tags:
  - "tracing"
  - "test-flakiness"
  - "cargo-test"
  - "rust"
  - "concurrency"
---

## Problem

A new unit test needed to assert on the exact text of a `tracing::info!` log
line emitted by `DataStore::open_engine_readonly` in `src/db/store.rs`. The
standard approach — build a `tracing_subscriber::fmt` subscriber with a
custom `MakeWriter` capturing to an in-memory buffer, then run the code under
test inside `tracing::subscriber::with_default(subscriber, || { ... })` — is
exactly the pattern already used successfully elsewhere in this repo
(`src/main.rs`'s `sync_progress_tests::capture_warn_logs` helper).

But applied to `open_engine_readonly`'s call-site, the captured buffer was
**empty** under default (parallel) `cargo test` execution — not merely
containing the wrong text, but containing *nothing at all* — even though
`open_engine_readonly` definitely ran and definitely called `info!`
(confirmed by adding a temporary `eprintln!` inside the custom writer's
`write()` method, which never fired). Running the exact same test **alone**
(`cargo test --lib <full_test_path>`, matching only that one test) worked
perfectly every time. Running with `--test-threads=1` (serialized, but still
alongside all 9 sibling tests that also call `open_engine_readonly`) also
worked every time. Only **default parallel** execution alongside those
siblings reproduced the empty-capture failure, and it reproduced
**deterministically** (100% of runs), not merely as an occasional flake.

## Root Cause

This matches a known, currently-open `tracing-core` bug:
[tokio-rs/tracing#2874](https://github.com/tokio-rs/tracing/issues/2874),
confirmed still present in `tracing-core 0.1.36` (the version resolved in
this repo's `Cargo.lock`) — not merely "a documented performance
optimization" working as intended. `tracing-core` decides a macro
call-site's subscriber `Interest` (`never()`/`sometimes()`/`always()`) the
first time that call-site fires in the process, then caches that decision
globally. The bug: if that first fire happens on a thread where **no
dispatcher is active yet** (the built-in no-op default), the callsite gets
`Interest::never()` cached — and, per #2874, this negative cache is **never
invalidated**, even once a real dispatcher later becomes active elsewhere
in the process. The `open_engine_readonly` call-site is shared with 9 other
characterization tests in the same file (verified: `Select-String -Pattern
"DataStore::open_engine_readonly\("` across `src/db/store.rs`'s test module
returns 10 call sites total — the 9 pre-existing siblings plus this one)
that call it **without ever installing any tracing subscriber**. Under
`cargo test`'s default multi-threaded execution, several of those sibling
test threads race to be the first to touch that call-site, and whichever
one wins — very plausibly one of those 9 "no subscriber" threads, given
only 1 of the ~10 threads touching this call-site carries a real subscriber
— triggers exactly the #2874 failure mode: `Interest::never()` gets cached
and stays cached, so the `info!` macro short-circuits **before ever
consulting the currently active dispatcher**, for every future call to that
call-site, on every thread, for the rest of the process — including the one
thread that later installs a real scoped subscriber via
`tracing::subscriber::with_default`. The scoped subscriber is genuinely
active; the event is just never constructed or dispatched at all.

With `--test-threads=1`, the test happened to be positioned early enough in
source-declaration order (inserted immediately before the first
`open_engine_readonly`-calling test in the file) that it won the "first
touch" race every time — that was luck of file position, not a property of
the fix.

## Resolution

Neither of the two "obvious" individual fixes was sufficient on its own:

1. **`tracing::callsite::rebuild_interest_cache()`** called inside the
   `with_default` closure (forces a fresh interest recomputation while the
   scoped subscriber is active) — reduced but did not eliminate the failure
   rate (still failed most of the time under parallel execution).
2. **`tracing_subscriber::EnvFilter`** instead of a plain `.with_max_level()`
   scalar — reduced but did not eliminate the failure rate on its own, and
   combining it with `rebuild_interest_cache()` was not reliably better
   (still ~1-in-5 failures in one measured run). **Correction**: the
   original draft of this learning claimed `EnvFilter` "cannot resolve
   interest statically" and therefore "forces `Interest::sometimes()`" —
   that is not a reliable guarantee. A simple crate-scoped directive like
   `graphtor_core=info` can still be resolved from callsite metadata alone
   and cached as `always`/`never` like any other filter; only genuinely
   dynamic, per-event-dependent directives force `sometimes()`. Treat the
   `EnvFilter` swap as an empirically-observed partial improvement, not a
   mechanism guaranteed to defeat this caching behavior.

The combination that achieved **zero failures across 15+ consecutive stress
runs** under default parallel `cargo test`:

* `tracing_subscriber::EnvFilter::new("graphtor_core=info")` (scoped to the
  crate's own target, both to help force per-event evaluation and to avoid
  capturing an unrelated third-party crate's `INFO` event as a false
  "capture worked" signal)
* `tracing::callsite::rebuild_interest_cache()` inside the `with_default`
  closure
* **A bounded retry loop** (`capture_info_logs_retrying`, 25 attempts, a
  fresh temp DB per attempt so `open_engine_readonly` can be called
  repeatedly without lock conflicts) that treats "captured logs is empty" as
  "this attempt lost the race, try again" and only asserts on content once
  ANY attempt produces non-empty output. The retry loop deliberately does
  **not** retry on "wrong content" (only on "no content at all"), so a
  genuine wording regression still fails the test immediately with the
  actual (non-empty, incorrect) captured text shown for debugging — it does
  not mask real assertion failures, only papers over the scheduling race.

```rust
// Retry until ANY event was observed (not until the CONTENT matches) —
// this distinction is what keeps a genuine wording regression failing fast.
fn capture_info_logs_retrying<F, T>(mut make_operation: impl FnMut() -> F) -> (T, String)
where
    F: FnOnce() -> T,
{
    const MAX_ATTEMPTS: u32 = 25;
    let mut last = None;
    for _ in 0..MAX_ATTEMPTS {
        let (result, logs) = capture_info_logs_once(make_operation());
        if !logs.is_empty() {
            return (result, logs);
        }
        last = Some((result, logs));
    }
    last.expect("MAX_ATTEMPTS is greater than zero")
}
```

## Prevention

* **Diagnose empty-vs-wrong-content separately.** If a `tracing` capture
  test fails with an *empty* buffer (not merely unexpected text), suspect the
  callsite-interest-cache race before suspecting the subscriber setup itself
  — especially if the exact same test passes when run alone or with
  `--test-threads=1`, but fails under default parallel execution.
* **Prefer `EnvFilter` over `.with_max_level()`** for any new `tracing`
  capture test whose call-site is also exercised by sibling tests with no
  subscriber, and call `tracing::callsite::rebuild_interest_cache()` inside
  the `with_default` scope regardless — both reduce the failure rate, but
  neither is sufficient alone if enough "no subscriber" sibling tests are
  racing for the same call-site.
* **Add a bounded retry as the final, pragmatic safety net** — a
  mitigation, not a guarantee — rather than a first resort. A retry loop
  that only retries on "no signal at all" (never on "wrong signal")
  preserves genuine regression detection while absorbing the inherent
  scheduling race; it does not make the race impossible, only
  astronomically unlikely to survive every attempt, and scheduling outcomes
  across attempts are not proven independent. 25 attempts was empirically
  generous (observed per-attempt failure rates during tuning ranged from
  ~20% to ~80% depending on which partial fix was in place; even at a
  pessimistic 80% per-attempt failure and treating attempts as independent,
  `0.8^25 ≈ 4×10⁻³`, and the actual combined fix measured 0% failures across
  15+ full runs) — generous, not airtight.
* **A quick standalone reproduction** (a throwaway `examples/*.rs` binary
  calling only the capture helper + a bare `tracing::info!`, deleted after
  use) is a fast way to confirm the *mechanism itself* works before
  debugging why it fails inside the full test suite — it isolates "is my
  subscriber wired correctly" from "is something else in this process
  interfering."
