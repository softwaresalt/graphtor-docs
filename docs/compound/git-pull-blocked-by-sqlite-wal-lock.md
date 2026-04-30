# Compound Learning: Git Pull Blocked by Locked SQLite WAL File

**Category:** Git / Tooling  
**Discovered:** 2026-04-29  
**Context:** Syncing local main after PR merges while backlogit MCP server is running

## Problem

When the `backlogit` MCP server is running, it holds an exclusive lock on
`.backlogit/backlogit.db-wal`. Git tracks this file (it was committed
accidentally). When `git pull`, `git merge`, or `git checkout` tries to update
the WAL, it fails:

```
error: Your local changes to the following files would be overwritten by merge:
        .backlogit/backlogit.db-wal
```

The "Unlink" error occurs because git needs to delete-and-replace the file,
which requires an exclusive lock that SQLite already holds.

## Solution

Use `git update-index --cacheinfo` to manually set the index entry for the WAL
file to match the remote version's blob hash. Then the merge/pull succeeds
using `-X theirs`:

```bash
# Step 1: Find the remote blob hash for the WAL
git ls-tree origin/main .backlogit/
# Look for the backlogit.db-wal blob hash, e.g.: 66ec29b45c0f01420c2f3f51b028781f824cad7d

# Step 2: Update the index entry (without touching the actual file)
git update-index --cacheinfo 100644,<BLOB_HASH>,.backlogit/backlogit.db-wal

# Step 3: Merge accepting remote changes (-X theirs handles other files)
git merge -X theirs origin/main
```

## Root Cause

`.backlogit/backlogit.db-wal` should be gitignored. It is a SQLite
write-ahead log — an ephemeral cache artifact. It was accidentally committed
to the repo.

## Long-Term Fix

Add `.backlogit/*.db-wal` and `.backlogit/*.db-shm` to `.gitignore`. Remove
these files from git tracking:

```bash
git rm --cached .backlogit/backlogit.db-wal .backlogit/backlogit.db-shm
# Add to .gitignore, then commit
```

This prevents the conflict from recurring.

## Evidence

- Post-PR #6 merge local sync, 2026-04-29
- `git update-index --cacheinfo` resolved the block
- Stale `index.lock` also needed manual removal after interrupted git process
