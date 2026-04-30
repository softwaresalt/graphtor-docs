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
the CI step. Each suppressed advisory requires its own flag:

```yaml
# .github/workflows/ci.yml
- name: Security audit
  run: |
    cargo audit \
      --ignore RUSTSEC-2026-0041 \
      --ignore RUSTSEC-2026-0008
```

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

- PR #5: `fix(ci): suppress transitive audit advisories from cozo and git2 deps`
- `audit.toml` in repo root — documents both advisories
- `.github/workflows/ci.yml` lines 36–44 — `--ignore` flags

## Related Advisories (2026-04-29)

| Advisory | Crate | Severity | Root Cause |
|---|---|---|---|
| RUSTSEC-2026-0041 | lz4_flex 0.10.0 | High (8.2) | Transitive: `cozo → swapvec → lz4_flex ^0.10`; semver-locked |
| RUSTSEC-2026-0008 | git2 0.19.0 | Medium | Direct dep; unsound `Buf` deref UB; fix pending git2 0.20 |
