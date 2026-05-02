---
title: "backlogit status transitions: queued → done requires intermediate active state"
tags: [backlogit, workflow, status]
date: 2026-05-02
---

## Problem

Attempting to move a backlogit item directly from `queued` to `done` via
`backlogit_move_item` (MCP) or `backlogit move` (CLI) is rejected with a
status conflict error. This happens even when the work is genuinely complete
and the intermediate `active` state was never set.

## Context

Stale features (019-F, 020-F) had all their tasks completed but were still in
`queued` status after shipment 011-S was closed. Attempting a direct
`queued → done` transition failed.

## Solution

Always transition through `active` first:

```text
backlogit_move_item(id, status="active")
backlogit_move_item(id, status="done")
```

or with CLI:

```bash
backlogit move 019-F --status active
backlogit move 019-F --status done
```

## Why

backlogit enforces a linear state machine: `queued → active → done | blocked`.
Skipping states violates the FSM constraints and is rejected server-side.

## Evidence

Session handling of 019-F and 020-F cleanup, 2026-05-02.
