---
type: session-memory
agent: stage
date: 2026-08-23
branch: chore/stage-049-S
base_head: 450cab32a3267cd8bcb04aa7636b1e46a4c6fb3c
feature: 056-F
shipment: 049-S
scope: final-narrative-ownership-cleanup
---

# Stage final narrow cleanup — 056-F / 049-S (MCP serve OS error 232)

Planning/backlog/docs only. No Rust/source/build/PR/merge/main-checkout/admin-fallback
work (full local build **not applicable** — planning-only, recorded). Fixed only the
6 precise live narrative/ownership gaps from the current Copilot review; did NOT
reopen the graph architecture. No dependency/frontmatter edits — only prose body
sections and docs prose. Backlog mutations applied by direct source edit + `backlogit
sync` (documented rehydration path) since every change was prose within existing
sections; no `dependencies:`/label/status state changed.

## Six fixes applied

1. **056.019-T H3-B1 selection** — Removed unconditional selection of discriminator
   delivery `056.027`. H3-B1 (isolated-config + working-directory) now activates only
   the managed-cwd/recovery tasks (`056.017/056.024/056.008/056.018/056.009`); the
   discriminator tasks (`056.026` gen / `056.027` delivery) are gated separately on an
   independently evidenced `type`/`transport` mismatch (else not-needed). Aligned the
   plan T-H3-B section and the Likely-Surfaces `056.019` row. 056-F/decision already
   separated the two families (no edit needed).

2. **056.028-T PHASE 1.5** — Assigned to a new **unconditional PHASE 1.5
   evidence-infrastructure release unit** (sole member `056.028`). Owner = Stage;
   ordering = immediately after `049-S` closes and BEFORE any selected remedy shipment;
   readiness gate = standalone-probe CI green; remains in T4 (`056.004`) fan-in; NOT
   cause-selected (`cause:probe-ci` is a categorization label, not a remedy `selection:`
   gate); retains `phase:evidence`. Not in `049-S` because its workflow depends on the
   standalone crate delivered by `049-S`. No manifest created now. Reflected in plan
   phase inventory + summary + residual risks + reviewed-artifact identity, in 056-F DoD,
   and in 056.028-T body/AC.

3. **056.004-T (T4) executable runner** — 056.004 now OWNS a future concrete script
   `scripts/verify_copilot_mcp_show.ps1` (test-first ownership; Ship implements when
   056.004 lands — NOT implemented now). Encoded the 8-point contract (structured argv,
   no exit-0-alone pass, fail on OS error 232 / missing CONNECTED/INITIALIZED, record
   advertised tools, correlate config hash + named non-substituting server-start source
   [inherited stderr / 056.006 sink / OS tracing — gate-off run may not use sink], consume
   056.002 read-only production-control for JSON-RPC id/protocolVersion/tools/get_status,
   redacted structured result, preserve native exit checking). Plan Verification step 5
   now calls the script instead of the raw exit-only loop.

4. **056.023-T audit gate** — Added `cargo audit --file tools/mcp-probe/Cargo.lock`
   (immediate-failure) to completion gates, since 056.023 adds `serde_json` and finalizes
   the probe lockfile. 056.028 retains continuous-CI audit ownership.

5. **Narrative/ownership cleanup** (plan + decision): Likely-Surfaces split ownership
   (056.008 cwd-only; new 056.026 discriminator + 056.027 delivery rows; 056.024 parallel
   decision `056.017→056.024→056.018 ∥ 056.017→056.008`, stale `056.017→056.024→056.008`
   removed; 056.009 cwd delivery); H1 "three tasks"→four (056.014/056.005/056.025/056.015);
   decision Open-Questions "056.008 emits discriminator"→056.026; 056.004 AC + plan T4-body
   now name BOTH 056.009 (cwd) and 056.027 (discriminator) delivery evidence; 056.013 README
   reconciliation made conditional (mutate only when discriminator change selected, else
   verify + record not-needed — removed the "always mutate README" contradiction); risky-
   action label collision fixed (056.024 `T2g`→`T2i`, approval class left descriptive/low —
   scope not broadened); removed static "latest reviewed HEAD/outcome" + "PR #106 remains
   BLOCKED" claims from live plan/decision (defer to PR #106 `## Local Review Readiness`;
   historical audit-trail SHAs retained); added explicit no-`056.011`↔T4-cycle clarification;
   reviewed-artifact inventory now covers `056.001..056.028` + PHASE 1.5.

6. **Hook duplicate note** — Added operator/residual-risk note: `.backlogit/hooks_queue.jsonl`
   seq 994/995 are a benign duplicate `create_artifact` for `056.021-T` (artifact singular,
   no intervening delete/recreate, no supported removal/supersede op → stream left intact,
   NO destructive repair attempted). Consumers dedupe by item/event identity, not raw seq.

## Files changed (9)
- `.backlogit/queue/056-F.md`, `056.004-T.md`, `056.013-T.md`, `056.019-T.md`,
  `056.023-T.md`, `056.028-T.md`
- `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`
- `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`
- (+ this memory file)

`049-S.md` intentionally UNCHANGED (membership preserved).

## Validation
- `backlogit sync` OK (492 artifacts); `backlogit doctor` = 140 issues, **all pre-existing
  `archived_from_self_ref` in `.backlogit/archive/` (0 new)**.
- `049-S` = exactly 8 tasks (`056.020/056.022/056.023/056.021/056.001/056.002/056.003/056.019`).
- `056.028` in NO shipment (unshipped).
- All 16 remedy tasks retain `phase:remedy` + `cause:<family>` + `selection:pending`.
- 056 dependency DAG **acyclic** (Kahn: 28 nodes / 61 edges / 28 toposorted); no dependency
  edits made; nothing depends on `056.004-T`.
- Stale-string sweep empty; markers/frontmatter balanced; `git diff --check` clean.

## Review (report-only, 3 lenses)
- Correctness Reviewer, Architecture Strategist, Agent-Native Parity Reviewer: **no P0/P1**.
- Remediated one in-scope P2 (plan T4-body now names both 056.009 + 056.027 delivery evidence,
  completing fix 5f). One Correctness P3 ("056.019 B1 omits 056.008") was a **false positive**
  (bullet correctly lists all 5). Remaining advisories left: hook-dedupe blanket-rule precision;
  /mcp show↔056.002 tool-set parity assertion (pre-existing deferred P2); single-runner lock
  quiescence (pre-existing deferred P2).

## Preserved (not touched)
`.mcp.json` (modified), `.backlogit/checkpoints/*`, `.backlogit/runtime/`, and untracked
scripts (`exec_commands.ps1`, `exec_git_commands.sh`, `git_commands_output.ps1`,
`run_commands.ps1`, `run_git_commands.sh`, `search_patterns.ps1`, `temp_git_commands.sh`).
Append-only hook history not hand-edited.

## Copilot thread mapping (current unresolved → status)
- **Addressed by this pass:** 056.019 (H3-B1 selects 056.027) → fix 1; 056.028 (no release
  unit) → fix 2; 056.004 + plan T4 recipe (exit-only) → fix 3; 056.023 (missing audit) → fix 4;
  Likely-Surfaces stale ownership + reviewed-artifact 25→28 scope + phase inventory → fix 5;
  hooks_queue 994/995 duplicate → fix 6.
- **Still open — out of scope (not among the 6 targeted gaps; "do not reopen graph
  architecture"):** H3-B2-"blocks shipment" wording threads (056-F/056.019/plan/decision) —
  current artifacts already scope H3-B2 as terminal-done closing 049-S + blocking downstream;
  056-F re-probe-vs-016/024 and discriminator-only activation-predicate threads — already
  handled in current DoD; 056.020 kill/wait teardown threads — already reconciled (056.020 owns
  bounded self-test reaping); decision "JSON-RPC per /mcp show" — already corrected.
- **Still open — PR-body/readiness (cannot edit PR body/threads this pass):** stale PR
  description "24 tasks / single chain" (056-F, 049-S, 056.028) and stale readiness-block HEAD
  (10f05bd/6c39604 vs current). Plan-side deferral to PR #106 `## Local Review Readiness` added;
  PR body update is a Ship/PR-lifecycle action.

## Next steps
- Ship (not Stage): refresh PR #106 body/readiness block for the current HEAD; run the
  current-HEAD local review gate; resolve bot threads after the fix commit is pushed.
- On `049-S` close: Stage assembles PHASE 1.5 (`056.028`), then consumes T0/056.019 selection
  before any remedy shipment.
