# Compound Learning: backlogit Shipment State Machine Transitions

**Category:** Workflow / Tooling  
**Discovered:** 2026-04-29  
**Context:** Shipping shipment 002-S

## Problem

`backlogit shipment ship <id>` requires the shipment to be in `released`
status. There is no direct transition from `active` to `released`. Attempting
`backlogit move <id> released` from `active` fails.

## Solution

To close a shipment from `active`:

```bash
# Step 1: Move active → done
backlogit update <shipment-id> --status done

# Step 2: Archive it (this is the "shipped" action)
backlogit archive <shipment-id>
```

The archived shipment lands in `.backlogit/archive/`.

## State Machine Summary

Valid transitions for shipments (observed in practice):

```
queued → active → done → (archive)
```

The `released` status exists in the schema but is not reachable from `active`
through normal CLI commands. Do not attempt `backlogit move <id> released`.

## Task Status Transitions

For features and tasks, valid transitions follow the same pattern:

```
queued → active → done → (archive)
```

**Important:** You cannot skip states. `backlogit move <id> done` fails if the
item is in `queued`. You must go through `active` first.

```bash
# Wrong (fails from queued):
backlogit move task-id done

# Correct:
backlogit move task-id active  # then
backlogit move task-id done    # or backlogit update task-id --status done
```

## Evidence

- Session execution of 002-S, 2026-04-29
- Shipment archived successfully via `update --status done` + `archive`
