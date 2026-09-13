# Compound Learning: backlogit Shipment State Machine Transitions

**Category:** Workflow / Tooling  
**Discovered:** 2026-04-29  
**Context:** Shipping shipment 002-S

> **SUPERSEDED (2026-09-13):** The shipment-lifecycle guidance below
> (`queued -> active -> done -> (archive)` via
> `backlogit update <id> --status done` + `backlogit archive`) does **not**
> match the backlogit version now installed in this workspace
> (`1.10.1-0.20260823032255-b07729386a31+dirty`), empirically confirmed
> during shipment `049-S` closure. See
> `docs/compound/2026-05-07-backlogit-shipment-status-constraints.md` for
> the current, evidence-backed shipment status enum
> (`queued -> active -> shipped | abandoned`, no `done`, no `blocked`) and
> the confirmed requirement that only the `backlogit shipment ship`
> cascade operation (not `move`/`update`) can reach `shipped`. The
> **Task Status Transitions** section below remains accurate for task
> artifacts and is unaffected by this supersession.

## Problem

`backlogit shipment ship <id>` requires the shipment to be in `released`
status. There is no direct transition from `active` to `released`. Attempting
`backlogit move <id> released` from `active` fails.

## Solution (superseded for shipments — see notice above)

To close a shipment from `active`:

```bash
# Step 1: Move active → done
backlogit update <shipment-id> --status done

# Step 2: Archive it (this is the "shipped" action)
backlogit archive <shipment-id>
```

The archived shipment lands in `.backlogit/archive/`.

## State Machine Summary (superseded for shipments — see notice above)

Valid transitions for shipments (observed in practice on the 2026-04-29
backlogit version):

```
queued → active → done → (archive)
```

The `released` status exists in the schema but is not reachable from `active`
through normal CLI commands. Do not attempt `backlogit move <id> released`.
As of the 1.10.1 version confirmed 2026-09-13, the terminal shipment status
is `shipped` (or `abandoned`), reached **only** via the `backlogit shipment
ship` cascade operation. The non-cascading `backlogit move <id> --status
shipped` command — including the one `shipment-reconcile`'s safe-close mode
itself attempts as its first step — is unconditionally refused by the
installed version (error code `shipment_shipped_requires_envelope`); it
never sets a shipment record's live status to `shipped`. Do not read
"safe-close" as a second way to reach `shipped` directly: see the
superseding entry cited above for the full empirical evidence trail
(`docs/compound/2026-05-07-backlogit-shipment-status-constraints.md`).

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
