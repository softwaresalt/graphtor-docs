//! Integration tests: MCP tool manifest — tool list completeness and structure.
//!
//! Verifies that [`graphtor_core::mcp::list_mcp_tools`] returns exactly the
//! expected set of tools with non-empty descriptions, and that the list is
//! sorted deterministically.  These tests act as a contract guard: any change
//! to the registered tool set will cause a test failure and require an
//! intentional update here.

use graphtor_core::mcp::list_mcp_tools;

/// The full set of tool names advertised by the MCP server.
///
/// Must be kept in sync with the tools registered in `src/mcp/server.rs`.
const EXPECTED_TOOLS: &[&str] = &[
    "get_chunk_by_id",
    "get_document",
    "get_status",
    "list_sources",
    "research_topic",
    "search_local_docs",
    "search_semantic",
    "traverse_doc_links",
];

// ── T017.001: manifest returns exactly the expected tool set ─────────────────

#[test]
fn list_mcp_tools_returns_expected_tool_count() {
    let tools = list_mcp_tools();
    assert_eq!(
        tools.len(),
        EXPECTED_TOOLS.len(),
        "expected {} tools, got {}; tool names: {:?}",
        EXPECTED_TOOLS.len(),
        tools.len(),
        tools.iter().map(|t| t.name.as_ref()).collect::<Vec<_>>()
    );
}

// ── T017.002: every expected tool name is present ────────────────────────────

#[test]
fn list_mcp_tools_contains_all_expected_names() {
    let tools = list_mcp_tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    for expected in EXPECTED_TOOLS {
        assert!(
            names.contains(expected),
            "expected tool '{expected}' not found in manifest; available: {names:?}"
        );
    }
}

// ── T017.003: no unexpected tools are present ─────────────────────────────────

#[test]
fn list_mcp_tools_contains_no_unexpected_names() {
    let tools = list_mcp_tools();
    for tool in &tools {
        let name = tool.name.as_ref();
        assert!(
            EXPECTED_TOOLS.contains(&name),
            "unexpected tool '{name}' found in manifest; update EXPECTED_TOOLS if intentional"
        );
    }
}

// ── T017.004: all tools have non-empty descriptions ──────────────────────────

#[test]
fn list_mcp_tools_all_have_descriptions() {
    let tools = list_mcp_tools();
    for tool in &tools {
        let desc = tool.description.as_deref().unwrap_or("");
        assert!(
            !desc.trim().is_empty(),
            "tool '{}' has an empty description",
            tool.name
        );
    }
}

// ── T017.005: tool list is sorted by name ─────────────────────────────────────

#[test]
fn list_mcp_tools_is_sorted_by_name() {
    let tools = list_mcp_tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(
        names, sorted,
        "tool list must be sorted alphabetically by name for deterministic output"
    );
}

// ── T017.006: CLI --json manifest parity ─────────────────────────────────────
//
// Verifies that the tool names returned by `list_mcp_tools()` (used by the
// `manifest --json` CLI command) are identical to the alphabetically-sorted
// `EXPECTED_TOOLS` constant, establishing a stable contract between the CLI
// manifest output and the live MCP `tools/list` response.

#[test]
fn list_mcp_tools_matches_expected_sorted_names() {
    let tools = list_mcp_tools();
    let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();

    let mut expected_sorted = EXPECTED_TOOLS.to_vec();
    expected_sorted.sort_unstable();

    assert_eq!(
        names, expected_sorted,
        "tool names don't match expected sorted list"
    );
}
