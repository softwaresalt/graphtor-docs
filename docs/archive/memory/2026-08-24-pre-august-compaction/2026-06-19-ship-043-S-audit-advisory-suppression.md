---
session_type: ship
date: 2026-06-19
shipment_id: 043-S
chore_id: 035-C
tasks: [035.001-T, 035.002-T]
branch: chore/043-S-audit-advisory-suppression
status: pr-pending-creation
merge: NOT MERGED — awaiting operator approval (P-014)
---

# Ship Memory — 043-S Post-042-S Unmaintained Transitive Audit Advisory Suppression

## Outcome

Took shipment 043-S to a green, review-clean state and committed the work on a
feature branch. Stops at merge-ready PR per P-014 (no merge, no post-merge closure).

## Steps completed

- P-012 tool gate: cargo 1.93.1, git 2.54, gh 2.81, backlogit 1.2.0 all OK.
  No `.autoharness/backlog-registry.yaml` → file-backed mode (intended).
- Index sync OK (390 artifacts).
- P-001 OK (no other active item/shipment). Constitution I/II/IV re-read
  (config-only change; library TDD/.unwrap rules N/A to these edits).
- Branch created from clean main: `chore/043-S-audit-advisory-suppression`.
- Shipment 043-S claimed (active). Baseline audit characterized: 1 vuln
  (lz4_flex, out of scope) + 5 unmaintained warnings. git2/RUSTSEC-2026-0008
  confirmed absent from Cargo.lock.
- Harness red state proven: `cargo audit --ignore RUSTSEC-2026-0041 --deny warnings`
  → `error: 5 denied warnings found!` (exit 1).

## Unit 1 — 035.001-T spike (indicatif/number_prefix): INFEASIBLE -> suppress

Bumped indicatif 0.17->0.18 + hf-hub 0.3->0.4 (resolved 0.18.4 / 0.4.3).
number_prefix NOT removed: indicatif 0.18 drops it (unit-prefix), but hf-hub
0.4.3 still pins indicatif ^0.17 — both its API features (ureq = the sync
`hf_hub::api::sync::Api` used by src/embed/model.rs, and tokio = async) force
`dep:indicatif`. The 0.4 bump also added a large async cascade
(reqwest/tokio/tower/hyper/h2/tokio-rustls). Reverted manifest experiment.
Decision: RUSTSEC-2025-0119 is suppress-only.

## Unit 2 — 035.002-T suppression + CI hardening: DONE

- `audit.toml`: added 5 unmaintained advisories (adler 0056, bincode 0141,
  fxhash 0057, number_prefix 0119, paste 0436) with transitive path + named
  upstream blocker + `Review: 2026-09-18`. Removed obsolete git2 (0008).
  Preserved lz4_flex (0041, owned by 013.008-T).
- `.github/workflows/ci.yml`: audit step `--ignore` set now exactly matches
  audit.toml (6 IDs), git2 dropped, `--deny warnings` added (enforced allowlist).

## Quality gates (ALL GREEN)

- `cargo fmt --all -- --check` → pass
- `cargo clippy --all-targets -- -D warnings -D clippy::pedantic` → pass
- `cargo test --all-targets` → pass (257 unit + all integration suites, 0 failed)
- Exact CI command `cargo audit --ignore <6 ids> --deny warnings` → exit 0, 0 warnings
- Code review (code-review agent) → PASS, no issues

## Commits

- `56dee27` chore(ci): suppress post-042-S unmaintained audit advisories with
  allowlist gate (audit.toml + ci.yml). Associated with 035-C, 035.002-T.
- backlog state commit (035-C/035.001-T/035.002-T -> done; spike outcome recorded).

## BLOCKER flagged for Stage (backlog hygiene, non-blocking for code)

ID collision: task IDs `035.001-T` / `035.002-T` are reused — they exist BOTH in
`.backlogit/queue/` (chore 035-C, this shipment) AND in `.backlogit/archive/`
(archived feature 035-F: "Remove Editor::Copilot variant" etc.). The backlogit
CLI mis-resolves these IDs to the archive copies; `backlogit update/move` on them
silently corrupts the archived 035-F records (caught + restored via P-007
`git restore .backlogit/archive/`). Therefore task status was tracked via direct
queue-file frontmatter edits, NOT backlogit CLI mutations.

ACTION REQUIRED before post-merge closure: `backlogit shipment ship 043-S` will
archive 035.001-T/035.002-T into `.backlogit/archive/`, colliding with the
existing 035-F archive files. Stage must renumber/disambiguate the 035-C task IDs
(or the archived 035-F task IDs) before the shipment is archived, or closure will
overwrite/lose the 035-F history.

## Next steps

1. Push branch, open PR to main, request Copilot review.
2. Poll CI + Copilot review; address findings within circuit-breaker limits.
3. Run §1.9 pre-merge readiness gate.
4. HALT for operator merge approval (P-014). Do NOT merge. Do NOT run Step 6.
