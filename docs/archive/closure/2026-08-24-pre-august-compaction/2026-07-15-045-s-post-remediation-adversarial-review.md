# Post-Remediation Adversarial Re-Review — Shipment 045-S (Cycle 1 of 2)

**Commit under review**: `c449e15` — "fix(mcp): adversarial-review remediations (F1/F2)"
**Branch**: `feat/045-s-consumption-first-graphtor`
**Parent commit**: `f8b47e2`
**Scope**: `src/workspace/mcp_config.rs`, `src/workspace/serve_discovery.rs` — the ONLY two files changed by this commit. The broader 25-task shipment diff is explicitly out of scope for this cycle; it already holds a READY TO PROCEED verdict from the prior full adversarial review.
**Review date**: 2026-07-15
**Cycle**: Post-remediation re-review (Phase 7), cycle 1 of a maximum 2

## Method

Per the adversarial-review protocol: 3 independent parallel reviewer agents, one per model tier, each given the identical diff, surrounding-code context, review questions, and ruleset, with no visibility into each other's output prior to this consensus-assembly step.

| Reviewer | Tier | Model | Alt provider? |
|---|---|---|---|
| Reviewer-A | Tier 1 (fast/cheap) | `claude-haiku-4.5` | No |
| Reviewer-B | Tier 2 (standard) | `claude-sonnet-4.6` | No |
| Reviewer-C | Tier 3 (frontier) | `claude-opus-4.6` | No |

`alt_review_provider`/`alt_review_family` were not set for this invocation, and `.github/instructions/adversarial-review.instructions.md` confirms no alternate provider is configured for this workspace ("Not currently configured for this workspace — all reviewer slots use the standard tier routing set"). Standard tier routing applied to all 3 slots.

**Ruleset**: `.github/copilot-review-instructions.md` does not exist in this repo. Per protocol default, the built-in ruleset was used, synthesized from `.github/instructions/rust.instructions.md` (Rust idiom / `clippy::pedantic` / error-handling conventions) plus this module family's established atomic-write and path-containment conventions.

All 3 reviewers completed and returned valid, schema-conformant JSON (no prose, correct field shapes). No reviewer failures; full consensus basis available (protocol minimum is 2).

## Orchestrator-level direct verification (performed before dispatch, and used to construct the reviewer prompts)

* **Diff fidelity**: `git show c449e15 --stat` / `git show c449e15` confirms exactly 2 files changed, 106 insertions / 31 deletions, matching the commit message's own description.
* **F1 call-site exclusivity**: `git grep -n "PruneOutcome"` and `git grep -n "prune_managed_server"` across `src/` return matches ONLY inside `src/workspace/mcp_config.rs` — the enum definition, its 3 match arms, and the single call site inside `remove_mcp_config`. No other file in the codebase references either symbol, so there is no missed call site to update.
* **F2 `existing_candidates` non-regression**: the `existing_candidates` loop (`validate_path(candidate, candidate_root)?`, current line 98) sits textually outside the diff's hunks; read directly from the current file it is unchanged from before the fix.
* **F2 production relevance (not merely a test artifact)**: both real (non-test) call sites in `src/main.rs` (`serve`, ~line 2494; `status`, ~line 2720) pass genuinely distinct arguments — `&graphtor_dir` (`cwd.join(".graphtor")`) as `scan_root`, and `cwd` (the full project root) as `candidate_root`. This confirms F2 was a real, production-reachable divergence for any operator whose `sources.yaml` declares a `type: database` entry outside `.graphtor/` but inside the project root — not a theoretical or test-only gap.
* **Test count reconciliation**: manually counted `#[test]` functions in both post-fix files. `mcp_config.rs` = 21 (20 pre-existing + `remove_prune_rewrite_is_atomic_leaves_no_stray_temp_file`). `serve_discovery.rs` = 27 (26 pre-existing + `explicit_database_entry_outside_graphtor_but_inside_project_root_is_rejected`). Both match the commit's claimed counts exactly.
* **F2 test discriminating power** (manually traced against the OLD code path): under the pre-fix `validate_path(path, candidate_root)`, the new test's `outside_graphtor_db` (placed directly in `project_root`, with `candidate_root == project_root`) would have validated successfully and been served — `served.is_empty()` would be FALSE, failing the test. Under the new code it correctly returns empty. This is a genuine, non-superficial regression test.
* **F1 test discriminating power** (manually traced the same way): under the OLD `fs::write(&dest, new_content)` code, no temp file is ever created in the first place, so `stray.is_empty()` is trivially true under the OLD code too. This test does **not** discriminate old vs. new behavior in the non-crashing path — see PR1 below. All 3 independently-dispatched reviewers reached the same conclusion.

None of the above are new problems with F1/F2's actual code fixes — both are confirmed correct and complete. The one caveat (PR1) concerns test-assertion rigor, not the underlying fix.

## F1 verification — RESOLVED

`PruneOutcome::Rewrite` changed from `String` to `serde_json::Value`; `prune_managed_server`'s tail now returns `PruneOutcome::Rewrite(value)` directly (no pre-serialization); `remove_mcp_config`'s `Rewrite` arm now calls `write_json(&dest, &new_value, rel_path)?` — the identical shared helper `generate_mcp_config` already used. This is exactly the described fix, confirmed by direct diff inspection. **All 3 reviewers and the orchestrator agree: correctly and completely resolved.**

## F2 verification — RESOLVED

The explicit `type: database` entry validation in `discover_served_databases` now calls `validate_path(path, scan_root)` (was `candidate_root`); the `existing_candidates` loop is untouched (still `candidate_root`); the doc comment and `# Errors` section were both updated to match the new behavior. This is exactly the described fix, confirmed by direct diff inspection AND by manual trace showing the new test would have failed on the pre-fix code. **All 3 reviewers and the orchestrator agree: correctly and completely resolved**, with no broken legitimate use case — `existing_candidates`/`--db-path` behavior is provably unaffected (the pre-existing `explicit_db_path_outside_graphtor_root_but_within_project_root_is_honoured` test exercises that exact path and is untouched by the diff).

## Consensus findings (HIGH confidence — flagged by all 3 reviewers)

### PR1 — Rewrite-atomicity test does not actually discriminate old vs. new behavior

* **Confidence**: HIGH (3/3 reviewers, plus the orchestrator's independent trace)
* **Severity**: MINOR (all three reviewers independently rated MINOR — no severity conflict to resolve)
* **File**: `src/workspace/mcp_config.rs`
* **Line**: ~1003–1046 (`remove_prune_rewrite_is_atomic_leaves_no_stray_temp_file`)
* **Issue**: The test's core assertion (`stray.is_empty()` — no leftover `.tmp-*` file) holds true under BOTH the old direct-`fs::write` implementation (which never created a temp file at all) and the new atomic temp-file+rename implementation (whose temp file is cleaned up by a successful rename). The test therefore cannot detect a regression back to the old, non-atomic behavior — it guards a trivially-true postcondition in the happy path, not the atomicity invariant its name and comment claim to verify.
* **Priority score**: HIGH(3) × MINOR(2) = **6**
* **Action class**: `advisory` (MINOR severity is always advisory per the routing table, independent of confidence tier)
* **Suggested fix** (reviewers independently proposed different angles — genuine independent reasoning, not a repeated hint): (a) inject a failure between the temp-write and rename steps to prove the rename step is load-bearing, or (b) — the pragmatic option — rename the test and soften its claim to what it actually verifies (e.g. `remove_prune_rewrite_preserves_unrelated_servers_and_leaves_no_debris`), since black-box crash-injection testing of filesystem atomicity is disproportionate effort for this module. This exact weak idiom already exists, unflagged, in the pre-existing sibling test `generate_write_leaves_no_stray_temp_file` — a pre-existing pattern, not a new weakness invented by this fix. Reasonable to defer both together rather than treat this fix as uniquely deficient.
* **Blocking?** No. Does not indicate the F1 code fix is wrong — `write_json` is demonstrably being called (confirmed by direct code reading) — only that this specific regression test would not catch a future revert to the old behavior.

## Majority findings (MEDIUM confidence)

None. No finding was flagged by exactly 2 of the 3 reviewers.

## Unique findings (LOW confidence — flagged by exactly one reviewer)

### PR2 — Removal of the serialization-failure fallback changes `remove_mcp_config`'s error behavior in a theoretically-reachable-but-practically-unreachable edge case

* **Confidence**: LOW (1/3 reviewers — Reviewer-A only; independently also noted by the orchestrator during preparation, but per protocol the confidence tier reflects the dispatched reviewer pool, not the preparer)
* **Severity**: MINOR
* **File**: `src/workspace/mcp_config.rs`
* **Line**: ~410 (`prune_managed_server`) / ~339–340 (`remove_mcp_config`'s call site)
* **Issue**: The old code's `Err(_) => PruneOutcome::Unchanged` branch silently treated a `serde_json::to_string_pretty` failure as a no-op — `remove_mcp_config` would continue to the next legacy config path and return `Ok(outcomes)` with that one file simply left un-pruned. The new code removes that branch; a serialization failure inside `write_json` now propagates as `Err(GraphtorError::Config)` via `?`, aborting the entire `remove_mcp_config` call (including any not-yet-processed legacy config paths) instead of silently skipping just the one file.
* **Why this is LOW severity in practice, not a real regression**: `prune_managed_server`'s `value` was itself successfully parsed moments earlier via `serde_json::from_str`, and a `serde_json::Value` built from already-valid parsed JSON cannot represent a non-finite float or non-string map key — the classic `to_string_pretty` failure modes. In practice this path is unreachable; the old fallback was defensive/belt-and-suspenders, not protecting against a realistic failure. Failing loudly here is arguably more correct than silently leaving a managed entry un-pruned with no signal to the operator, and it is now consistent with `generate_mcp_config`'s existing `write_json(...)?` error-propagation convention.
* **Priority score**: LOW(1) × MINOR(2) = **2**
* **Action class**: `advisory`
* **Suggested fix**: None required. Optionally mention this intentional error-propagation-hardening side effect in a code comment for future readers, since it was not explicitly called out in the original F1 description.
* **Blocking?** No.

## Remediation plan (ordered by priority = confidence × severity)

| Rank | Finding | Confidence | Severity | Score | Action class | Gating? |
|---|---|---|---|---|---|---|
| 1 | PR1 — rewrite-atomicity test non-discriminating | HIGH | MINOR | 6 | advisory | No |
| 2 | PR2 — serialization-failure fallback removed | LOW | MINOR | 2 | advisory | No |

No entries in this cycle rose to `safe_auto`, `gated_auto`, or `manual` — both findings are advisory-only (MINOR severity is always advisory per the routing table, independent of confidence tier). No fixes were auto-applied in this cycle because none were required.

## Bug/issue queue entries (P0/P1)

**None.** No finding in this cycle reached P0 or P1 severity. No `backlogit add` work items were created. PR1 and PR2 are recorded here as advisory follow-ups only; opening a low-priority backlog item for PR1 (test-rigor polish, shared with the pre-existing sibling test `generate_write_leaves_no_stray_temp_file`) is at the operator's discretion, not required before merge.

## Post-remediation cycle tracking

```yaml
post_remediation:
  cycles_run: 1
  cap_reached: false
  residual_findings: 2
  residual_severity_ceiling: MINOR
  status: "clean"
```

Both residual findings are advisory-only (MINOR severity, no gating action class), so no further `safe_auto` fixes were triggered and cycle 2 was not required. This invocation used 1 of the maximum 2 allowed post-remediation cycles; the cap was not reached because nothing remained that required forced remediation, not because the loop was cut short.

## Final verdict: READY_WITH_FOLLOWUPS

Both F1 and F2 are **fully and correctly resolved** with no partial-remediation gaps:

* **F1**: the prune-rewrite path now goes through the same atomic `write_json` helper as `generate_mcp_config`, exactly as claimed. Confirmed by direct diff/code inspection; no other call site of `PruneOutcome`/`prune_managed_server` exists anywhere in the codebase.
* **F2**: the explicit `type: database` entry validation now uses `scan_root`, exactly as claimed, while `existing_candidates`/`--db-path` validation against the broader `candidate_root` is provably untouched. Confirmed by direct diff/code inspection, by production call-site review (`main.rs`, where `scan_root` ≠ `candidate_root` in real usage — a genuine production fix, not test-only pedantry), and by manual trace proving the new regression test would have failed on the pre-fix code.

No CRITICAL or MAJOR finding was raised by any reviewer, and no P0/P1 issue was introduced. The only two findings — both MINOR/advisory, one HIGH-confidence (unanimous across all 3 reviewers) and one LOW-confidence (single-source) — concern test-assertion rigor and a benign, practically-unreachable error-propagation behavior change, neither of which affects the correctness of the shipped fix. These are listed as **follow-up polish items**, not blockers:

1. **PR1** (recommended, non-blocking): tighten or rename `remove_prune_rewrite_is_atomic_leaves_no_stray_temp_file` so its assertion actually discriminates atomic vs. non-atomic behavior, or relabel it as a happy-path regression test rather than an atomicity proof. Consider doing this together with the pre-existing sibling test `generate_write_leaves_no_stray_temp_file`, which shares the identical weakness.
2. **PR2** (optional, non-blocking): no code change needed; optionally document the intentional error-propagation hardening resulting from removing the dead `Err(_) => PruneOutcome::Unchanged` fallback.

Recommend proceeding — these follow-ups can be addressed opportunistically and do not need to gate this shipment.
