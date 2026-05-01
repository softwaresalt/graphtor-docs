//! Markdown response formatting helpers for MCP tool outputs.
//!
//! Converts typed result structs from [`crate::db::search`] and
//! [`crate::db::traverse`] into structured markdown strings suitable
//! for direct LLM consumption.
//!
//! # Placement decision (013.006-T)
//!
//! This module lives under `src/mcp/` because its sole current consumers are
//! the MCP tool implementations in [`crate::mcp::server`].  The formatted
//! types (`SearchResult`, `TraversalResult`) originate in `src/db/` and carry
//! no MCP-protocol-specific logic, so the module *could* be promoted to
//! `src/format.rs` if a non-MCP surface (e.g., the `status` command or a
//! future HTTP API) needs the same output.  Until such a consumer exists,
//! keeping the module co-located with its only caller avoids premature
//! abstraction.

use std::fmt::Write as _;

use crate::db::{search::SearchResult, traverse::TraversalResult};

/// Format a slice of [`SearchResult`]s as markdown for LLM consumption.
///
/// Produces a numbered list of results with source path, heading
/// hierarchy, and a fenced code block containing the chunk content.
/// Returns `"No results found."` when the slice is empty.
#[must_use]
pub fn format_search_results(results: &[SearchResult]) -> String {
    if results.is_empty() {
        return "No results found.".to_string();
    }
    let mut out = String::from("## Search Results\n\n");
    for (i, r) in results.iter().enumerate() {
        let heading = if r.heading_hierarchy.is_empty() {
            String::new()
        } else {
            format!("\n**Headings:** {}", r.heading_hierarchy.join(" › "))
        };
        write!(
            out,
            "### Result {}\n\n**Chunk ID:** `{}`\n**Source:** `{}`{}\n\n```\n{}\n```\n\n",
            i + 1,
            r.chunk_id,
            r.path,
            heading,
            r.content.trim(),
        )
        .expect("write to String is infallible");
    }
    out
}

/// Format a slice of [`TraversalResult`]s as markdown for LLM consumption.
///
/// Produces a depth-annotated list of related chunks discovered via BFS.
/// Returns a no-results message when the slice is empty.
#[must_use]
pub fn format_traversal_results(start_id: &str, results: &[TraversalResult]) -> String {
    if results.is_empty() {
        return format!("No related chunks found from chunk `{start_id}`.");
    }
    let mut out = format!("## Related Chunks\n\nStarting from chunk `{start_id}`:\n\n");
    for r in results {
        writeln!(
            out,
            "- **Depth {depth}** — `{path}` (chunk ID: `{chunk_id}`)",
            depth = r.depth,
            path = r.path,
            chunk_id = r.chunk_id,
        )
        .expect("write to String is infallible");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{search::SearchResult, traverse::TraversalResult};

    #[test]
    fn empty_search_returns_no_results_message() {
        assert_eq!(format_search_results(&[]), "No results found.");
    }

    #[test]
    fn empty_traversal_returns_no_related_message() {
        let msg = format_traversal_results("abc123", &[]);
        assert!(msg.contains("abc123"));
        assert!(msg.contains("No related chunks"));
    }

    #[test]
    fn search_result_contains_path_and_content() {
        let result = SearchResult {
            chunk_id: "c1".to_string(),
            path: "docs/api.md".to_string(),
            heading_hierarchy: vec!["API Reference".to_string()],
            content: "This is the content.".to_string(),
        };
        let md = format_search_results(&[result]);
        assert!(md.contains("c1")); // chunk_id should appear in output
        assert!(md.contains("docs/api.md"));
        assert!(md.contains("API Reference"));
        assert!(md.contains("This is the content."));
    }

    #[test]
    fn traversal_result_contains_depth_and_path() {
        let result = TraversalResult {
            chunk_id: "c2".to_string(),
            path: "docs/guide.md".to_string(),
            depth: 1,
        };
        let md = format_traversal_results("seed-chunk", &[result]);
        assert!(md.contains("seed-chunk"));
        assert!(md.contains("Depth 1"));
        assert!(md.contains("docs/guide.md"));
    }
}
