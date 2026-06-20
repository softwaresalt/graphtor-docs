# Compound Learning: backlogit Level-1 ID Collisions Across Parent Types

**Category:** Workflow / Tooling
**Discovered:** 2026-06-19
**Context:** Shipping shipment 043-S — chore `035-C` collided with archived feature `035-F`

## Problem

backlogit assigns the numeric prefix `{NNN}` for a new level-1 item as
`max(NNN among items of the SAME artifact_type) + 1`. That per-type counter
collides with the way the `{NNN}` namespace is shared.

In `.backlogit/config.yaml`, `queue_layout` places five types at level 1:
`feature`, `deliberation`, `shipment`, `spike`, `chore`. They share one `{NNN}`
namespace — `001-F`, `001-S`, and `001-DL` coexist by design.

Sharing is harmless for types with no children. `deliberation` and `shipment`
both declare `allowed_children: []`, so `001-S` reusing the number `001` never
produces a colliding child ID.

It is **not** harmless for `feature` and `chore`. Both declare
`allowed_children: [task, review]` and spawn hierarchical children of the form
`{NNN}.{nnn}-T`. When a chore is created while `max(chore) < max(feature)`, the
per-type counter hands it an `{NNN}` a feature already owns, and every task
child collides.

### Concrete failure

Only two chores existed: `034-C` and `035-C`. The new chore was assigned
`035 = max(chore 034) + 1`, colliding with the existing feature `035-F`. The
correct shared next number was `043` (`max(feature/chore/spike) = 042`). The
collision propagated to the children:

- `queue/035.001-T.md`, `queue/035.002-T.md` (under active chore `035-C`)
- `archive/035.001-T.md`, `archive/035.002-T.md` (under archived feature `035-F`)

> [!WARNING]
> With duplicate IDs present, bare-ID CLI calls resolve ambiguously. During the
> 043-S session, `backlogit update 035.001-T` resolved to the **archive** copy
> and silently corrupted the archived `035-F` record. It was caught and reverted
> with `git restore .backlogit/archive/` (P-007), after which task status was
> tracked via direct queue-file edits.

## Detection

Run this scan as a harvest preflight and again before `backlogit shipment ship`:

```sql
SELECT hierarchy_path, COUNT(*) AS n, GROUP_CONCAT(id) AS ids
FROM items
WHERE level = 1 AND artifact_type IN ('feature', 'chore', 'spike')
GROUP BY hierarchy_path
HAVING COUNT(*) > 1;
```

A non-empty result means two task-spawning parents share a number. The index
includes archived items, so this catches queue-vs-archive collisions.

## Prevention

1. **Prefer `feature` for level-1 work that spawns tasks.** Reserve `chore` and
   `spike` for childless items where practical. This keeps task-spawning parents
   in a single counter and avoids cross-type overlap.
2. **Verify NNN uniqueness at creation.** After creating any task-spawning
   parent, run the detection query. The correct next number is
   `max(NNN across feature + chore + spike in queue AND archive) + 1`, not the
   per-type max backlogit uses.
3. **IDs are immutable** (`header-def.yaml`: `id.immutable: true`). A
   mis-numbered parent cannot be renamed in place. To fix a live collision,
   archive or delete the mis-numbered item and recreate it — with its children —
   at the correct shared number.
4. **Never use bare-ID mutations while a collision exists.** `backlogit update`,
   `move`, `archive`, and `ship` resolve a bare `{NNN}.{nnn}-T` ambiguously.
   Operate via full file paths until the duplicate is removed.
5. **Gate `shipment ship` on a clean scan.** Ship must not run
   `backlogit shipment ship` while any manifest item's NNN collides with an
   archived twin — closure will clobber the archive record.

> [!NOTE]
> Merging a **code** PR remains safe even with a collision present: `queue/` and
> `archive/` are separate directories and a code PR only lands source files. The
> hazard is confined to backlogit CLI operations that resolve bare IDs.

## Evidence

- Shipment 043-S session, 2026-06-19; PR #71.
- Collision scan returned exactly one row: `035 → 035-C, 035-F`.
- Only two chores exist (`034-C`, `035-C`); `max(feature/chore/spike) = 42`.
- `queue/035.001-T.md` and `archive/035.001-T.md` both present.
- `.backlogit/config.yaml` — `queue_layout` level-1 types include `feature` and
  `chore`, both with `allowed_children: [task, review]`.
- `docs/memory/2026-06-19-ship-043-S-audit-advisory-suppression.md`
