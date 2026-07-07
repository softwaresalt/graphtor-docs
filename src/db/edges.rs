//! Edge and code-snippet storage operations.
//!
//! Manages two stored relations:
//!
//! - `doc_edges` — directed hyperlink edges between chunks
//! - `doc_code` — code snippets extracted from document chunks

use std::collections::{BTreeMap, HashMap};

use cozo::DataValue;
use tracing::debug;

use super::store::DataStore;
use crate::error::GraphtorError;
use crate::parse::types::{CodeSnippet, Reference};

/// A stored hyperlink edge from one chunk to a document path.
#[derive(Debug, Clone, PartialEq)]
pub struct EdgeRecord {
    /// Identifier of the source chunk containing the link.
    pub src_chunk_id: String,
    /// Target document path (may be relative or absolute within the corpus).
    pub target_path: String,
    /// Human-readable link text.
    pub link_text: String,
    /// Optional anchor fragment (e.g. `"#heading-id"`).
    pub anchor: Option<String>,
}

/// A stored code snippet extracted from a chunk.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeRecord {
    /// Stable identifier for this snippet (SHA-256 of content + `chunk_id`).
    pub snippet_id: String,
    /// Identifier of the parent chunk.
    pub chunk_id: String,
    /// Programming language tag, if detected.
    pub language: Option<String>,
    /// Raw code content.
    pub content: String,
}

/// Upsert a hyperlink edge from a parsed [`Reference`].
///
/// Replaces any existing record with the same `(src_chunk_id, target_path)` key.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query failure.
pub fn upsert_edge(store: &DataStore, reference: &Reference) -> Result<(), GraphtorError> {
    let script = r"
        ?[src_chunk_id, target_path, link_text, anchor]
            <- [[$src, $target, $link_text, $anchor]]
        :put doc_edges { src_chunk_id, target_path => link_text, anchor }
    ";
    let mut params = BTreeMap::new();
    params.insert(
        "src".to_string(),
        DataValue::Str(reference.source_chunk_id.as_str().into()),
    );
    params.insert(
        "target".to_string(),
        DataValue::Str(reference.target_path.as_str().into()),
    );
    params.insert(
        "link_text".to_string(),
        DataValue::Str(reference.link_text.as_str().into()),
    );
    params.insert("anchor".to_string(), opt_str(reference.anchor.as_deref()));
    store.mutate(script, params)?;
    debug!(
        src = %reference.source_chunk_id,
        target = %reference.target_path,
        "upserted doc_edges record"
    );
    Ok(())
}

/// Upsert many hyperlink edges in a **single** multi-row `:put` mutation.
///
/// Mirrors [`upsert_edge`]'s column layout and per-row serialization but
/// amortizes `CozoScript` compilation and the transaction commit across the whole
/// slice — one script compile for `refs.len()` rows.
///
/// When two references share the same `(src_chunk_id, target_path)` key, the
/// **last** one in slice order wins, identical to calling [`upsert_edge`] once
/// per reference in order. (Within-batch duplicates are collapsed in Rust to
/// guarantee this, because a raw multi-row `:put` would otherwise resolve the
/// key by `CozoDB`'s sorted tuple order rather than by slice order.)
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on mutation failure.
pub fn upsert_edges_batch(store: &DataStore, refs: &[&Reference]) -> Result<(), GraphtorError> {
    if refs.is_empty() {
        return Ok(());
    }
    // Collapse duplicate keys keeping the last occurrence so the `:put` sees
    // unique keys and slice-order last-writer-wins is preserved exactly.
    let mut by_key: HashMap<(&str, &str), DataValue> = HashMap::with_capacity(refs.len());
    for reference in refs {
        let row = DataValue::List(vec![
            DataValue::Str(reference.source_chunk_id.as_str().into()),
            DataValue::Str(reference.target_path.as_str().into()),
            DataValue::Str(reference.link_text.as_str().into()),
            opt_str(reference.anchor.as_deref()),
        ]);
        by_key.insert(
            (
                reference.source_chunk_id.as_str(),
                reference.target_path.as_str(),
            ),
            row,
        );
    }
    let rows: Vec<DataValue> = by_key.into_values().collect();
    let script = r"
        ?[src_chunk_id, target_path, link_text, anchor] <- $rows
        :put doc_edges { src_chunk_id, target_path => link_text, anchor }
    ";
    let mut params = BTreeMap::new();
    let row_count = rows.len();
    params.insert("rows".to_string(), DataValue::List(rows));
    store.mutate(script, params)?;
    debug!(count = row_count, "batch-upserted doc_edges records");
    Ok(())
}

/// List all outgoing edges from the given chunk.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query or row-decode failure.
pub fn list_edges_from_chunk(
    store: &DataStore,
    src_chunk_id: &str,
) -> Result<Vec<EdgeRecord>, GraphtorError> {
    let script = r"
        ?[src_chunk_id, target_path, link_text, anchor]
            := *doc_edges{ src_chunk_id, target_path, link_text, anchor },
               src_chunk_id = $src
    ";
    let mut params = BTreeMap::new();
    params.insert("src".to_string(), DataValue::Str(src_chunk_id.into()));
    let rows = store.query(script, params)?;
    rows.rows.iter().map(|row| row_to_edge(row)).collect()
}

/// Upsert a code snippet from a parsed [`CodeSnippet`].
///
/// Replaces any existing record with the same `snippet_id`.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query failure.
pub fn upsert_code_snippet(store: &DataStore, snippet: &CodeSnippet) -> Result<(), GraphtorError> {
    let script = r"
        ?[snippet_id, chunk_id, language, content]
            <- [[$snippet_id, $chunk_id, $language, $content]]
        :put doc_code { snippet_id => chunk_id, language, content }
    ";
    let mut params = BTreeMap::new();
    params.insert(
        "snippet_id".to_string(),
        DataValue::Str(snippet.id.as_str().into()),
    );
    params.insert(
        "chunk_id".to_string(),
        DataValue::Str(snippet.chunk_id.as_str().into()),
    );
    params.insert("language".to_string(), opt_str(snippet.language.as_deref()));
    params.insert(
        "content".to_string(),
        DataValue::Str(snippet.content.as_str().into()),
    );
    store.mutate(script, params)?;
    debug!(snippet_id = %snippet.id, "upserted doc_code record");
    Ok(())
}

/// Upsert many code snippets in a **single** multi-row `:put` mutation.
///
/// Mirrors [`upsert_code_snippet`]'s column layout and per-row serialization
/// but amortizes `CozoScript` compilation and the transaction commit across the
/// whole slice — one script compile for `snippets.len()` rows.
///
/// When two snippets share the same `snippet_id`, `CozoDB` applies
/// last-writer-wins by key, identical to calling [`upsert_code_snippet`] once
/// per snippet in slice order.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on mutation failure.
pub fn upsert_code_snippets_batch(
    store: &DataStore,
    snippets: &[&CodeSnippet],
) -> Result<(), GraphtorError> {
    if snippets.is_empty() {
        return Ok(());
    }
    let rows: Vec<DataValue> = snippets
        .iter()
        .map(|snippet| {
            DataValue::List(vec![
                DataValue::Str(snippet.id.as_str().into()),
                DataValue::Str(snippet.chunk_id.as_str().into()),
                opt_str(snippet.language.as_deref()),
                DataValue::Str(snippet.content.as_str().into()),
            ])
        })
        .collect();
    let script = r"
        ?[snippet_id, chunk_id, language, content] <- $rows
        :put doc_code { snippet_id => chunk_id, language, content }
    ";
    let mut params = BTreeMap::new();
    params.insert("rows".to_string(), DataValue::List(rows));
    store.mutate(script, params)?;
    debug!(count = snippets.len(), "batch-upserted doc_code records");
    Ok(())
}

/// Delete all outgoing edges from chunks matching `src_chunk_id`.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on mutation failure.
pub fn delete_edges_for_chunk(store: &DataStore, src_chunk_id: &str) -> Result<(), GraphtorError> {
    let rm = r"
        ?[src_chunk_id, target_path]
            := *doc_edges{ src_chunk_id, target_path },
               src_chunk_id = $src
        :rm doc_edges { src_chunk_id, target_path }
    ";
    let mut params = BTreeMap::new();
    params.insert("src".to_string(), DataValue::Str(src_chunk_id.into()));
    store.mutate(rm, params)?;
    debug!(src_chunk_id, "deleted doc_edges records for chunk");
    Ok(())
}

/// Delete all code snippets associated with `chunk_id`.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on mutation failure.
pub fn delete_code_for_chunk(store: &DataStore, chunk_id: &str) -> Result<(), GraphtorError> {
    let rm = r"
        ?[snippet_id] := *doc_code{ snippet_id, chunk_id }, chunk_id = $cid
        :rm doc_code { snippet_id }
    ";
    let mut params = BTreeMap::new();
    params.insert("cid".to_string(), DataValue::Str(chunk_id.into()));
    store.mutate(rm, params)?;
    debug!(chunk_id, "deleted doc_code records for chunk");
    Ok(())
}

// ── Row decoders ─────────────────────────────────────────────────────────────

fn row_to_edge(row: &[DataValue]) -> Result<EdgeRecord, GraphtorError> {
    let src_chunk_id = require_str(row, 0, "src_chunk_id")?;
    let target_path = require_str(row, 1, "target_path")?;
    let link_text = require_str(row, 2, "link_text")?;
    let anchor = opt_col_str(row, 3);
    Ok(EdgeRecord {
        src_chunk_id,
        target_path,
        link_text,
        anchor,
    })
}

// ── Value helpers ─────────────────────────────────────────────────────────────

fn opt_str(v: Option<&str>) -> DataValue {
    match v {
        Some(s) => DataValue::Str(s.into()),
        None => DataValue::Null,
    }
}

fn require_str(row: &[DataValue], idx: usize, field: &str) -> Result<String, GraphtorError> {
    row.get(idx)
        .and_then(|v| v.get_str())
        .map(str::to_owned)
        .ok_or_else(|| GraphtorError::Database {
            message: format!("missing or non-string field '{field}' at column {idx}"),
            operation: "row_decode".to_string(),
        })
}

fn opt_col_str(row: &[DataValue], idx: usize) -> Option<String> {
    row.get(idx).and_then(|v| v.get_str()).map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> DataStore {
        let s = DataStore::open_mem().expect("open_mem");
        s.ensure_schema().expect("ensure_schema");
        s
    }

    fn reference(src: &str, target: &str, text: &str, anchor: Option<&str>) -> Reference {
        Reference {
            source_chunk_id: src.to_owned(),
            target_path: target.to_owned(),
            link_text: text.to_owned(),
            anchor: anchor.map(str::to_owned),
        }
    }

    fn snippet(id: &str, chunk_id: &str, lang: Option<&str>, content: &str) -> CodeSnippet {
        CodeSnippet {
            id: id.to_owned(),
            chunk_id: chunk_id.to_owned(),
            language: lang.map(str::to_owned),
            content: content.to_owned(),
        }
    }

    #[test]
    fn edges_batch_matches_single_row_puts() {
        let batched = store();
        let single = store();
        let r0 = reference("c1", "a.md", "A", None);
        let r1 = reference("c1", "b.md", "B", Some("#sec"));
        let r2 = reference("c2", "c.md", "C", None);
        let refs = [&r0, &r1, &r2];

        upsert_edges_batch(&batched, &refs).expect("batch edges");
        for r in refs {
            upsert_edge(&single, r).expect("single edge");
        }

        for src in ["c1", "c2"] {
            let mut b = list_edges_from_chunk(&batched, src).expect("list batch");
            let mut s = list_edges_from_chunk(&single, src).expect("list single");
            b.sort_by(|x, y| x.target_path.cmp(&y.target_path));
            s.sort_by(|x, y| x.target_path.cmp(&y.target_path));
            assert_eq!(b, s, "edge mismatch for {src}");
        }
    }

    #[test]
    fn edges_batch_last_writer_wins_on_duplicate_key() {
        let s = store();
        let first = reference("c1", "dup.md", "first", None);
        let second = reference("c1", "dup.md", "second", Some("#x"));
        upsert_edges_batch(&s, &[&first, &second]).expect("batch");
        let edges = list_edges_from_chunk(&s, "c1").expect("list");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].link_text, "second");
        assert_eq!(edges[0].anchor.as_deref(), Some("#x"));
    }

    #[test]
    fn code_snippets_batch_matches_single_row_puts() {
        let batched = store();
        let single = store();
        let s0 = snippet("s0", "c1", Some("rust"), "fn main() {}");
        let s1 = snippet("s1", "c1", None, "plain text");
        let s2 = snippet("s2", "c2", Some("json"), "{}");
        let snippets = [&s0, &s1, &s2];

        upsert_code_snippets_batch(&batched, &snippets).expect("batch code");
        for sn in snippets {
            upsert_code_snippet(&single, sn).expect("single code");
        }

        let read = |st: &DataStore| -> Vec<(String, String, Option<String>, String)> {
            let rows = st
                .query(
                    r"?[snippet_id, chunk_id, language, content] :=
                        *doc_code{ snippet_id, chunk_id, language, content }",
                    BTreeMap::new(),
                )
                .expect("query code");
            let mut out: Vec<_> = rows
                .rows
                .iter()
                .map(|row| {
                    (
                        require_str(row, 0, "snippet_id").expect("id"),
                        require_str(row, 1, "chunk_id").expect("cid"),
                        opt_col_str(row, 2),
                        require_str(row, 3, "content").expect("content"),
                    )
                })
                .collect();
            out.sort();
            out
        };
        assert_eq!(read(&batched), read(&single));
    }

    #[test]
    fn batch_empty_inputs_are_noops() {
        let s = store();
        upsert_edges_batch(&s, &[]).expect("empty edges");
        upsert_code_snippets_batch(&s, &[]).expect("empty code");
        assert!(list_edges_from_chunk(&s, "c1").expect("list").is_empty());
    }
}
