# Compound Learning: Copilot Review Catches Doc-to-Code Drift on MCP Tool Formatters

**Category:** Documentation / Code Review  
**Discovered:** 2026-05-06  
**Context:** PR #37 — 022-S documentation alignment chore (2 Copilot review rounds)

## Problem

When documenting MCP tool output format from memory or from tool descriptions
alone, the actual formatter output can differ from what the author assumed. In
PR #37, two documentation gaps were caught by Copilot review:

1. The `research_topic` tool's response sections were named incorrectly
   (`"Related chunks discovered via graph traversal"` instead of
   `"### Related Context"` / `"### Search Results"`).
2. The response description implied related chunks contained full content, but
   the formatter only emits `- **Depth N** — path (chunk ID: ...)` bullets.
3. The `manifest` subcommand description said output was "identical to what the
   MCP server returns" — but the CLI sorts tools alphabetically while the live
   server is unordered.
4. "top 3 seeds" was wrong when `top_k < 3` — should be `min(top_k, 3)`.

## Solution

When documenting MCP tool output or CLI output, **always read the formatter
source directly** before writing the response description:

1. Find the formatter function: `grep -r "format_research_results\|format_" src/mcp/`
2. Read the actual string literals, section headings, and bullet formats
3. Copy the exact heading strings into the documentation
4. For compound behavior (e.g., `min(top_k, 3)`), read the implementation
   rather than paraphrasing the description

### Formatter Source Lookup Pattern

```bash
# Find formatters
Get-ChildItem src/mcp/ -Filter "*.rs" | Select-String -Pattern "fn format_" -SimpleMatch

# Read the formatter output format
view src/mcp/format.rs  # lines 162-212 for research_topic
```

### Correct Pattern (research_topic)

```markdown
**Response:** Markdown with two sections:
- `### Search Results` — initial search hits with full chunk content
- `### Related Context` — BFS-discovered related chunks as a bullet list in the format:
  `- **Depth N** — \`path\` (chunk ID: \`...\`)` (no content inline)
```

### Ordering Caveat Pattern

When CLI output is generated from a sorted list but live server is not sorted:

```markdown
Tool definitions are derived from the same source as the MCP server,
guaranteeing parity of tool names, descriptions, and parameter schemas.
Note that the tool list is sorted alphabetically for deterministic output;
ordering may differ from the live server's `tools/list` response.
```

## Evidence

- PR #37, Copilot review round 1: 5 findings — 4 code/content, 1 PR description
- PR #37, Copilot review round 2: 4 findings — all formatter/output accuracy issues
- Source of truth: `src/mcp/format.rs` lines 162-212, `src/mcp/server.rs` lines 456-457
- All 9 findings fixed across 2 review rounds; 2 commits (`a2bca36`, `def59c4`)
