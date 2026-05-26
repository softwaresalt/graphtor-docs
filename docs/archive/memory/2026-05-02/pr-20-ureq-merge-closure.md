---
type: session-memory
timestamp: 2026-05-02T21:05:00-07:00
agent: Ship
pr: "20"
merge_commit: f4166c9e97a4675f3c01367222126e6035731f36
shipment_shipped: "011-S"
---

## Session: PR #20 — reqwest → ureq fix

### Outcome

PR #20 merged to main (commit `f4166c9`). Shipment `011-S` (New Acquisition
Backends) archived with this commit as the final closure point.

### Changes Merged

| File | Change |
|------|--------|
| `src/acquire/url.rs` | Replace `reqwest::blocking` with `ureq` (pure-sync, no tokio dep) |
| `src/acquire/url.rs` | `fetch_robots_txt` uses dedicated 5s `ureq::Agent` |
| `src/acquire/url.rs` | `crawl_url_source` doc comment — removed stale "failing to build HTTP client" |
| `Cargo.toml` | `ureq = "2"` (default features = rustls only) |
| `tests/acquire_url_test.rs` | New regression test for nested-runtime panic |

### Copilot Review Rounds (2)

5 comments across 2 review rounds — all addressed and resolved.

### CI Fix Iterations

| Commit | Issue | Fix |
|--------|-------|-----|
| 53ad34d | `UrlSource` import path wrong; tokio `net` feature missing | Use `config::source::UrlSource`; use `std::net::TcpListener` |
| a203604 | rustfmt failure | No fix needed (superseded) |
| 594c664 | rustfmt: `not_found` split across 2 lines | Collapse to single line |

**Root cause of extra CI iterations**: HTTP/1.0 without `Content-Length` in the
test server was suspected to cause hangs on Linux CI. Switched to HTTP/1.1 +
`Content-Length: 30` + `Connection: close` to give ureq an explicit body length.
(The previous run actually passed — the change was a robustness improvement.)

### Backlog Actions

- `011-S` shipped (archived) with merge commit `f4166c9`
- PR #21 (`chore/backlog-cleanup`) created: removes 6 duplicate queue files + adds docs stash entry `493CA939`
- Stash entry `493CA939` added: comprehensive graphtor-docs documentation (high priority)

### State After Session

- `main` is at commit `f4166c9` — fully functional web crawler with ureq
- `.graphtor/config/sources.yaml` has 4 sources (AVM URL, tmp PDFs, tmp DOCX, fabric-rest-api-specs Git)
- `.graphtor/graph.db` is 17 MB with indexed content

### Next Steps

1. Merge PR #21 (backlog cleanup — no CI, only `.backlogit/` files)
2. Consider harvesting stash entry `493CA939` (documentation) into a new feature when ready
3. The `013.008-T` blocked task (upgrade cozo/git2 for audit advisories) remains blocked
