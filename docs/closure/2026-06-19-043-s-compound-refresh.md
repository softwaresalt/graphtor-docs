---
date: 2026-06-19
slug: 043-s-compound-refresh
shipment: 043-S
skill: compound-refresh
mode: apply
context: Post-merge closure for PR #71 / shipment 043-S (cargo-audit allowlist hardening + backlogit ID-collision resolution)
owner: copilot
---

# Compound Refresh Report — 043-S

## Scope & Evidence

Refresh triggered by post-merge closure of shipment `043-S`. Evidence sources:

* Shipped `audit.toml` (merge `5441384`) — 6 ignored advisories: RUSTSEC-2026-0041,
  -2025-0056, -2025-0141, -2025-0057, -2025-0119, -2024-0436; the git2
  RUSTSEC-2026-0008 entry is **gone**.
* Shipped `.github/workflows/ci.yml` audit step — pins `cargo install cargo-audit
  --version "^0.22" --locked`, lists the 6 `--ignore` flags, and adds
  `--deny warnings` (allowlist enforcement).
* Deliberation `002-DL` chosen direction (Option B: allowlist + drop git2 +
  attempt number_prefix upgrade).
* Post-ship backlog state — zero queue/archive ID collisions; clean archival.

## Entries Reviewed & Classifications

### 1. `docs/compound/cargo-audit-workspace-config-limitation.md` → **update**

Core learning (cargo-audit 0.22 does not auto-discover workspace `audit.toml`;
suppress via CLI `--ignore`) remains accurate and valuable. Details drifted:

* **Added** the `--deny warnings` allowlist-hardening pattern to the Solution —
  this is the meaningful new capability shipped in 043-S (passive filter →
  enforced allowlist that fails CI on any unlisted advisory).
* **Added** the `cargo install cargo-audit --version "^0.22" --locked` pin so the
  documented 0.22-specific behavior stays aligned with what CI installs.
* **Corrected** the stale `## Related Advisories` table: the git2 0.19
  RUSTSEC-2026-0008 suppression was resolved and **dropped** in 043-S. Preserved
  the historical PR #5 table for provenance and annotated it with a NOTE pointing
  to the current 043-S set and `audit.toml` as the authoritative record.
* **Refreshed** Evidence: kept the PR #5 discovery citation, added PR #71 /
  shipment 043-S (merge `5441384`).

Citations preserved and strengthened; no substance removed.

### 2. `docs/compound/backlogit-level1-id-collision-across-parent-types.md` → **update (light)**

Learning is accurate as written. Added a `## Resolution (confirmed 2026-06-19)`
section recording shipped reality: the renumber `035-C → 043-C` (children
`043.001-T`, `043.002-T`) drove the collision scan to zero, and
`backlogit shipment ship 043-S` (merge `5441384`) archived all four items with no
archive clobbering — validating prevention rules #3 (recreate at the correct
shared number) and #5 (gate `shipment ship` on a clean scan). This is
evidence-backed confirmation, not a rewrite.

## Entries Left As-Is

* `docs/compound/backlogit-shipment-state-machine.md` — not in scope of this
  shipment's changes; left `keep` without edit.

## Follow-Up

* None for the compound library. The 2026-09-18 advisory re-review is tracked in
  the post-merge closure record, not in compound.
