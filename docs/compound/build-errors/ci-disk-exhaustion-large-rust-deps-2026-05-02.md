---
title: CI disk exhaustion on ubuntu-latest with large Rust dependency trees
tags: [ci, rust, disk-space, github-actions]
date: 2026-05-02
shipment: 011-S
---

## Problem

When adding large Rust crate families (reqwest + rustls-tls, scraper/html5ever,
docx-rs), `cargo test --all-targets` exhausted the ubuntu-latest runner disk
(`No space left on device`). The runner's ~14 GB of usable disk is consumed by:
- Pre-installed tools that are never used (Android SDK, .NET, GHC, CodeQL)
- Debug build artifacts from all new transitive crates
- The mold linker output binary

This produces a GitHub Actions runner error like:
```
System.IO.IOException: No space left on device :
  '/home/runner/actions-runner/extracted/_diag/Worker_*.log'
```

This is easy to misdiagnose — the annotation points at runner infrastructure
failure, not a Rust build error.

## Fix

Add a "Free disk space" step immediately after `actions/checkout`, before any
Rust compilation. Remove the largest unused tool directories:

```yaml
- name: Free disk space
  run: |
    sudo rm -rf /usr/share/dotnet
    sudo rm -rf /usr/local/lib/android
    sudo rm -rf /opt/ghc
    sudo rm -rf /opt/hostedtoolcache/CodeQL
    sudo docker image prune --all --force
    df -h
```

This frees ~10 GB before the build starts.

## Detection

Check the GitHub check-run annotations, not the truncated log tail:

```bash
gh api "repos/{owner}/{repo}/check-runs/{run_id}/annotations" \
  --jq '.[] | .message'
```

If the message contains `No space left on device` the runner is disk-full.
`cargo audit`/`cargo test` may appear to be the failing step but the root
cause is always runner disk exhaustion.

## Prevention

Keep the free-disk-space step in `ci.yml` for any workspace with large dep
trees. Alternatively, use `jlumbroso/free-disk-space@main` for a more
comprehensive sweep.
