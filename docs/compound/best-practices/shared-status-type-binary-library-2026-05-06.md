# Shared SyncStatus Across Binary and Library Crates

**Recorded:** 2026-05-06  
**Context:** 020-S — spawn_background_sync returning Arc<Mutex<SyncStatus>> to DocServer

## Problem

A binary crate (`main.rs`) needed to share a live status type (`SyncStatus`) with a
library crate type (`DocServer` in `src/mcp/server.rs`). Both crates must reference
the same type to avoid duplicate definitions and to allow the binary to write status
while the server reads it.

## Solution

Define the type in the library crate and re-export it at a public module boundary.

**In `src/mcp/server.rs` (library crate):**
```rust
#[derive(Debug, Default)]
pub enum SyncStatus {
    #[default]
    Idle,
    Syncing,
    Done { files: usize, chunks: usize },
    Error(String),
}
```

**In `src/mcp/mod.rs`:**
```rust
pub use server::{DocServer, SyncStatus};
```

**In `src/main.rs` (binary):**
```rust
use graphtor_core::mcp::SyncStatus;
```

The `Arc<Mutex<SyncStatus>>` is constructed in the binary, populated by
`spawn_background_sync`, and injected into `DocServer::with_sync_status()`.

## Key Rules

- `SyncStatus` must be `pub` in `server.rs` for the binary to import it
- Re-export at the `mcp` module level for clean import paths
- Use `std::sync::Mutex` (not tokio) — the critical section is a single status
  write/read; no async code holds the lock across await points
- The `Arc` clone count is exactly 2: one for the background task writer, one held
  by `DocServer` for the reader

## Avoid

- Defining the type in `main.rs` — it can't be imported by the library crate
- Using `tokio::sync::Mutex` when there are no async lock holders — adds overhead
  for no benefit
- Cloning `SyncStatus` across threads without `Arc` — would copy the status, not share it
