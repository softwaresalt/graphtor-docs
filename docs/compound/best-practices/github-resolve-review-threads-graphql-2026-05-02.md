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

Two reply paths exist. The REST path replies by numeric comment ID:

```bash
gh api repos/{owner}/{repo}/pulls/{pr}/comments/{comment_id}/replies \
  -X POST -f body="Fixed in <sha>. <brief description>"
```

The GraphQL path replies by thread node ID and is more robust when iterating
many review waves — it needs no REST comment-ID mapping and pairs cleanly with
the resolve mutation that also takes the thread ID:

```bash
gh api graphql -f query='
  mutation Reply($tid: ID!, $b: String!) {
    addPullRequestReviewThreadReply(
      input: { pullRequestReviewThreadId: $tid, body: $b }
    ) { comment { id } }
  }
' -f tid="<thread_node_id>" -f b="<reply body>"
```

Always reply before resolving so the thread has a clear audit trail.

### PowerShell quoting caveat for reply bodies

On Windows, pre-assign the thread ID and body to `$tid`/`$body` variables and
pass them via `-f "tid=$tid" -f "body=$body"`. Reply-body text must avoid
apostrophes, `$`, backticks, and double quotes, which PowerShell interprets
before `gh` sees them. Write plain prose (spell out parenthetical SHAs rather
than backtick-quoting them) to sidestep the escape conflict documented in
`gh-pr-body-powershell-backtick-conflict-2026-04-29.md`.

## Evidence

- PR #19 Copilot review remediation, 2026-05-02. All 4 threads replied to and
  resolved via GraphQL.
- PRs #90 / #91 (045-S), 2026-07-16. 14 review waves resolved using the GraphQL
  `addPullRequestReviewThreadReply` reply-by-thread-ID path plus the PowerShell
  quoting discipline above.
