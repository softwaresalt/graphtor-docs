---
title: "GitHub PR review threads: GraphQL vs REST IDs for inline replies"
date: 2026-05-05
tags: [github, pr-automation, graphql, rest]
---

## Problem

When addressing Copilot pull-request review comments on a PR, replies must be
posted to the correct endpoint using the **numeric REST comment ID**, not the
GraphQL global node ID (`PRRC_kwDO...`).

The `GET /repos/{owner}/{repo}/pulls/{pr}/comments` REST endpoint returns
comments only when filtered by the correct `user.login` — for the Copilot
review bot, filter on `"Copilot"` (not `"copilot-pull-request-reviewer[bot]"`,
which returns 0 results from the `reviews` endpoint).

## Solution

```powershell
# Get numeric comment IDs for the third+ Copilot review round
gh api "repos/{owner}/{repo}/pulls/{pr}/comments" \
  --paginate \
  --jq '.[] | select(.user.login == "Copilot") | {id, created_at, body: .body[0:80]}'
```

Then reply using the numeric `id`:

```powershell
gh api -X POST "repos/{owner}/{repo}/pulls/{pr}/comments/{numeric_id}/replies" \
  --field body="Fixed in {sha}. ..."
```

## Key Facts

- Copilot review comments appear under `user.login: "Copilot"` in the pulls
  comments REST API, not under `"copilot-pull-request-reviewer[bot]"`.
- Copilot reviews may not appear in the REST `pulls/{pr}/reviews` endpoint
  immediately after the GitHub Actions workflow completes — poll the review
  threads via GraphQL instead.
- Thread resolution uses the GraphQL `resolveReviewThread` mutation with the
  `PRRT_kwDO...` node ID (not the comment ID).
- `POST /pulls/{pr}/comments/{id}/replies` requires the parent's numeric ID.
