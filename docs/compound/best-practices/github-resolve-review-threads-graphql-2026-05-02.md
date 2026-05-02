---
title: "Resolving GitHub PR review threads requires GraphQL — no REST endpoint exists"
tags: [github, graphql, pr-review, ci]
date: 2026-05-02
---

## Problem

After addressing Copilot review comments on a PR, threads must be marked as
resolved so the review is considered handled. The GitHub REST API has no
endpoint for resolving review threads.

## Solution

Use the GitHub GraphQL API via `gh api graphql`:

### Step 1 — Get thread node IDs

```bash
gh api graphql -f query='
  query GetThreads($owner: String!, $repo: String!, $pr: Int!) {
    repository(owner: $owner, name: $repo) {
      pullRequest(number: $pr) {
        reviewThreads(first: 100) {
          nodes {
            id
            isResolved
            comments(first: 1) {
              nodes { body path line }
            }
          }
        }
      }
    }
  }
' -f owner="softwaresalt" -f repo="graphtor-docs" -F pr=19
```

### Step 2 — Resolve each thread

```bash
gh api graphql -f query='
  mutation ResolveThread($threadId: ID!) {
    resolveReviewThread(input: { threadId: $threadId }) {
      thread { isResolved }
    }
  }
' -f threadId="<thread_node_id>"
```

Match threads to comments by `path` and `line`. Resolve only bot-authored
threads (Copilot, linters) — never auto-resolve human reviewer threads.

### Replying to a comment before resolving

```bash
gh api repos/{owner}/{repo}/pulls/{pr}/comments/{comment_id}/replies \
  -X POST -f body="Fixed in <sha>. <brief description>"
```

Always reply before resolving so the thread has a clear audit trail.

## Evidence

PR #19 Copilot review remediation, 2026-05-02. All 4 threads replied to and
resolved via GraphQL.
