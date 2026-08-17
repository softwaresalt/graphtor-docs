# Compound Learning: cargo-audit 0.22 Does Not Auto-Discover Workspace audit.toml

**Category:** Build / CI  
**Discovered:** 2026-04-29  
**Context:** PR #5 — CI audit failure from transitive lz4_flex advisory

## Problem

`cargo audit` 0.22 does **not** auto-discover an `audit.toml` file in the
workspace root. Adding `[advisories] ignore = [...]` entries to `audit.toml`
has **no runtime effect** — advisories are still reported as failures.

## Solution

Pass `--ignore <ADVISORY_ID>` flags directly on the `cargo audit` CLI call in
the CI step. Each suppressed advisory requires its own flag, and pair the set
with `--deny warnings` so the ignore list becomes an explicit **allowlist**: any
NEW unmaintained/unsound advisory that is not listed fails CI and forces a
re-triage.

```yaml
# .github/workflows/ci.yml
- name: audit
  run: |
    # Pin cargo-audit to the 0.22 minor line so a breaking 0.23+ release cannot
    # silently change the gate's behavior (still allows 0.22.x patch updates).
    cargo install cargo-audit --version "^0.22" --locked
    cargo audit \
      --ignore RUSTSEC-2026-0041 \
      --ignore RUSTSEC-2025-0056 \
      --ignore RUSTSEC-2025-0141 \
      --ignore RUSTSEC-2025-0057 \
      --ignore RUSTSEC-2025-0119 \
      --ignore RUSTSEC-2024-0436 \
      --ignore RUSTSEC-2026-0249 \
      --deny warnings
```

`--deny warnings` is what upgrades the suppression list from a passive filter to
an enforced allowlist. The authoritative, always-current suppression set lives
in `audit.toml` (rationale + review dates) and `.github/workflows/ci.yml` (the
enforced `--ignore` flags); treat the list above as representative, not
exhaustive.

## Why audit.toml Still Exists

`audit.toml` documents the suppression rationale and upgrade tracking. Keep it
as human-readable documentation. If the project migrates to `cargo-deny`, the
`[advisories] ignore` section activates automatically (cargo-deny reads
`audit.toml`).

## Forward Path

When `cargo-deny` replaces `cargo audit` in CI:
1. Add `cargo-deny` to the CI step.
2. The `[advisories]` section in `audit.toml` takes effect with no changes.
3. Remove the `--ignore` flags from the CI step.

## Evidence

- PR #5 (2026-04-29): `fix(ci): suppress transitive audit advisories from cozo
  and git2 deps` — original discovery of the no-auto-discovery limitation.
- PR #71 / shipment 043-S (2026-06-19): `chore(ci): suppress post-042-S
  unmaintained audit advisories with allowlist gate` (merge `5441384`) — added
  `--deny warnings` allowlist hardening, pinned cargo-audit to `^0.22`, and
  dropped the resolved git2 0.19 (RUSTSEC-2026-0008) suppression.
- `audit.toml` in repo root — authoritative human-readable suppression record.
- `.github/workflows/ci.yml` audit step — enforced `--ignore` flags + `--deny
  warnings`.

## Related Advisories (origin set, 2026-04-29 — historical)

| Advisory | Crate | Severity | Root Cause |
|---|---|---|---|
| RUSTSEC-2026-0041 | lz4_flex 0.10.0 | High (8.2) | Transitive: `cozo → swapvec → lz4_flex ^0.10`; semver-locked |
| RUSTSEC-2026-0008 | git2 0.19.0 | Medium | Direct dep; unsound `Buf` deref UB; fix pending git2 0.20 |

> [!NOTE]
> This table reflects the original PR #5 suppression set. As of shipment 043-S
> (PR #71, 2026-06-19) the git2 0.19 advisory (RUSTSEC-2026-0008) was resolved
> and **dropped** from the ignore list, and five unmaintained-crate advisories
> were added behind the `--deny warnings` allowlist: RUSTSEC-2025-0056 (adler),
> RUSTSEC-2025-0141 (bincode), RUSTSEC-2025-0057 (fxhash), RUSTSEC-2025-0119
> (number_prefix), and RUSTSEC-2024-0436 (paste). See `audit.toml` for the
> authoritative current set and 2026-09-18 review dates.

## Newest Addition (shipment 047-S, 2026-08-17)

CI on PR #97 failed on a **newly published** advisory that appeared in the
`RustSec` advisory database sometime between shipment 043-S and 047-S:
`RUSTSEC-2026-0249` (`smartstring` 1.0.1, unmaintained, a **direct**
dependency of `cozo 0.7.6` per `cargo tree -i smartstring`). This is the
weekly-scheduled-run trip-wire (see the `ci.yml` `schedule:` comment)
working exactly as designed: the allowlist mechanism intentionally fails CI
on any advisory not yet triaged, forcing a conscious decision rather than
silently accepting new supply-chain risk. Added to both `audit.toml` and
`ci.yml`'s `--ignore` list following the identical pattern above, with the
same `Review: 2026-09-18` date as the other unmaintained-crate suppressions
(batch re-triage is simpler than staggered per-advisory dates). Confirmed
this specific advisory reproduces identically on `main` with zero code
changes — a pure function of advisory-database freshness, not anything
introduced by the PR that happened to surface it. **Lesson for future
shipments**: if `cargo audit` fails in CI on a PR that did not touch
`Cargo.toml`/`Cargo.lock`, check whether the SAME failure reproduces on
`main` directly before assuming the PR introduced a regression — it is very
likely this same "new advisory published since the allowlist was last
curated" situation, not a dependency the PR added.
