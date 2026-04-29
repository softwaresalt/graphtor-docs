---
description: "Backlog tool integration instructions — teaches agents how to interact with the installed backlog management tool using abstracted operations"
applyTo: '**'
---

# Backlog Integration Instructions

This workspace uses **backlogit** for structured backlog management. All agents MUST use the backlog tool for task tracking rather than creating ad-hoc markdown files or static task lists.

## Tool Configuration

| Setting | Value |
|---------|-------|
| Tool | backlogit |
| Directory | `.backlogit/` |
| Access | MCP |
| Registry | `.autoharness/backlog-registry.yaml` |

## Operation Reference

Use these operations for all backlog interactions. The operation names are abstract — the actual tool names and parameters are mapped through the backlog registry.

### Core Operations (All Tools)

| Operation | MCP Tool | CLI Command | Purpose |
|-----------|----------|-------------|---------|
| Create task | `backlogit_create_item` | `backlogit create` | Create a new task/artifact |
| List tasks | `backlogit_list_items` | `backlogit list` | List tasks with filters |
| Get task | `backlogit_get_item` | `backlogit get` | Retrieve task details |
| Update task | `backlogit_update_item` | `backlogit update` | Modify task fields |
| Move task | `backlogit_move_item` | `backlogit move` | Change task status |
| Search | `backlogit_search_items` | `backlogit search` | Full-text search |
| Complete | `backlogit_archive_item` | `backlogit archive` | Mark task complete |

### Status Values

| Abstract Status | Tool-Specific Value |
|----------------|---------------------|
| Queued | `queued` |
| Active | `active` |
| Done | `done` |
| Blocked | `blocked` |

### Extended Operations (Tool-Dependent)

| Query (SQL) | `backlogit_query_sql` | `backlogit query` | Read-only SQL against index |
| Queue view | `backlogit_get_queue` | `backlogit queue` | Prioritized ready-work list |
| Add dependency | `backlogit_add_dependency` | — | Wire task ordering |
| Remove dependency | `backlogit_remove_dependency` | — | Remove task ordering |
| Get dependencies | `backlogit_get_dependencies` | — | Inspect dependency graph |
| Append comment | `backlogit_append_comment` | — | Add execution note |
| Track commit | `backlogit_track_commit` | — | Associate commit with task |
| Save memory | `backlogit_save_memory` | — | Persist agent continuity state |
| Sync index | `backlogit_sync_index` | `backlogit sync` | Refresh query cache |
| Merge sync | `backlogit_merge_sync` | — | Incremental sync |
| Stash | `backlogit_stash` | `backlogit stash` | Defer work item |
| Fetch stash | `backlogit_fetch_stash` | — | List stash entries |
| Harvest stash | `backlogit_harvest_stash` | — | Promote stash to work item |
| Create shipment | `backlogit_create_shipment` | — | Create release unit |
| Get shipment | `backlogit_get_shipment` | — | Retrieve shipment details |
| Ship shipment | `backlogit_ship_shipment` | — | Close and archive shipment |
| Doctor | `backlogit_doctor` | `backlogit doctor` | Structural integrity check |
| Poll hooks | `backlogit_poll_hook_events` | — | Check priority signals |

## Agent Workflow Patterns

### Creating a Task

```text
Call backlogit_create_item with:
  title: "Task title"
  artifact_type: "task"
  status: "queued"
  description: "Task description"
  parent_id: "parent-task-id"  (if applicable)
  labels: "label1,label2"      (if applicable)
```

### Claiming a Task (Status → Active)

```text
Call backlogit_move_item with:
  id: "task-id"
  status: "active"
```

### Completing a Task

```text
Call backlogit_archive_item with:
  id: "task-id"
```

### Listing Ready Tasks

```text
Call backlogit_list_items with:
  status: "queued"
```

### Adding a Label

```text
Call backlogit_update_item with:
  id: "task-id"
  labels: "existing-label,harness-ready"
```

## Advanced Patterns When Supported

If the registry advertises advanced features, prefer them over ad hoc workarounds:

* **Token-efficient lookup** — use the query operation when `features.sql_query` is true
* **Ready-work selection** — use queue-aware operations when `features.queue` is true
* **Dependency reasoning** — use dependency operations when `features.dependencies` is true
* **Agent continuity** — use memory and checkpoint operations when `features.memory` or `features.checkpoints` are true
* **Traceability** — use comment or commit-tracking operations when `features.comments` or `features.commit_tracking` are true
* **Index freshness** — use sync / rehydration operations when the workspace was edited outside normal mutation tools

If a tool-specific overlay instruction file is installed (for example,
`.github/instructions/backlogit.instructions.md`), follow it in addition to this generic guide.

## Rules

1. **Always use the backlog tool** for task management. Do not create markdown task files outside the `.backlogit/` directory.
2. **Use abstract status values** mapped through the registry, not hardcoded strings.
3. **Check the registry** (`.autoharness/backlog-registry.yaml`) for the exact field names and operation parameters when unsure.
4. **Prefer MCP tools** over CLI when both are available — MCP returns structured JSON, CLI returns human-readable text.
5. **Feature gating**: Before calling an extended operation, verify the feature is supported by checking the `features` section in the registry.

Generated by autoharness | Template: backlog-integration.instructions.md.tmpl
