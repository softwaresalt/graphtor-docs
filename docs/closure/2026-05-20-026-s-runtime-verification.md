---
date: 2026-05-20
slug: 026-s-runtime-verification
shipment: 026-S
surface: cli
mode: manual
status: BLOCKED
merge_commit: 69a4fb75b492e56b916965592c8a5a264ac39216
owner: copilot
---

# Runtime Verification — 026-S Remove non-functional Editor::Copilot MCP config path

## Verification Target

Verify the shipped CLI surface for:

* `install` help text only listing `vscode` and `cursor`
* editor parsing rejecting `copilot`
* uninstall cleanup still removing legacy `.github/copilot/mcp.json`

## Preconditions

* PR `#49` merged at `69a4fb75b492e56b916965592c8a5a264ac39216`
* The merged shipment diff only touched:
  * `src/cli/mod.rs`
  * `src/main.rs`
  * `src/workspace/mcp_config.rs`
  * `src/workspace/uninstall.rs`
* `src/acquire/url.rs` is outside the `026-S` diff

## Commands Attempted

```text
cargo test remove_mcp_configs_removes_legacy_copilot_file
```

## Expected Behavior

* The workspace compiles cleanly
* The targeted uninstall cleanup test runs and passes
* Follow-up CLI checks can exercise the shipped `install` surface on the merged code

## Observed Behavior

The verification command did not reach the targeted test. Compilation failed first
with unrelated baseline errors in `src/acquire/url.rs`:

* unresolved import `url::Url`
* type inference failures at lines 357, 371, and 375

Because the repository does not currently compile on the merged default-branch code,
meaningful post-merge CLI verification of `026-S` could not proceed.

## Evidence

* `026-S` diff excludes `src/acquire/url.rs`
* build failure occurs before the shipped CLI path can be exercised
* the blocker is outside the shipped shipment scope

## Verdict

**BLOCKED**

## Blocking Condition

Baseline compilation is failing in `src/acquire/url.rs`, which is outside shipment
`026-S`. Fix that unrelated compile break before re-running CLI verification on the
merged branch.

## Recommended Next Action

After the baseline compile issue is resolved, re-run:

```text
cargo run --bin graphtor-docs -- install --help
cargo run --bin graphtor-docs -- install --editor copilot
cargo test remove_mcp_configs_removes_legacy_copilot_file
```
