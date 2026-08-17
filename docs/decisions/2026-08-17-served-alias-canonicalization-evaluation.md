---
title: "Served-Alias Canonicalization Evaluation (PR90 Deferral 5868A7C5)"
description: "Investigation into whether served database aliases need explicit canonicalization or reporting beyond the current canonical-path dedup union"
topic: "Served-database alias handling in discover_served_databases"
depth: "lightweight"
decision_status: "decided"
promoted_to: "none"
stash_ids:
  - "5868A7C5"
linked_artifacts:
  - "docs/exec-plans/2026-08-16-serve-auto-discovery-followups-plan.md"
  - "docs/decisions/2026-08-16-serve-auto-discovery-followups-deliberation.md"
tags:
  - serve-discovery
  - investigation
  - follow-up
---

## Problem Frame

PR90 deferral `5868A7C5` asked whether served database aliases —
`type: database` entries in `sources.yaml`, each carrying a configured
`id`/alias plus a `path` — need explicit canonicalization or reporting
beyond the canonical-path dedup union that `discover_served_databases`
(`src/workspace/serve_discovery.rs:91`) already performs. Task 055.002-T
required an investigate-first evaluation and accepts a documented no-op as
a terminal outcome.

## Investigation

### How aliasing works today

* `discover_served_databases` builds `served: Vec<PathBuf>` by canonicalizing
  every candidate (`existing_candidates`, explicit `type: database` entries,
  and the auto-discovery root scan) through `validate_path`, then
  deduplicating with a `BTreeSet<PathBuf>` keyed on the canonical path
  (`serve_discovery.rs:118-158`).
* `DatabaseSource` (`src/config/source.rs:205-215`) carries exactly two
  fields: `id` (the alias/name) and `path`. No other behavior, override, or
  configuration is attached to an alias.
* Because identity is decided exclusively by the canonicalized filesystem
  path, and an alias carries no configuration beyond that path, two
  different aliases that resolve to the same file collapse to one served
  entry with **no loss of information** — there is no second field that
  could be silently dropped.

### Existing test coverage confirms correctness

The current dedup behavior is already comprehensively characterized:

* `served_set_is_canonical_deduped_union_of_candidates_and_root_scan` and
  `same_underlying_file_referenced_twice_collapses_to_one_entry` — the same
  underlying file supplied through two different discovery paths collapses
  to one served entry.
* `explicit_database_entry_matching_an_auto_discovered_file_collapses_to_one_entry`
  (the "shared-alias" case) — an explicit `type: database` alias pointing at
  a file that is *also* auto-discoverable collapses to one entry, not two.
* `explicit_database_entry_outside_graphtor_but_inside_project_root_is_rejected`
  (the "outside-alias" case) and
  `explicit_database_entry_escaping_root_via_dotdot_is_rejected_not_served` —
  an alias whose path escapes the authorized root is rejected with a
  propagated validation error, never silently dropped or silently served.
* `explicit_database_entry_via_windows_junction_is_rejected_not_served` —
  containment holds even through a junction/reparse point.

All of these tests pass unchanged against the current implementation,
confirming the dedup union already behaves correctly across the union,
sharing, and containment scenarios an alias could plausibly introduce.

### Is there a concrete, tested gap?

The only theoretical enhancement identified by the covering deliberation was
diagnostic: surfacing a `type: database` entry's configured alias/`id` in
`status` output, which today reports only the resolved database file path
and its ingested sources (`status_database_json`,
`print_status_database` in `src/main.rs`). No functional defect, incorrect
dedup result, or user-reported confusion motivates this — it would be a
nice-to-have verbosity improvement, not a fix for a concrete, tested gap.
Constitution Principle VI (single responsibility) and the task's own
acceptance criteria direct against adding code speculatively: a diagnostic
addition is warranted only when a concrete, tested gap is found, and none
was found here.

## Decision

**(a) Document that the current canonical-path dedup is sufficient. No code
change.**

The dedup union in `discover_served_databases` already handles every
alias-related scenario correctly and is proven by existing, passing tests:
union assembly, canonical-path collapse for duplicate/shared aliases, and
fail-closed rejection of out-of-root aliases (including symlink/junction
escapes). Because a `type: database` alias carries no configuration beyond
its resolved path, there is no information-loss risk from collapsing two
aliases that resolve to the same file. No diagnostic gap was identified that
would justify adding a new `status`-output surface for the configured
alias/`id` at this time.

## Rejected Alternative

* **(b) Add a bounded diagnostic surfacing the configured alias/`id` in
  `status` output.** Rejected for now: no concrete, tested gap motivates it,
  and adding it speculatively would violate Principle VI (single
  responsibility — new code must be justified by a concrete requirement).
  If a future concrete need for surfacing per-alias diagnostics emerges
  (for example, an operator report of confusion when multiple aliases
  target the same database), revisit this decision with that evidence in
  hand as a new stash entry.

## Task Outcome

055.002-T is complete with outcome (a): a documented no-op. No source files
were changed for this task. `cargo test` and `cargo clippy --all-targets --
-D warnings -D clippy::pedantic` remain green because no code changed.
