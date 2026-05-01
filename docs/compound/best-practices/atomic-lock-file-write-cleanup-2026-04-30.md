---
title: Atomic lock file write failure cleanup
tags: [rust, concurrency, lock-file, error-handling]
date: 2026-04-30
---

## Problem

After `OpenOptions::new().create_new(true).open()` succeeds (O_CREAT|O_EXCL),
a subsequent `write_all()` failure leaves an empty lock file on disk. Future
`create_new` attempts return `AlreadyExists` indefinitely, blocking all lock
acquisitions.

## Solution

On `write_all` error: drop the file handle first (to flush/close), then call
`fs::remove_file` best-effort before returning the error.

```rust
Ok(mut file) => {
    if let Err(e) = file.write_all(content.as_bytes()) {
        drop(file);                      // close before remove
        let _ = fs::remove_file(&path);  // best-effort cleanup
        return Err(GraphtorError::Config {
            message: format!("failed to write lock file: {e}"),
            field: None,
        });
    }
    Ok(Self { path })
}
```

## Concurrent-release race

Between `create_new` returning `AlreadyExists` and a subsequent `metadata()`
call, another process may release the lock (delete the file). This produces
`NotFound` from `metadata()`, which must be handled by retrying `create_new`
once rather than returning a false "workspace is locked" error.

```rust
Err(e) if e.kind() == ErrorKind::NotFound => {
    return retry_create_lock(&path, content);
}
```

## Citations

- `src/workspace/lock.rs` — `acquire()`, `handle_existing_lock()`,
  `retry_create_lock()` (PR #12, commit 61e9d8b)
