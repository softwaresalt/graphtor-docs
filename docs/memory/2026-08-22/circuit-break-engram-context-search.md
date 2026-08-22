---
type: circuit-breaker
doc_type: memory
source: direct-stage-correction
timestamp: 2026-08-22T08:09:19.6105997Z
agent: GitHub Copilot CLI
skill: direct
breaker_type: universal
operation: Engram stale-text context searches
attempts: 6
---

## Failure Chain

Six independent stale-text queries were dispatched in one parallel batch with
`ENGRAM_DIRECT=1` and the invalid argument `--region context`.

### Attempts 1-6

All returned:

```text
Invalid request parameters: invalid region 'context': expected code or all
```

## Context

- Files involved: `docs/exec-plans/2026-08-21-mcp-serve-initialize-handshake-regression-plan.md`, `docs/decisions/2026-08-21-mcp-serve-initialize-os-error-232-deliberation.md`
- Engram workspace health: bound to `C:\Source\GitHub\graphtor`, fully scanned, `stale_files=false`
- Resolution: Circuit breaker triggered. Do not retry Engram search in this session and do not substitute broad grep.
- Continuation: use exact reads of known files for remaining line-level confirmation.
