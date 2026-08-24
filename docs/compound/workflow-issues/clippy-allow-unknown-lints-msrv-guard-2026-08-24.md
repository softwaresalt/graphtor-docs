---
title: "clippy::allow for a newer pedantic lint can itself break older-MSRV builds via unknown_lints"
description: "Suppressing a clippy lint that postdates the crate's declared MSRV with a bare #[allow(clippy::new_lint)] fails -D warnings on older toolchains via unknown_lints; guard with #[allow(unknown_lints)] first"
problem_type: "lint_failure"
category: "workflow-issues"
component: "src/mcp/server.rs"
root_cause: "A clippy::pedantic lint (clippy::unused_async_trait_impl) that postdates the crate's declared rust-version (1.75) was suppressed with a bare #[allow(clippy::unused_async_trait_impl)]; on an older clippy that does not recognize the lint name, the unrecognized name itself trips the unknown_lints lint, which becomes a hard error under -D warnings"
resolution_type: "code_fix"
severity: "medium"
message: "warning: unknown lint: `clippy::unused_async_trait_impl` [unknown_lints]"
file_path: "src/mcp/server.rs"
citations:
  - "PR #106: chore/stage-049-S staging PR, Ship CI-drift remediation commits 7dee9c7 and 37e9acf"
  - "Copilot automated PR review comment (databaseId 3847243993) on PR #106"
tags:
  - "clippy"
  - "pedantic"
  - "unknown_lints"
  - "msrv"
  - "rust-version-skew"
  - "ci"
---

## Problem

CI's floating `stable` Rust toolchain (`dtolnay/rust-toolchain@... stable` in
`.github/workflows/ci.yml`) advanced from 1.97.0 to 1.98.0 mid-review and
introduced a new `clippy::pedantic` lint, `clippy::unused_async_trait_impl`,
which flagged an `rmcp-macros`-generated `#[tool_handler]` async trait impl
(the macro's generated body never reaches an `.await` point). The first fix
applied a bare suppression:

```rust
#[allow(clippy::unused_async_trait_impl)]
#[tool_handler]
impl ServerHandler for DocServer { ... }
```

This fix was verified clean under Rust 1.97.0 and 1.98.0. However, an
automated Copilot review on the same PR correctly flagged a follow-on defect:
the crate's declared `rust-version = "1.75"` (Cargo.toml) predates the
existence of `clippy::unused_async_trait_impl` (added to Clippy well after
1.75.0 shipped). A contributor building with the declared MSRV toolchain and
this repository's documented `-D warnings -D clippy::pedantic` gate would hit:

```text
warning: unknown lint: `clippy::unused_async_trait_impl`
error: unknown lints are not allowed with -D warnings
```

because clippy 1.75 does not recognize the lint name at all, and an
unrecognized name inside `#[allow(...)]` itself triggers `unknown_lints`
(part of the default `unused` lint group), which `-D warnings` promotes to a
hard error.

## Root Cause

Suppressing a version-specific clippy lint with a bare `#[allow(clippy::lint_name)]`
assumes every toolchain that will compile the crate recognizes that lint
name. When a crate declares an MSRV older than the lint's introduction, that
assumption is false, and the suppression attribute itself becomes the new
failure under a strict `-D warnings` gate.

## Resolution

Add `#[allow(unknown_lints)]` immediately before the version-specific allow,
so an unrecognized lint name is itself tolerated:

```rust
#[allow(unknown_lints)]
#[allow(clippy::unused_async_trait_impl)]
#[tool_handler]
impl ServerHandler for DocServer { ... }
```

Verified clean under Rust 1.97.0 and 1.98.0. End-to-end verification under
the literal declared MSRV (1.75.0) was blocked in this repository by an
unrelated, pre-existing `Cargo.lock` issue (a transitive dependency requiring
the `edition2024` Cargo feature, unsupported by Cargo 1.75) — confirmed
pre-existing and untouched by the PR that surfaced this finding. The
`unknown_lints` guard is nonetheless the textbook-correct, standard idiom for
this exact situation and is safe to apply regardless.

## Prevention

1. **Whenever suppressing a `clippy::pedantic` (or any versioned) lint that
   was added after the crate's declared MSRV, pair it with
   `#[allow(unknown_lints)]`** placed immediately before the version-specific
   allow, so older toolchains that do not recognize the lint name do not fail
   on the allow attribute itself under `-D warnings`.
2. Check the lint's introduction version against `Cargo.toml`'s
   `rust-version` before adding a bare `#[allow(clippy::...)]` for a
   recently-added lint.
3. This is a distinct, second-order gotcha from plain CI/local toolchain
   skew (see the sibling `clippy-useless-conversion-ci-rust-version-skew` and
   `clippy-pedantic-map-unwrap-or-ci-vs-local` entries): those document a new
   lint *firing unexpectedly*; this one documents the *fix for that* breaking
   a different, older toolchain if not guarded.
4. Periodically verify the crate actually builds under its declared MSRV
   (`cargo +<msrv> build`), independent of clippy — this session discovered
   this repository's current `Cargo.lock` cannot even resolve under Rust
   1.75.0 (`globset 0.4.18` requires the `edition2024` Cargo feature),
   meaning the MSRV promise is not currently exercisable at all. That is a
   separate, wider issue worth its own follow-up.
