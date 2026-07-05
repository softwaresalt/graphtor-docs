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

use crate::db::{
    search::SearchResult, store::DbStatus, traverse::TraversalResult, ChunkRecord, SourceRecord,
};

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

/// Render a cross-source annotation for a traversal result, or an empty string
/// for an intra-source result.
fn cross_source_marker(r: &TraversalResult) -> String {
    if r.cross_source {
        format!(" _(cross-source: `{}`)_", r.source_id)
    } else {
        String::new()
    }
}

/// Format a slice of [`TraversalResult`]s as markdown for LLM consumption.
///
/// Produces a depth-annotated list of related chunks discovered via BFS.
/// Cross-source hops are annotated with the target source. Returns a
/// no-results message when the slice is empty.
#[must_use]
pub fn format_traversal_results(start_id: &str, results: &[TraversalResult]) -> String {
    if results.is_empty() {
        return format!("No related chunks found from chunk `{start_id}`.");
    }
    let mut out = format!("## Related Chunks\n\nStarting from chunk `{start_id}`:\n\n");
    for r in results {
        let marker = cross_source_marker(r);
        writeln!(
            out,
            "- **Depth {depth}** — `{path}` (chunk ID: `{chunk_id}`){marker}",
            depth = r.depth,
            path = r.path,
            chunk_id = r.chunk_id,
        )
        .expect("write to String is infallible");
    }
    out
}

/// Format a slice of [`SourceRecord`]s as markdown for LLM consumption.
///
/// Produces a bulleted list of registered sources with ID, kind, name, and
/// last-sync timestamp.  Returns a no-sources message when the slice is empty.
#[must_use]
pub fn format_sources_list(sources: &[SourceRecord]) -> String {
    if sources.is_empty() {
        return "No documentation sources registered.".to_string();
    }
    let mut out = format!(
        "## Documentation Sources ({} registered)\n\n",
        sources.len()
    );
    for src in sources {
        let synced = src.synced_at.as_deref().unwrap_or("never");
        writeln!(
            out,
            "- **{}** (`{}`) — kind: `{}`, last synced: `{}`",
            src.name, src.source_id, src.kind, synced,
        )
        .expect("write to String is infallible");
    }
    out
}

/// Format a single [`ChunkRecord`] as markdown for LLM consumption.
///
/// Produces a labelled block with chunk ID, source, path, heading hierarchy,
/// position, and a fenced content block.
#[must_use]
pub fn format_chunk(chunk: &ChunkRecord) -> String {
    let heading = if chunk.heading_hierarchy.is_empty() {
        String::new()
    } else {
        format!("\n**Headings:** {}", chunk.heading_hierarchy.join(" › "))
    };
    format!(
        "## Chunk `{}`\n\n**Source:** `{}`\n**Path:** `{}`{}\n**Position:** {}\n\n```\n{}\n```\n",
        chunk.chunk_id,
        chunk.source_id,
        chunk.path,
        heading,
        chunk.position,
        chunk.content.trim(),
    )
}

/// Format all chunks of a document as markdown for LLM consumption.
///
/// Produces a titled section per chunk with position index, chunk ID, heading
/// context, and a fenced content block.  Returns a no-chunks message when
/// the slice is empty.
#[must_use]
pub fn format_document(path: &str, chunks: &[ChunkRecord]) -> String {
    if chunks.is_empty() {
        return format!("No chunks found for document `{path}`.");
    }
    let mut out = format!("## Document: `{path}`\n\n**Chunks:** {}\n\n", chunks.len());
    for chunk in chunks {
        let heading = if chunk.heading_hierarchy.is_empty() {
            String::new()
        } else {
            format!(" — {}", chunk.heading_hierarchy.join(" › "))
        };
        write!(
            out,
            "### Chunk {} (ID: `{}`){}\n\n```\n{}\n```\n\n",
            chunk.position + 1,
            chunk.chunk_id,
            heading,
            chunk.content.trim(),
        )
        .expect("write to String is infallible");
    }
    out
}

/// Format composite research results as markdown for LLM consumption.
///
/// Combines an initial set of search results (from keyword or semantic search)
/// with related chunks discovered via BFS graph traversal.  The related section
/// is unified and deduplicated across all seeds before formatting — each chunk
/// appears at most once.
///
/// Returns a two-section markdown document: `### Search Results` listing the
/// initial matches, and `### Related Context` listing the BFS-discovered chunks.
#[must_use]
pub fn format_research_results(
    query: &str,
    initial: &[SearchResult],
    related: &[TraversalResult],
) -> String {
    let display_query = query.trim().replace('\n', " ").replace('\r', "");
    let mut out = format!("## Research: {display_query}\n\n");

    out.push_str("### Search Results\n\n");
    if initial.is_empty() {
        out.push_str("No results found.\n\n");
    } else {
        for (i, r) in initial.iter().enumerate() {
            let heading = if r.heading_hierarchy.is_empty() {
                String::new()
            } else {
                format!("\n**Headings:** {}", r.heading_hierarchy.join(" › "))
            };
            write!(
                out,
                "#### Result {}\n\n**Chunk ID:** `{}`\n**Source:** `{}`{}\n\n```\n{}\n```\n\n",
                i + 1,
                r.chunk_id,
                r.path,
                heading,
                r.content.trim(),
            )
            .expect("write to String is infallible");
        }
    }

    if !related.is_empty() {
        out.push_str("### Related Context\n\n");
        for tr in related {
            let marker = cross_source_marker(tr);
            writeln!(
                out,
                "- **Depth {depth}** — `{path}` (chunk ID: `{chunk_id}`){marker}",
                depth = tr.depth,
                path = tr.path,
                chunk_id = tr.chunk_id,
            )
            .expect("write to String is infallible");
        }
        out.push('\n');
    }

    out
}

/// Format a [`DbStatus`] snapshot as markdown for LLM consumption.
///
/// Produces a concise status report with source count, chunk count, and
/// current schema version.
#[must_use]
pub fn format_db_status(status: &DbStatus) -> String {
    format!(
        "## Database Status\n\n\
         - **Sources:** {}\n\
         - **Chunks:** {}\n\
         - **Schema version:** {}\n",
        status.source_count, status.chunk_count, status.schema_version,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{search::SearchResult, store::DbStatus, traverse::TraversalResult};

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
            source_id: "test-source".to_string(),
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
            source_id: "src".to_string(),
            path: "docs/guide.md".to_string(),
            depth: 1,
            cross_source: false,
        };
        let md = format_traversal_results("seed-chunk", &[result]);
        assert!(md.contains("seed-chunk"));
        assert!(md.contains("Depth 1"));
        assert!(md.contains("docs/guide.md"));
    }

    // ── format_sources_list ───────────────────────────────────────────────────

    #[test]
    fn format_sources_list_empty_returns_no_sources_message() {
        assert_eq!(
            format_sources_list(&[]),
            "No documentation sources registered."
        );
    }

    #[test]
    fn format_sources_list_contains_source_id_and_name() {
        let src = SourceRecord {
            source_id: "ms-azure".to_string(),
            url: "https://github.com/MicrosoftDocs/azure-docs".to_string(),
            kind: "git".to_string(),
            name: "Azure Docs".to_string(),
            synced_at: Some("2024-01-15T12:00:00Z".to_string()),
        };
        let md = format_sources_list(&[src]);
        assert!(md.contains("ms-azure"));
        assert!(md.contains("Azure Docs"));
        assert!(md.contains("git"));
        assert!(md.contains("2024-01-15T12:00:00Z"));
    }

    #[test]
    fn format_sources_list_uses_never_when_no_sync_timestamp() {
        let src = SourceRecord {
            source_id: "src-001".to_string(),
            url: "https://example.com".to_string(),
            kind: "local".to_string(),
            name: "Local Docs".to_string(),
            synced_at: None,
        };
        let md = format_sources_list(&[src]);
        assert!(md.contains("never"));
    }

    // ── format_chunk ──────────────────────────────────────────────────────────

    #[test]
    fn format_chunk_contains_all_fields() {
        let chunk = ChunkRecord {
            chunk_id: "abc123".to_string(),
            source_id: "src-001".to_string(),
            path: "docs/guide.md".to_string(),
            title: None,
            position: 3,
            char_offset: 300,
            heading_hierarchy: vec!["Guide".to_string(), "Installation".to_string()],
            content: "Install the package with cargo.".to_string(),
        };
        let md = format_chunk(&chunk);
        assert!(md.contains("abc123"));
        assert!(md.contains("src-001"));
        assert!(md.contains("docs/guide.md"));
        assert!(md.contains("Installation"));
        assert!(md.contains("Install the package with cargo."));
        assert!(md.contains('3')); // position
    }

    #[test]
    fn format_chunk_without_headings_omits_heading_line() {
        let chunk = ChunkRecord {
            chunk_id: "no-heading".to_string(),
            source_id: "src".to_string(),
            path: "readme.md".to_string(),
            title: None,
            position: 0,
            char_offset: 0,
            heading_hierarchy: vec![],
            content: "Plain content.".to_string(),
        };
        let md = format_chunk(&chunk);
        assert!(
            !md.contains("Headings:"),
            "should not include Headings line"
        );
        assert!(md.contains("Plain content."));
    }

    // ── format_document ───────────────────────────────────────────────────────

    #[test]
    fn format_document_empty_returns_no_chunks_message() {
        let md = format_document("docs/missing.md", &[]);
        assert!(md.contains("docs/missing.md"));
        assert!(md.contains("No chunks found"));
    }

    #[test]
    fn format_document_contains_path_and_chunk_count() {
        let chunks = vec![
            ChunkRecord {
                chunk_id: "d-0".to_string(),
                source_id: "src".to_string(),
                path: "api.md".to_string(),
                title: None,
                position: 0,
                char_offset: 0,
                heading_hierarchy: vec![],
                content: "First chunk.".to_string(),
            },
            ChunkRecord {
                chunk_id: "d-1".to_string(),
                source_id: "src".to_string(),
                path: "api.md".to_string(),
                title: None,
                position: 1,
                char_offset: 100,
                heading_hierarchy: vec!["API".to_string()],
                content: "Second chunk.".to_string(),
            },
        ];
        let md = format_document("api.md", &chunks);
        assert!(md.contains("api.md"));
        assert!(md.contains('2')); // chunk count
        assert!(md.contains("First chunk."));
        assert!(md.contains("Second chunk."));
    }

    // ── format_research_results ───────────────────────────────────────────────

    #[test]
    fn format_research_results_empty_initial_shows_no_results() {
        let md = format_research_results("error handling", &[], &[]);
        assert!(
            md.contains("error handling"),
            "query should appear in output"
        );
        assert!(
            md.contains("No results found"),
            "should indicate no results"
        );
    }

    #[test]
    fn format_research_results_includes_chunk_id_and_path() {
        let initial = SearchResult {
            chunk_id: "abc123".to_string(),
            source_id: "src-1".to_string(),
            path: "docs/errors.md".to_string(),
            heading_hierarchy: vec!["Error Handling".to_string()],
            content: "Handle errors gracefully.".to_string(),
        };
        let md = format_research_results("errors", &[initial], &[]);
        assert!(md.contains("errors"), "query should appear in heading");
        assert!(md.contains("abc123"), "chunk id should appear");
        assert!(md.contains("docs/errors.md"), "path should appear");
        assert!(
            md.contains("Error Handling"),
            "heading hierarchy should appear"
        );
        assert!(
            md.contains("Handle errors gracefully."),
            "content should appear"
        );
    }

    #[test]
    fn format_research_results_includes_related_context() {
        let related = TraversalResult {
            chunk_id: "rel001".to_string(),
            source_id: "src".to_string(),
            path: "docs/related.md".to_string(),
            depth: 1,
            cross_source: false,
        };
        let md = format_research_results("topic", &[], &[related]);
        assert!(md.contains("rel001"), "related chunk id should appear");
        assert!(md.contains("docs/related.md"), "related path should appear");
        assert!(
            md.contains("Depth 1") || md.contains("depth 1") || md.contains('1'),
            "depth should appear"
        );
    }

    // ── format_db_status ─────────────────────────────────────────────────────

    #[test]
    fn format_db_status_contains_all_counts() {
        let status = DbStatus {
            source_count: 5,
            chunk_count: 1234,
            schema_version: 2,
        };
        let md = format_db_status(&status);
        assert!(md.contains('5'));
        assert!(md.contains("1234"));
        assert!(md.contains('2'));
        assert!(md.contains("Sources"));
        assert!(md.contains("Chunks"));
        assert!(md.contains("Schema version"));
    }
}
