---
title: fs::copy does not preserve source mtime — use SHA-256 for binary equality
tags: [rust, fs, upgrade, hashing, mtime]
date: 2026-04-30
---

## Problem

`std::fs::copy` does **not** preserve the source file's modification time on
most platforms (Linux, macOS, Windows). The destination always gets the
current wall clock as its mtime. Any equality check based on
`src_mtime == dst_mtime` will fail after every copy, causing a "binary is
already up-to-date" check to never return true.

## Solution

Use SHA-256 content hashing to determine binary equality:

```rust
use sha2::{Digest as _, Sha256};

fn file_sha256(path: &Path) -> Option<Vec<u8>> {
    let data = std::fs::read(path).ok()?;
    Some(Sha256::digest(&data).to_vec())
}

// In upgrade():
let src_hash = file_sha256(&exe);
let dst_hash = file_sha256(&dest);
if src_hash.is_some() && src_hash == dst_hash {
    return Ok(UpgradeResult { upgraded: false, ... });
}
```

`sha2` is a zero-dependency pure-Rust crate already used for chunk IDs in
this project, so no new dependency is needed.

## Citations

- `src/workspace/upgrade.rs` — `upgrade()`, `file_sha256()` (PR #12,
  commit 61e9d8b)
- `src/chunk/id.rs` — prior sha2 usage as precedent
