# Compound Learning: Keep Docs Synchronized with Actual Implementation State

**Category:** Documentation / Code Review  
**Discovered:** 2026-04-29  
**Context:** PR #6 Copilot review on copilot-instructions.md

## Problem

Documentation described capabilities that were planned but not yet implemented.
Specifically, `src/db/search.rs` was described as "HNSW vector search" in three
places, but the actual implementation provides keyword/text search only, with
`search_similar` returning a `not-implemented` error.

Copilot review flagged all three occurrences (Tech Stack table, Project
Structure listing, Architecture Reference) as false accuracy claims.

## Solution

When documenting module capabilities in architecture or developer guidelines:

1. **Describe what exists**, not what is planned.
2. For planned features, use explicit markers: `(planned)`, `(future)`, or
   `(not yet implemented)`.
3. After implementing a new capability, update the docs in the same PR as the
   implementation — not a separate follow-up.

### Correct Pattern

```markdown
# In Tech Stack table:
| Unified Store | CozoDB | ... text/keyword search; HNSW vector search: planned |

# In Project Structure:
search.rs             # Text/keyword search (HNSW vector search: planned)

# In Architecture Reference:
| Unified storage | CozoDB — Datalog, graph traversal; HNSW vector search: planned |
```

## Writing Style Note

Copilot review also flagged em dashes (`—`) in inline comments as violating
the repo's writing-style guidelines. In code comments and file structure
listings, use colons (`:`) rather than em dashes.

```
# Wrong:
store.rs              # CozoDB DataStore — open, schema, lifecycle

# Correct:
store.rs              # CozoDB DataStore: open, schema, lifecycle
```

## Evidence

- PR #6, Copilot review: 5 findings, all valid, all fixed in commit `2dc0e94`
- Affected file: `.github/copilot-instructions.md`
