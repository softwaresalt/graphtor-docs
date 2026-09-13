# Adversarial Multi-Model Review — Shipment 049-S

**Shipment**: Fix MCP serve initialize-handshake regression / Copilot CLI OS error 232
**Branch**: `chore/fix-mcp-serve-initialize-handshake-regression` (vs `main`)
**Backlog tasks**: 056.020-T, 056.022-T, 056.023-T, 056.021-T, 056.001-T, 056.002-T, 056.003-T, 056.019-T
**Review type**: Adversarial multi-model review (post-standard-9-persona-review, pre-PR gate)
**Date**: 2026-09-13
**Reviewers**: 3 (`reviewers: 3`)
**Output mode**: `full`
**Post-remediation review**: SKIPPED (`post_remediation_review: false` — Phase 7 not run; Ship owns applying fixes and re-verification)

---

## Phase 1 — Prepare

### Environment constraint (methodology note)

No `git`/shell/bash tool access was available to any agent in this session
(confirmed across multiple delegation attempts). `git diff main...HEAD` could
not be executed. The review surface was instead assembled by direct,
verbatim reading of current file content via `view`/`view_range`:

- Files that are wholly new in this shipment (all of `tools/mcp-probe/**`,
  `src/workspace/serve_preflight.rs`, `tests/common/serve_driver.rs`, the two
  new integration test files) were read in full — this is equivalent to a
  diff since there is no prior version.
- `src/workspace/mod.rs` and `tests/common/mod.rs` are tiny and were read in
  full.
- `src/main.rs` (6,454 lines total, overwhelmingly pre-existing/unrelated) was
  scanned end-to-end to extract, verbatim, only the functions this shipment
  touches: the `use workspace::serve_preflight::{...}` import, `ServeOpenedDatabases`,
  `open_serve_databases`, `run_duplicate_intake_preflight`, and `cmd_serve` — the
  only places in `main.rs` calling into the new `serve_preflight` module.

A consolidated code package (embedded excerpts + a manifest of files each
reviewer had to open directly) was written to the gitignored scratch path
`logs/adversarial-review-049-S-code-package.md` so all three reviewers could
work from one canonical assembly rather than three duplicated ~45KB prompts.

### Ruleset

No `.github/copilot-review-instructions.md` exists in this repository. Used
the built-in harness ruleset (correctness, security, concurrency,
maintainability, architecture/scope) combined with this repo's `AGENTS.md`
Constitution principles I–VI (Safety-First Rust / `#![forbid(unsafe_code)]` /
no `.unwrap()`/`.expect()` in library code / clippy pedantic as hard errors;
Test-First Development; Workspace Isolation and Security Boundaries; CLI
Workspace Containment; Destructive Command Approval; Safety Modes).

### Reviewer count and model routing

`reviewers: 3`, no `models`/`alt_provider`/`alt_family`/`anchor_provider`/
`anchor_family` overrides supplied. Per the protocol's count=3 "Anchor
dispatchable mapping" (Anchor Reviewer + Reviewer-A (Tier 1) + Reviewer-C
(Tier 3), since the default anchor route `gpt-5.6-sol` is dispatchable in
this environment's model list):

| Reviewer | Route | Model requested | Model actually used | Notes |
|---|---|---|---|---|
| Anchor Reviewer | Anchor review route | `gpt-5.6-sol` via `openai`, `reasoning_effort: high` | `gpt-5.6-sol`, `reasoning_effort: high` | Dispatched as configured, no fallback needed |
| Reviewer-A | Tier 1 (fast/cheap) | `claude-haiku-4.5` | `claude-haiku-4.5` | Dispatched as configured |
| Reviewer-C | Tier 3 (frontier) | `claude-opus-4.6` | `claude-opus-4.8` | **Declared substitution**: `claude-opus-4.6` is not present in this environment's available model list (`claude-opus-4.7`/`4.8`/`5` are); `claude-opus-4.8` was used as the closest available frontier-tier equivalent. Logged here per protocol requirement to record any deviation from the literal default name. |

Reviewer-B (Tier 2, standard) is correctly **not** used for `reviewers: 3`
under the anchor-dispatchable mapping — consistent with the protocol table.
No alternate-provider (`alt_review_provider`/`alt_review_family`) override
was supplied, so no Gemini/other-provider slot was assigned.

All three reviewers received the identical code package, ruleset context,
and the "already-fixed, do not re-flag" / "known residual risk, do not
re-flag" lists from the request, and were instructed to return **only** a
JSON array of findings (`severity`, `rule`, `file`, `line`, `issue`, `fix`).

---

## Phase 2 — Parallel Dispatch (raw results)

All three reviewers completed successfully and returned before Phase 3 began.

- **Anchor Reviewer** (`gpt-5.6-sol`): 9 findings, all severity `MAJOR`.
- **Reviewer-A** (`claude-haiku-4.5`): 0 findings (`[]`) — completed the task
  but found nothing to report.
- **Reviewer-C** (`claude-opus-4.8`): 4 findings, all severity `MINOR`.

---

## Phase 3 — Aggregate and Classify

Findings were fuzzy-matched by `file` + `line ± 2` + same underlying issue.
One pair (Anchor's `exact_cli.rs:324` and Reviewer-C's `exact_cli.rs:300`,
both describing the same `CaptureSink` unbounded-buffer construct) was
matched as the same finding despite a 24-line gap between the two reported
line numbers, which is outside the nominal `± 2` fuzzy-match window — this
drift is attributable to the reviewers scanning the file via different
`view_range` chunk boundaries rather than a true git diff with stable line
anchors (documented environment constraint above). The match was made on
**identical named construct + identical file + identical semantic issue**
(both describe `CaptureSink`'s `Arc<Mutex<Vec<u8>>>` growing without a size
cap), which is a stronger match criterion than raw line proximity and is
treated as intentional fuzzy-matching in spirit of the protocol. No other
cross-reviewer matches were found.

**Important structural note for `reviewers: 3`**: with exactly 3 reviewers,
the "Plurality" tier (more than one reviewer but not a strict majority) is
mathematically impossible — 2 of 3 already satisfies strict majority
(2 > 3/2 = 1.5). Confidence tiers therefore collapse to exactly three
possible outcomes: 3/3 = **Consensus/HIGH**, 2/3 = **Majority/MEDIUM**,
1/3 = **Unique/LOW**. The Plurality section below is empty by construction,
not by omission.

**Independent verification**: because this review had no git diff to anchor
against, and because acting on a wrong line reference wastes remediation
effort, every finding below was independently re-checked against the actual
current file content via `view`/`view_range` before being included in this
report. One reviewer claim was found to be factually incorrect and is
**excluded** from the findings tables (documented in the Verification Notes
appendix). One finding's severity/framing was adjusted based on direct
source inspection (also documented). This verification pass was performed by
the aggregating agent, not as an additional (4th) reviewer vote — it does
not change any finding's confidence tier, only its accuracy/framing.

---

## Consensus findings (HIGH confidence — flagged by all 3 reviewers)

**None.** Reviewer-A (`claude-haiku-4.5`) returned zero findings, so no
finding achieved 3/3 agreement. This is a valid, reportable outcome of the
protocol, not a gap — it means no issue was severe/obvious enough to be
independently caught by the fastest/cheapest model as well as the other two.
**There are no mandatory-before-merge findings under the strict HIGH-confidence
bar.**

---

## Majority findings (MEDIUM confidence — flagged by 2 of 3 reviewers)

### M-1: Unbounded in-memory capture of the exact-CLI child's stdout

- **File**: `tools/mcp-probe/src/exact_cli.rs`
- **Line**: ~292–320 (Anchor reported line 324; Reviewer-C reported line 300;
  independently verified location is the `CaptureSink` struct/`impl Write`
  block, confirmed present as described)
- **Severity**: MAJOR (most conservative of the two ratings — Anchor rated
  MAJOR, Reviewer-C rated MINOR; per Phase 3 rule, the higher severity is
  taken on conflict)
- **Confidence**: MEDIUM (2 of 3 reviewers — Anchor + Reviewer-C)
- **Issue**: `CaptureSink` (used as the `outgoing` sink for `run_duplex_pump`
  when running the exact Copilot CLI child in `run_leg`) appends every byte
  of the child's stdout into an `Arc<Mutex<Vec<u8>>>` with no size cap:
  ```rust
  impl Write for CaptureSink {
      fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
          self.0
              .lock()
              .unwrap_or_else(std::sync::PoisonError::into_inner)
              .extend_from_slice(buf);
          Ok(buf.len())
      }
      ...
  }
  ```
  Unlike `tests/common/serve_driver.rs`'s `BoundedCapture` (which caps stderr
  at a fixed byte limit and sets a truncation flag), there is no equivalent
  bound here. A verbose, misbehaving, or adversarial Copilot CLI child can
  grow the wrapper's memory without limit for the entire leg deadline window
  (default 45s) before the wall-clock deadline is ever reached.
- **Fix**: Cap `CaptureSink`'s buffer (mirroring `BoundedCapture` in
  `tests/common/serve_driver.rs`) so a chatty/adversarial child cannot drive
  unbounded memory growth; record a truncation flag on the leg outcome
  (`LegOutcome`) so downstream classification can treat truncated capture as
  non-authoritative rather than silently trusting a partial capture.
- **Action class**: `gated_auto` — a deterministic, well-scoped fix (bound
  the buffer, add a truncation flag) exists, but confirm before applying
  since it changes a public struct's behavior under load and should be
  paired with a test that exercises the truncation path.

**Acknowledgment required**: this is the only MEDIUM-confidence finding.
Per your stated obligations, you must explicitly fix or defer (with
rationale) this finding before PR.

---

## Plurality findings (MEDIUM confidence — more than one but not a strict majority)

**Structurally empty for `reviewers: 3`** (see Phase 3 note above — 2 of 3
reviewers already constitutes strict majority, so no finding can land in
this tier at this reviewer count).

---

## Unique findings (LOW confidence — flagged by exactly 1 of 3 reviewers)

All items below were independently re-verified by the aggregating agent
against current source before inclusion. Severity shown is the reviewer's
original rating; where independent verification led to a different
assessment, this is noted explicitly.

### U-1: Shared evidence file not reset before each leg — stale-evidence misattribution risk
- **File**: `tools/mcp-probe/src/exact_cli.rs`
- **Line**: ~721 (`run_leg`, evidence-snapshot section)
- **Severity**: MAJOR — **confirmed** by direct source inspection
- **Source**: Anchor only
- **Issue**: `run_leg` removes only its own leg-scoped snapshot destination
  (`leg_evidence_copy`) before spawning the exact-CLI child:
  ```rust
  let leg_evidence_copy = leg_dir.join("wrapper-evidence-snapshot.json");
  let _ = fs::remove_file(&leg_evidence_copy);
  ```
  It never removes or invalidates the **shared** `evidence_output` path
  itself (by design, both control and treatment legs point at the same
  `evidence.json` — see `workspace.rs` module docs). If this leg's wrapper
  is never spawned, crashes before writing, or otherwise fails to refresh
  `evidence_output`, the subsequent
  `fs::copy(evidence_output, &leg_evidence_copy)` silently copies the
  **previous leg's** stale evidence and attributes it to the current leg —
  there is no check that the copied evidence actually corresponds to this
  leg's own run (e.g. via the `run_nonce` field already present in
  `EvidenceSummary`).
- **Fix**: Before spawning each leg's child, remove or invalidate the shared
  `evidence_output` (`let _ = fs::remove_file(evidence_output);`), and after
  the snapshot copy, verify `wrapper_evidence["run_nonce"]` matches the
  nonce expected for this specific run before treating the copy as this
  leg's evidence; treat a missing/mismatched nonce as "no evidence for this
  leg" rather than silently reusing a prior leg's data.
- **Action class**: `manual` (single-source finding on MAJOR-severity logic
  in the tool's core differential-classification correctness; should not be
  auto-applied without a maintainer confirming the nonce field is reliably
  available for this check).

### U-2: `leg_has_valid_initialize` ignores the evidence-wide `valid` flag
- **File**: `tools/mcp-probe/src/exact_cli.rs`
- **Line**: ~852 (`fn leg_has_valid_initialize`)
- **Severity**: MAJOR — **confirmed** by direct source inspection
- **Source**: Anchor only
- **Issue**:
  ```rust
  fn leg_has_valid_initialize(leg: &LegOutcome) -> bool {
      leg.wrapper_evidence
          .as_ref()
          .and_then(|value| value.get("initialize_correlation"))
          .is_some_and(|value| !value.is_null())
  }
  ```
  This checks only that `initialize_correlation` is present and non-null; it
  never checks the evidence summary's own top-level `valid` boolean (emitted
  by `evidence_summary_to_json` alongside `invalid_reason`). `EvidenceSummary.valid`
  exists specifically to signal that the collector itself detected a problem
  (e.g. internal channel saturation per `EvidenceCollector::new_with_capacity`'s
  own doc comment). Evidence that was explicitly marked `valid: false` (with
  a real `invalid_reason`) but which happened to populate
  `initialize_correlation` before the invalidating event occurred will still
  be treated by this function as "this leg had a valid initialize" — this
  directly affects the H0a/pass classification that is this entire probe
  tool's reason for existing.
- **Fix**: Require `wrapper_evidence["valid"] == true` in addition to a
  well-formed, non-null `initialize_correlation` before returning `true`.
- **Action class**: `manual`.

### U-3: `identify_copilot` invokes the target executable with no deadline/guard
- **File**: `tools/mcp-probe/src/exact_cli.rs`
- **Line**: ~258 (`fn identify_copilot`)
- **Severity**: MAJOR — **confirmed** by direct source inspection
- **Source**: Anchor only
- **Issue**: `identify_copilot` calls
  `Command::new(exe_path).arg("--version").output()` directly — a blocking
  call with no wall-clock deadline and no `ChildGuard`/pump-and-reap
  composition, unlike every other child-process invocation in this crate
  (`run_leg`'s exact-CLI spawn, the transport pump). A wedged or
  intentionally hanging `--version` invocation blocks the entire probe run
  indefinitely before Gate 1 / leg execution ever begins, with no way to
  recover.
- **Fix**: Route identity collection through the existing
  deadline-governed `ChildGuard`/pump-and-reap composition (the same
  primitive `run_leg` already uses), with bounded stdout/stderr capture and
  a hard deadline.
- **Action class**: `manual`.

### U-4: Read-only server-control boundary check can't observe a write marker beyond the truncated stderr window
- **File**: `tests/common/serve_driver.rs`
- **Line**: ~672–708 (`fn run_read_only_server_control`)
- **Severity**: MAJOR — **confirmed** by direct source inspection, with a
  scope caveat (see Fix)
- **Source**: Anchor only
- **Issue**: `run_read_only_server_control`'s docstring makes a strong claim:
  *"observing one \[write-path marker\] fails the control closed even if
  every JSON-RPC step otherwise succeeded."* But the function's actual
  decision only inspects `shutdown.stderr` for the literal write-path
  marker — it does not consult `shutdown.stderr_truncated` when deciding
  whether to return `Control(success)`:
  ```rust
  let outcome = if stderr_shows_write_path(&shutdown.stderr) {
      ServerControlOutcome::BoundaryViolated { evidence: shutdown.stderr.clone() }
  } else {
      match result { Ok(success) => ServerControlOutcome::Control(success), ... }
  };
  ```
  `stderr_truncated` **is** propagated on the returned `ServerControlReport`,
  so a downstream consumer that explicitly checks it can still catch this —
  but the function's own boundary-violation claim is not fully accurate as
  currently worded/implemented: a write-path marker emitted after the
  bounded capture window fills is silently unobservable, and `Control` can
  still be returned.
- **Fix**: Either (a) treat `stderr_truncated == true` as
  `ServerControlOutcome::Diagnostic` (indeterminate) rather than allowing
  `Control` to be returned in that case, or (b) soften the docstring's claim
  to explicitly state the guarantee is conditioned on non-truncated stderr
  capture, and require callers to check `stderr_truncated` themselves.
- **Scope caveat**: this function is documented as "intended for a later,
  out-of-scope consumer (T4/056.011-T — not a member of shipment 049-S)".
  It is still part of the file in this shipment's review scope
  (`tests/common/serve_driver.rs`, 056.002-T), so the finding is included,
  but it is not currently invoked by anything wired into this shipment's own
  test suite — lowering practical urgency somewhat.
- **Action class**: `manual`.

### U-5: `LineReassembler`/`CollectorState` have no size or event-count bound
- **File**: `tools/mcp-probe/src/evidence.rs`
- **Line**: ~248–256 (`struct LineReassembler`), ~280–300 (`struct CollectorState`)
- **Severity**: MAJOR as reported — **partially verified** (structure
  confirmed to have no explicit cap; full exploitability not exhaustively
  traced within this review's time budget)
- **Source**: Anchor only
- **Issue**: `LineReassembler.pending: Vec<u8>` grows via `extend_from_slice`
  on every `push` call with no cap, and `CollectorState.events: Vec<FrameEvent>`
  grows by one entry per observed frame with no cap either. An unterminated
  frame (no `\n` ever arriving) or a sustained stream of small/valid frames
  over a long-lived session could grow process memory without bound, even
  though the upstream delivery channel (`EvidenceCollector`'s own bounded
  channel) is itself bounded and can mark the summary `invalid` on
  saturation.
- **Fix**: Cap `pending` bytes and `events` count; on overflow, call
  `state.mark_invalid(...)` (already the module's own established pattern
  for other failure conditions) rather than continuing to grow unbounded.
- **Action class**: `manual`.

### U-6: Transport-level delivery drops are invisible to `EvidenceCollector`
- **File**: `tools/mcp-probe/src/transport.rs` (drop site) /
  `tools/mcp-probe/src/evidence.rs` (blind consumer)
- **Line**: transport.rs ~147–154 (`deliver_copy`)
- **Severity**: MAJOR as reported — **architecturally plausible, not
  exhaustively proven end-to-end** within this review's time budget
- **Source**: Anchor only
- **Issue**: `transport::deliver_copy` uses `try_send` into its own
  delivery-worker channel (capacity 64) and **silently discards** the copy
  on `Full`/`Disconnected`:
  ```rust
  fn deliver_copy(tx: &SyncSender<(Direction, Vec<u8>)>, direction: Direction, bytes: &[u8]) {
      match tx.try_send((direction, bytes.to_vec())) {
          Ok(()) | Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {}
      }
  }
  ```
  `EvidenceCollector`'s own hook is invoked only for copies that make it
  through this outer delivery channel; when `EvidenceCollector`'s *own*
  internal channel saturates, it does mark the summary `invalid` (per its
  documented behavior) — but a drop at this *outer*, transport-level
  delivery channel happens **before** the hook (and thus before
  `EvidenceCollector`) ever sees the copy at all, so `EvidenceCollector` has
  no signal that anything was lost upstream of it, and can legitimately
  finalize with `valid: true` despite missing data.
- **Fix**: Expose delivery loss through a counter or drop callback on the
  transport side, and have `EvidenceCollector`'s hook (or `finalize`) force
  `valid: false` whenever any upstream transport-level copy was dropped.
- **Action class**: `manual`.

### U-7: `create_probe_workspace` builds returned paths from the raw (non-canonicalized) `repo_root`
- **File**: `tools/mcp-probe/src/workspace.rs`
- **Line**: ~513–525 (`fn create_probe_workspace`)
- **Severity**: MINOR — **downgraded from the reviewer's original MAJOR/
  security framing after independent verification**
- **Source**: Anchor only
- **Issue as originally reported**: Anchor characterized this as a
  containment-escape risk ("arbitrary repo roots allow writes outside the
  process working-directory tree"). **Independent verification found this
  characterization to be overstated**: `create_shared_dir_component_validated`
  and `validate_containment` both re-`canonicalize` each path component and
  check `starts_with(canonical_repo_root)` **before** it is trusted — so a
  genuine escape (e.g. via a symlinked/reparse-point ancestor) is still
  caught regardless of whether the intermediate `.join()` calls used the raw
  or canonical `repo_root`. The actual, narrower issue: `create_probe_workspace`
  calls `fs::canonicalize(repo_root)` into `canonical_repo_root` for
  containment-checking purposes, but builds `logs_dir`/`probe_root`/
  `workspace_root` (and therefore every path in the returned `ProbeWorkspace`
  struct) by joining onto the **original, non-canonicalized** `repo_root`
  parameter. If a caller passes a relative or symlink-containing `repo_root`,
  the paths returned to the caller (e.g. embedded as `--evidence-output` /
  `cwd` values inside the generated `.mcp.json` fixtures) are not
  necessarily portable if the process's current directory changes before
  those paths are consumed — a correctness/reliability concern, not a
  security containment bypass.
- **Fix**: Build `logs_dir`/`probe_root`/`workspace_root` (and the returned
  `ProbeWorkspace` fields) from `canonical_repo_root` rather than the raw
  `repo_root` parameter, so every returned path is absolute and canonical
  regardless of what form the caller's `repo_root` argument took.
- **Action class**: `advisory`.

### U-8: `DELIVERY_CHANNEL_CAPACITY` doc comment contradicts actual `try_send` drop semantics
- **File**: `tools/mcp-probe/src/transport.rs`
- **Line**: ~54–58
- **Severity**: MINOR — **confirmed** by direct source inspection
- **Source**: Reviewer-C only
- **Issue**: The doc comment above `DELIVERY_CHANNEL_CAPACITY` states a full
  channel "silently drops the oldest-pending delivery attempt". Delivery
  uses `SyncSender::try_send` (in `deliver_copy`), which on a full bounded
  channel returns `Err(TrySendError::Full(_))` for the **new** item being
  sent — it never evicts an already-queued (older) item. This is the
  opposite of the doc comment's claim, and is inconsistent with
  `deliver_copy`'s own (correct) doc comment two functions below, which
  says "silently drops the copy" (i.e., the current one).
- **Fix**: Reword the `DELIVERY_CHANNEL_CAPACITY` doc comment to "silently
  drops the current copy being delivered (never evicting already-queued
  items)" to match actual `try_send` semantics and the `deliver_copy` doc.
- **Action class**: `advisory`.

### U-9: Evidence output file written without owner-only permissions; key-based redaction misses value-embedded secrets
- **File**: `tools/mcp-probe/src/evidence.rs`
- **Line**: ~630–655 (`fn write_evidence_output`), ~140–170 (`fn redact_json_value`)
- **Severity**: MINOR — **confirmed** by direct source inspection
- **Source**: Reviewer-C only
- **Issue**: `redact_json_value` redacts only object members whose **key**
  matches a sensitive substring; a secret embedded inside a value under a
  benign key (e.g. a bearer token embedded in a URL string) in the persisted
  `initialize` params/result copy would not be redacted. Separately,
  `write_evidence_output` creates its temp file via plain
  `std::fs::File::create` with default (not owner-only) permissions —
  unlike `workspace.rs`'s `write_owner_only` (deliberately `0600`-at-creation
  for the `.mcp.json` fixtures, specifically because those files carry
  unredacted real secrets). The combination means any secret that slips past
  the key-based redaction ends up in a more widely readable file than the
  fixtures this crate already treats as sensitive.
- **Fix**: Either write the evidence output owner-only (mirroring
  `workspace::write_owner_only`) as defense-in-depth, or explicitly document
  that value-embedded secrets are out of the redaction model and rely solely
  on the key-based approach being sufficient in practice.
- **Action class**: `advisory`.

### U-10: `redact_argv` only handles the inline `--name=value` form
- **File**: `tools/mcp-probe/src/evidence.rs`
- **Line**: ~108–120 (`fn redact_argv`)
- **Severity**: MINOR — **confirmed**, but **already self-documented as an
  intentional scope limitation** in the function's own doc comment ("this
  task covers the common inline-assignment form only")
- **Source**: Reviewer-C only
- **Issue**: Space-separated `--name value` pairs (where the value is a
  distinct argv entry) are left fully unredacted by `redact_argv`. This is a
  real gap if any evidence path ever forwards space-separated secret flags
  through this helper, but the code's own doc comment already flags this as
  a known, deliberate scope boundary rather than an oversight.
- **Fix**: Either extend redaction to the two-token `--name value` form, or
  leave as-is given the documented scope and confirm (as a one-time check,
  not a code change) that no in-scope production path forwards
  space-separated secret flags through this helper.
- **Action class**: `advisory` (lowest urgency of all findings in this
  report — already self-documented as a known limitation).

---

## Phase 4 — Priority Scoring (all findings, sorted)

`priority = confidence_weight (HIGH=3, MEDIUM=2, LOW=1) × severity_weight (CRITICAL=4, MAJOR=3, MINOR=2)`

| # | Priority | Finding | Confidence | Severity | File |
|---|---|---|---|---|---|
| 1 | 6 | M-1 CaptureSink unbounded stdout capture | MEDIUM | MAJOR | tools/mcp-probe/src/exact_cli.rs |
| 2 | 3 | U-4 read-only boundary check vs. truncated stderr | LOW | MAJOR | tests/common/serve_driver.rs |
| 3 | 3 | U-5 LineReassembler/CollectorState unbounded | LOW | MAJOR | tools/mcp-probe/src/evidence.rs |
| 4 | 3 | U-3 identify_copilot no deadline/guard | LOW | MAJOR | tools/mcp-probe/src/exact_cli.rs |
| 5 | 3 | U-1 stale shared evidence_output reuse | LOW | MAJOR | tools/mcp-probe/src/exact_cli.rs |
| 6 | 3 | U-2 leg_has_valid_initialize ignores `valid` | LOW | MAJOR | tools/mcp-probe/src/exact_cli.rs |
| 7 | 3 | U-6 transport-level delivery drops invisible to collector | LOW | MAJOR | tools/mcp-probe/src/transport.rs |
| 8 | 2 | U-9 evidence file perms + value-embedded redaction gap | LOW | MINOR | tools/mcp-probe/src/evidence.rs |
| 9 | 2 | U-10 redact_argv inline-only form | LOW | MINOR | tools/mcp-probe/src/evidence.rs |
| 10 | 2 | U-8 DELIVERY_CHANNEL_CAPACITY doc contradiction | LOW | MINOR | tools/mcp-probe/src/transport.rs |
| 11 | 2 | U-7 create_probe_workspace raw-repo_root paths | LOW | MINOR | tools/mcp-probe/src/workspace.rs |

(Ties broken by file path, then by ascending line number within the same
file, per protocol.)

---

## Phase 5 — Action Classes

| Finding | Action class |
|---|---|
| M-1 | `gated_auto` — deterministic fix exists, confirm before applying |
| U-1, U-2, U-3, U-4, U-5, U-6 | `manual` — single-source MAJOR-severity logic findings; require human judgment before changing core probe classification/observability logic |
| U-7, U-8, U-9, U-10 | `advisory` — MINOR severity, lowest urgency |

No finding in this review reached `safe_auto` (no CRITICAL+HIGH-confidence
findings occurred at all).

---

## Backlog work items (P0/P1 findings)

No CRITICAL-severity findings occurred, so there are no P0 items. All MAJOR
findings below are logged as P1 items per this project's severity mapping.

```yaml
type: bug
title: "M-1: CaptureSink unbounded stdout capture in exact_cli.rs"
description: "CaptureSink appends the exact Copilot CLI child's entire stdout into an Arc<Mutex<Vec<u8>>> with no size cap, unlike serve_driver.rs's BoundedCapture; a verbose/adversarial child can exhaust wrapper memory before the leg deadline elapses."
file: "tools/mcp-probe/src/exact_cli.rs"
line: 300
severity: "MAJOR"
confidence: "MEDIUM"
fix: "Cap CaptureSink's buffer (mirroring BoundedCapture), and record a truncation flag on LegOutcome."
linked_review: "docs/closure/2026-09-13-fix-mcp-serve-initialize-handshake-regression-adversarial-review.md"
```

```yaml
type: bug
title: "U-1: Shared evidence_output not reset before each leg (stale-evidence misattribution)"
description: "run_leg removes only its own leg-scoped snapshot, never the shared evidence_output; a leg whose wrapper fails to write fresh evidence will silently copy and attribute the previous leg's stale evidence as its own."
file: "tools/mcp-probe/src/exact_cli.rs"
line: 721
severity: "MAJOR"
confidence: "LOW"
fix: "Remove/invalidate evidence_output before each leg starts; verify wrapper_evidence[\"run_nonce\"] matches the expected run before trusting a copied snapshot."
linked_review: "docs/closure/2026-09-13-fix-mcp-serve-initialize-handshake-regression-adversarial-review.md"
```

```yaml
type: bug
title: "U-2: leg_has_valid_initialize ignores the evidence-wide valid flag"
description: "leg_has_valid_initialize only checks that initialize_correlation is present/non-null; it never checks wrapper_evidence[\"valid\"], so evidence explicitly marked invalid (e.g. due to channel saturation) can still be treated as a valid initialize, skewing H0a pass classification."
file: "tools/mcp-probe/src/exact_cli.rs"
line: 852
severity: "MAJOR"
confidence: "LOW"
fix: "Require wrapper_evidence[\"valid\"] == true in addition to a well-formed initialize_correlation."
linked_review: "docs/closure/2026-09-13-fix-mcp-serve-initialize-handshake-regression-adversarial-review.md"
```

```yaml
type: bug
title: "U-3: identify_copilot invokes the target executable with no deadline or guard"
description: "identify_copilot calls Command::new(exe_path).arg(\"--version\").output() directly with no wall-clock deadline and no ChildGuard, unlike every other child-process invocation in this crate; a wedged --version hangs the whole probe run indefinitely."
file: "tools/mcp-probe/src/exact_cli.rs"
line: 258
severity: "MAJOR"
confidence: "LOW"
fix: "Route identity collection through the existing deadline-governed ChildGuard/pump-and-reap composition with bounded capture."
linked_review: "docs/closure/2026-09-13-fix-mcp-serve-initialize-handshake-regression-adversarial-review.md"
```

```yaml
type: bug
title: "U-4: run_read_only_server_control boundary claim not accurate under stderr truncation"
description: "The function's docstring claims a write-path marker always fails the control closed, but the boundary check only inspects stderr and does not consult stderr_truncated; a marker emitted after the bounded capture window fills is silently unobservable and Control can still be returned. Scope note: documented as intended for a future, out-of-scope (T4/056.011-T) consumer, not currently invoked within this shipment's own test suite."
file: "tests/common/serve_driver.rs"
line: 694
severity: "MAJOR"
confidence: "LOW"
fix: "Treat stderr_truncated == true as an indeterminate/Diagnostic outcome rather than allowing Control, or soften the docstring's guarantee claim to be explicitly conditioned on non-truncated capture."
linked_review: "docs/closure/2026-09-13-fix-mcp-serve-initialize-handshake-regression-adversarial-review.md"
```

```yaml
type: bug
title: "U-5: LineReassembler/CollectorState have no size or event-count bound"
description: "LineReassembler.pending and CollectorState.events grow unboundedly; an unterminated frame or sustained stream of small frames can grow memory without limit even though the upstream delivery channel is itself bounded."
file: "tools/mcp-probe/src/evidence.rs"
line: 248
severity: "MAJOR"
confidence: "LOW"
fix: "Cap pending bytes and events count; call the existing mark_invalid(...) pattern on overflow rather than growing unbounded."
linked_review: "docs/closure/2026-09-13-fix-mcp-serve-initialize-handshake-regression-adversarial-review.md"
```

```yaml
type: bug
title: "U-6: Transport-level delivery drops invisible to EvidenceCollector"
description: "transport::deliver_copy silently drops a copy via try_send on a full/disconnected delivery channel, before EvidenceCollector's own hook (and its own saturation detection) ever sees it; EvidenceCollector can finalize valid: true despite upstream data loss."
file: "tools/mcp-probe/src/transport.rs"
line: 150
severity: "MAJOR"
confidence: "LOW"
fix: "Expose delivery loss via a counter/drop callback on the transport side; force EvidenceCollector's summary invalid whenever any transport-level copy was dropped."
linked_review: "docs/closure/2026-09-13-fix-mcp-serve-initialize-handshake-regression-adversarial-review.md"
```

*(MINOR-severity findings U-7/U-8/U-9/U-10 are advisory-only per Phase 5 and
are not opened as backlog items by default; see their individual sections
above if you choose to track them anyway.)*

---

## Verification Notes (appendix)

One reviewer-submitted finding was investigated and found factually
incorrect, and is **excluded** from all sections above:

> **Excluded — Anchor's original finding**: *"write_evidence_output claims
> atomic replacement but `std::fs::rename` fails on Windows when
> `evidence.json` already exists."*
>
> **Why excluded**: Rust's standard library implementation of
> `std::fs::rename` on Windows uses `MoveFileExW` with the
> `MOVEFILE_REPLACE_EXISTING` flag, which succeeds when the destination file
> already exists (same volume, ordinary case) — this has been the behavior
> for a long time, not a recent fix. The claimed failure mode does not
> reproduce as a blanket statement; `write_evidence_output`'s atomic-rename
> approach is sound for its stated purpose. (A narrower, unclaimed edge case
> does exist — `MOVEFILE_REPLACE_EXISTING` can still fail if another process
> holds the destination open without `FILE_SHARE_DELETE` — but this is not
> what the reviewer described and was not independently pursued further as
> a new finding, since it was outside what either reviewer actually flagged.)

One finding's severity/framing was **adjusted** from the reviewer's original
submission after independent source verification (see **U-7** above for full
detail): Anchor's original framing characterized `create_probe_workspace` as
a containment-escape/security risk; direct inspection of
`validate_containment`/`create_shared_dir_component_validated` shows the
containment check itself re-canonicalizes and would still catch a genuine
escape, so this was reclassified as a MINOR correctness/portability issue
rather than a security bypass.

```yaml
post_remediation:
  cycles_run: 0
  cap_reached: false
  residual_findings: 0
  status: "skipped"
```

Phase 7 (post-remediation re-review) was skipped per
`post_remediation_review: false`. Ship owns applying fixes from the
remediation plan above and re-verifying via `cargo check`/`clippy`/`fmt`/
`test`; no files were edited by this review (report-only, as instructed).

---

## Already-known context re-confirmed present (not new findings)

Per the request's "avoid re-flagging" list, the following were spot-checked
during this review's independent verification pass and confirmed still
correctly present:

- `validate_nonce()` (rejects unsafe nonces before any join/create) and
  `create_shared_dir_component_validated()` (per-component containment
  validation, never a single `create_dir_all`) — both present and correct in
  `tools/mcp-probe/src/workspace.rs`.
- Mutex poison-recovery (`.lock().unwrap_or_else(std::sync::PoisonError::into_inner)`)
  — present and correct in `evidence.rs` (`CollectorState`, `CaptureSink`)
  and elsewhere.
- Panic-catching (`catch_unwind`) in the transport delivery worker
  (`spawn_delivery_worker`) — present and correct in `transport.rs`.
- Owner-only (`0600`) `.mcp.json` fixture writes via `write_owner_only` — present
  and correct in `workspace.rs` (note: this owner-only pattern is
  **not** applied to the evidence-output file itself — see U-9 above).
- `ServePreflightErrorStage::DuplicateIntake` trace wrap in `cmd_serve` —
  present and correct in `main.rs`.
- No explicit `[[bin]]`/`[lib]` in `tools/mcp-probe/Cargo.toml` — confirmed.

**Known/tracked residual risk (P-021-deferred, stash ECD56875) — noted, not
re-flagged as new**: `run_leg` in `exact_cli.rs` passes `--allow-all-tools`
to the real Copilot CLI child with only a prompt-level safety instruction and
no technical enforcement. This is intentionally out of scope for this
shipment per the request; independently reconfirmed present at the same call
site (`run_leg`'s `Command` construction) during this review.

---

## Summary for the operator

- **HIGH-confidence (Consensus) findings requiring mandatory fix before PR**:
  **zero**. No finding was independently caught by all 3 reviewers.
- **MEDIUM-confidence (Majority) findings requiring explicit
  fix-or-defer-with-rationale**: **one** (M-1, `CaptureSink` unbounded
  capture in `tools/mcp-probe/src/exact_cli.rs`, MAJOR severity).
- **MEDIUM-confidence (Plurality) findings**: none possible at `reviewers: 3`
  (structural, not a gap).
- **LOW-confidence (Unique) findings**: 10 total (6 MAJOR, 4 MINOR) after
  independent verification excluded 1 reviewer claim as factually incorrect
  and reframed 1 other's severity; all are human-judgment items, not
  merge-blocking, but 6 of them (U-1 through U-6) describe MAJOR-severity
  correctness gaps in the probe's own diagnostic-validity logic (stale
  evidence reuse, ignored validity flags, missing deadlines, unbounded
  buffers/collections, and an invisible transport-level drop path) that are
  worth triaging given they affect the trustworthiness of the probe tool's
  own output — even though none were independently corroborated by a second
  reviewer.

**Output file**: `docs/closure/2026-09-13-fix-mcp-serve-initialize-handshake-regression-adversarial-review.md`
