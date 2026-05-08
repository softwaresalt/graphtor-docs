---
problem_type: build_failure
category: best-practices
component: release-workflow
root_cause: openssl-sys cannot locate x86_64 OpenSSL headers when cross-compiling x86_64-apple-darwin on an ARM64 macOS runner
resolution_type: cargo-target-dep
severity: high
message: "openssl-sys: OPENSSL_VENDORED env var is ignored — use the 'vendored' Cargo feature via a target-specific dep"
file_path: Cargo.toml
citations:
  - Cargo.toml
  - .github/workflows/release.yml
tags: [openssl, cross-compile, macos, ci, release]
---

## Problem

When building `x86_64-apple-darwin` on `macos-latest` (ARM64 runner), `openssl-sys` fails:

```
Could not find directory of OpenSSL installation, and this `-sys` crate cannot proceed without this knowledge.
```

`macos-13` (Intel) runners appeared to fix this by doing a native build, but they are no longer
available (no runners are assigned — jobs stay "queued" indefinitely).

Setting `OPENSSL_VENDORED=1` as a CI environment variable does **not** work — `openssl-sys` ignores
this env var entirely. Vendored mode is a **Cargo feature**, not an env var.

## Root Cause

`openssl-sys` cross-compilation requires either:
1. System OpenSSL for the **target** architecture (x86_64) — not available on ARM64 macOS runners, or
2. The `vendored` Cargo feature, which compiles OpenSSL from source using Perl.

macOS ships Perl at `/usr/bin/perl`, satisfying the `vendored` build requirement.

## Resolution

Add a target-specific dependency in `Cargo.toml` to activate vendored OpenSSL **only** for the
`x86_64-apple-darwin` target:

```toml
[target.'cfg(all(target_os = "macos", target_arch = "x86_64"))'.dependencies]
openssl = { version = "0.10", features = ["vendored"] }
```

This:
- Activates the `vendored` feature for `x86_64-apple-darwin` builds (on any host)
- Does NOT activate for Windows or Linux builds
- Does NOT activate for `aarch64-apple-darwin` native ARM64 builds (which find Homebrew OpenSSL)
- Does NOT break local Windows dev builds (Perl not needed)

## Prevention

- Never try to enable `openssl-sys` vendored mode via an env var — it does not work.
- When cross-compiling `x86_64-apple-darwin` from any ARM64 host, always use the Cargo `vendored` feature.
- Use `[target.'cfg(...)'.dependencies]` for platform-specific dependencies to avoid polluting other targets.
- `macos-13` Intel runners should not be relied upon; use `macos-latest` (ARM64) with vendored OpenSSL instead.
