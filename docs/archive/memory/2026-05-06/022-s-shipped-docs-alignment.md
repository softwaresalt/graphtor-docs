---
type: session-memory
timestamp: 2026-05-06T00:00:00Z
agent: Ship
shipment: 022-S
feature: 031-F
pr: "https://github.com/softwaresalt/graphtor-docs/pull/37"
status: shipped
---

# Session Memory: 022-S Docs Alignment Shipped

---

## What Shipped

Four documentation files updated to reflect features delivered in prior cycles
(019-S rebranding, 020-S zero-config, 021-S JSON-RPC/manifest):

| File | Changes |
|---|---|
| `docs/cli-reference/graphtor-docs.md` | Added `--json` global flag, `manifest` subcommand, corrected `status --json` example to JSON-RPC envelope, clarified manifest ordering |
| `docs/mcp-tools.md` | Updated 7→8 tool count, added full `research_topic` section with correct formatter output, noted composite workflow role |
| `README.md` | Updated 7→8 in features list + docs table, added `--json` bullet, added `manifest` bullet, clarified manifest ordering |
| `AGENTS.md` | Removed "LocalDocRAG" header, updated DatabaseError to CozoDB, fixed db module list (added `vectors.rs`), updated date |

---

## 9 Copilot Review Findings — What Was Caught

**Round 1 (5 findings):**

1. `status --json` example showed raw JSON — should be JSON-RPC 2.0 envelope `{"jsonrpc":"2.0","id":null,"result":{...}}`
2. `research_topic` response section headings were wrong — actual formatter emits `### Search Results` and `### Related Context`
3. Related chunks description implied full content — actual format is only `- **Depth N** — path (chunk ID: ...)` bullets
4. AGENTS.md db module list omitted `vectors.rs` — file exists at `src/db/vectors.rs`
5. PR description didn't mention exec-plan and backlogit artifacts

**Round 2 (4 findings):**

6. `manifest` described as "identical" to live server — clarified: CLI sorts alphabetically, server is unordered
7. `research_topic` said "top 3 seeds" — corrected to `min(top_k, 3)` (so top_k=1 uses 1 seed)
8. Related Context format description was vague — updated to show exact bullet format `- **Depth N** — \`path\` (chunk ID: \`...\`)`
9. (Duplicate of #8 in different file) — fixed same wording in README

---

## Key Source-of-Truth Files (for future doc updates)

| What to document | Where to look |
|---|---|
| `research_topic` formatter output | `src/mcp/format.rs` lines 162-212 (`format_research_results`) |
| `research_topic` seed count | `src/mcp/server.rs` lines 456-457 (`seed_k = search_k.min(3)`) |
| `status --json` envelope | `src/cli/jsonrpc.rs` — `wrap_success()` function |
| `manifest` tool list ordering | `src/cli/mod.rs` — Manifest subcommand handler (sorted alphabetically) |
| MCP tool count | `src/mcp/server.rs` — count tool registrations |
| db module list | `src/db/` directory listing |

---

## Compound Learning Written

`docs/compound/mcp-formatter-source-verification-2026-05-06.md` — pattern for
verifying formatter output before writing documentation; includes lookup commands.

---

## Commits

1. `fcba2cd` — docs(docs): add --json global flag and manifest subcommand to CLI reference  
2. `61bbfc7` — docs(docs): add research_topic tool, fix MCP tool count 7->8  
3. `c9ca508` — docs(docs): update README features list and MCP tool count  
4. `687691f` — docs(docs): fix stale architecture references in AGENTS.md  
5. `807a2be` — chore(build): add backlog artifacts and exec plan for 022-S  
6. `a2bca36` — fix(docs): address Copilot review findings on PR #37 (round 1)  
7. `def59c4` — fix(docs): address second Copilot review findings on PR #37 (round 2)  
