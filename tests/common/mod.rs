//! Shared test helper utilities for graphtor-docs integration tests.

/// Build a docline v1 conformant markdown string suitable for use in test
/// fixtures that exercise the runtime ingestion path.
///
/// The returned string has the required `---...---` YAML frontmatter block with
/// the five mandatory fields (`title`, `source`, `ingested_at`, `doc_type`,
/// `source_path`) followed by `content`.
///
/// # Parameters
///
/// - `source_path` — the value written to `source_path:` in the frontmatter;
///   this becomes the canonical document identity used for chunk IDs and DB keys.
/// - `title` — value for the `title:` field.
/// - `content` — markdown body (appended after the closing `---`).
#[must_use]
pub fn docline_md(source_path: &str, title: &str, content: &str) -> String {
    format!(
        "---\ntitle: {title}\nsource: /test/source\ningested_at: \
         2026-01-01T00:00:00Z\ndoc_type: markdown\nsource_path: {source_path}\n---\n{content}"
    )
}

/// Build the standard docline-conformant content for a numbered document.
///
/// Convenience wrapper over [`docline_md`] for tests that create N numbered
/// documents.
#[must_use]
pub fn docline_md_numbered(n: usize, source_path_prefix: &str) -> String {
    let source_path = format!("{source_path_prefix}/doc{n:02}.md");
    let title = format!("Document {n}");
    let content = format!("# Document {n}\n\nContent of document {n}.\n");
    docline_md(&source_path, &title, &content)
}
