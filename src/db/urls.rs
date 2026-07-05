//! Cross-source URL index operations.
//!
//! Manages `doc_url_index`, a lookup relation that maps a document's globally
//! unique `canonical_url` (as emitted by docline) to the source and entry
//! chunk that publishes it. This index is the key that lets graph traversal
//! follow a link whose target lives in a *different* ingested source — an
//! absolute Learn URL such as `/fabric/admin/foo` — without reintroducing the
//! relative-path ambiguity that source-scoped resolution prevents.
//!
//! The index is derived from chunk data: it is populated during ingest for the
//! entry chunk of each document that carries a `canonical_url`, and pruned when
//! the referenced chunks are deleted or the database is rebuilt.

use std::collections::BTreeMap;

use cozo::DataValue;
use tracing::{debug, warn};

use super::store::DataStore;
use crate::error::GraphtorError;

/// A resolved cross-source target: the source, chunk, and path a
/// `canonical_url` maps to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UrlResolution {
    /// Identifier of the source that owns the target document. Derived
    /// authoritatively from the target chunk's own `doc_chunks` row.
    pub source_id: String,
    /// Stable chunk identifier of the target document's entry chunk.
    pub chunk_id: String,
    /// Relative document path of the target chunk within its source.
    pub path: String,
}

/// Register a `canonical_url` as resolving to `chunk_id`.
///
/// Uses upsert semantics — re-ingesting a document overwrites the prior entry
/// for its `canonical_url`. `canonical_url` is the primary key and MUST be
/// globally unique across sources (docline's contract). When a *different*
/// chunk already owns the URL, a warning is emitted so operators can see the
/// collision rather than silently mis-resolving.
///
/// The owning `source_id` is intentionally **not** stored — it is derived from
/// the target chunk's own row at resolve time, so the index can never drift out
/// of agreement with `doc_chunks`.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query failure.
pub fn upsert_url_index(
    store: &DataStore,
    canonical_url: &str,
    chunk_id: &str,
) -> Result<(), GraphtorError> {
    if let Some(existing) = current_owner(store, canonical_url)? {
        if existing != chunk_id {
            warn!(
                canonical_url,
                existing_chunk_id = %existing,
                new_chunk_id = %chunk_id,
                "canonical_url collision: overwriting an existing owner — \
                 canonical_url must be globally unique across sources"
            );
        }
    }
    let script = r"
        ?[canonical_url, chunk_id] <- [[$canonical_url, $chunk_id]]
        :put doc_url_index { canonical_url => chunk_id }
    ";
    let mut params = BTreeMap::new();
    params.insert(
        "canonical_url".to_string(),
        DataValue::Str(canonical_url.into()),
    );
    params.insert("chunk_id".to_string(), DataValue::Str(chunk_id.into()));
    store.mutate(script, params)?;
    debug!(canonical_url, chunk_id, "upserted doc_url_index entry");
    Ok(())
}

/// Return the chunk id currently registered for `canonical_url`, if any.
fn current_owner(store: &DataStore, canonical_url: &str) -> Result<Option<String>, GraphtorError> {
    let script = r"?[chunk_id] := *doc_url_index{ canonical_url: $url, chunk_id }";
    let mut params = BTreeMap::new();
    params.insert("url".to_string(), DataValue::Str(canonical_url.into()));
    let rows = store.query(script, params)?;
    Ok(rows
        .rows
        .into_iter()
        .next()
        .and_then(|r| r.into_iter().next())
        .and_then(|v| v.get_str().map(str::to_owned)))
}

/// Resolve a `canonical_url` to its owning source, chunk, and path.
///
/// This lookup is **not** source-scoped: it resolves globally across every
/// ingested source, which is what enables cross-product link resolution.
/// Returns `Ok(None)` when no document publishes the given URL, or when the
/// indexed chunk no longer exists (a dangling entry resolves to nothing rather
/// than to a stale match).
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query failure.
pub fn resolve_canonical_url(
    store: &DataStore,
    canonical_url: &str,
) -> Result<Option<UrlResolution>, GraphtorError> {
    // Join the index against doc_chunks so a dangling index entry (chunk since
    // deleted) yields no row, and so source_id/path come from the authoritative
    // chunk row rather than a denormalised copy.
    let script = r"
        ?[source_id, chunk_id, path] :=
            *doc_url_index{ canonical_url: $url, chunk_id },
            *doc_chunks{ chunk_id, source_id, path }
    ";
    let mut params = BTreeMap::new();
    params.insert("url".to_string(), DataValue::Str(canonical_url.into()));
    let rows = store.query(script, params)?;
    let Some(row) = rows.rows.into_iter().next() else {
        return Ok(None);
    };
    let get = |idx: usize, field: &str| -> Result<String, GraphtorError> {
        row.get(idx)
            .and_then(|v| v.get_str())
            .map(str::to_owned)
            .ok_or_else(|| GraphtorError::Database {
                message: format!("missing {field} in doc_url_index resolution"),
                operation: "resolve_canonical_url".to_string(),
            })
    };
    Ok(Some(UrlResolution {
        source_id: get(0, "source_id")?,
        chunk_id: get(1, "chunk_id")?,
        path: get(2, "path")?,
    }))
}

/// Register (or refresh) a parsed document's `canonical_url` against its entry
/// chunk.
///
/// Shared by both write paths (full-sync pipeline and incremental reingest) so
/// the "which chunk represents this document" rule lives in exactly one place.
/// No-op when the document declares no `canonical_url` or has no chunks.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query failure.
pub fn register_document_url(
    store: &DataStore,
    doc: &crate::parse::types::ParsedDocument,
) -> Result<(), GraphtorError> {
    if let Some(canonical) = doc
        .frontmatter
        .as_ref()
        .and_then(|f| f.canonical_url.as_deref())
    {
        if let Some(entry) = doc.entry_chunk() {
            upsert_url_index(store, canonical, &entry.chunk_id)?;
        }
    }
    Ok(())
}

/// Remove any `doc_url_index` entries that point at the given chunk ids.
///
/// Called when chunks are deleted (re-ingest or source removal) so the index
/// does not retain entries for chunks that no longer exist.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query failure.
pub fn delete_url_index_for_chunks(
    store: &DataStore,
    chunk_ids: &[String],
) -> Result<(), GraphtorError> {
    for chunk_id in chunk_ids {
        let script = r"
            ?[canonical_url] := *doc_url_index{ canonical_url, chunk_id },
                                chunk_id = $cid
            :rm doc_url_index { canonical_url }
        ";
        let mut params = BTreeMap::new();
        params.insert("cid".to_string(), DataValue::Str(chunk_id.as_str().into()));
        store.mutate(script, params)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> DataStore {
        let s = DataStore::open_mem().expect("open_mem");
        s.ensure_schema().expect("ensure_schema");
        s
    }

    fn put_chunk(s: &DataStore, chunk_id: &str, source_id: &str, path: &str) {
        use crate::parse::types::Chunk;
        crate::db::chunks::upsert_chunk(
            s,
            source_id,
            &Chunk {
                chunk_id: chunk_id.to_owned(),
                content: "c".to_owned(),
                heading_hierarchy: vec![],
                position: 0,
                char_offset: 0,
                source_path: path.to_owned(),
            },
        )
        .expect("upsert_chunk");
    }

    #[test]
    fn resolve_returns_none_when_url_absent() {
        let s = store();
        assert_eq!(resolve_canonical_url(&s, "/nope").expect("resolve"), None);
    }

    #[test]
    fn upsert_then_resolve_roundtrips() {
        let s = store();
        put_chunk(&s, "chunk-1", "fabric", "admin/foo.md");
        upsert_url_index(&s, "/fabric/admin/foo", "chunk-1").expect("upsert");
        let resolved = resolve_canonical_url(&s, "/fabric/admin/foo").expect("resolve");
        assert_eq!(
            resolved,
            Some(UrlResolution {
                source_id: "fabric".to_owned(),
                chunk_id: "chunk-1".to_owned(),
                path: "admin/foo.md".to_owned(),
            })
        );
    }

    #[test]
    fn upsert_overwrites_prior_entry() {
        let s = store();
        put_chunk(&s, "chunk-old", "fabric", "old.md");
        put_chunk(&s, "chunk-new", "fabric", "new.md");
        upsert_url_index(&s, "/u", "chunk-old").expect("upsert 1");
        upsert_url_index(&s, "/u", "chunk-new").expect("upsert 2");
        let resolved = resolve_canonical_url(&s, "/u").expect("resolve").unwrap();
        assert_eq!(resolved.chunk_id, "chunk-new");
    }

    #[test]
    fn resolve_none_when_indexed_chunk_deleted() {
        let s = store();
        // Index a canonical_url whose chunk was never stored — the join yields
        // nothing, so resolution is graceful (dangling, not a stale match).
        upsert_url_index(&s, "/orphan", "missing-chunk").expect("upsert");
        assert_eq!(resolve_canonical_url(&s, "/orphan").expect("resolve"), None);
    }

    #[test]
    fn delete_for_chunks_removes_matching_entries() {
        let s = store();
        put_chunk(&s, "chunk-1", "fabric", "a.md");
        upsert_url_index(&s, "/u", "chunk-1").expect("upsert");
        delete_url_index_for_chunks(&s, &["chunk-1".to_owned()]).expect("delete");
        assert_eq!(resolve_canonical_url(&s, "/u").expect("resolve"), None);
    }

    #[test]
    fn collision_overwrites_and_resolves_to_latest() {
        // Two distinct chunks claiming the same canonical_url: last writer wins
        // (a warning is emitted). Verifies the overwrite is deterministic.
        let s = store();
        put_chunk(&s, "chunk-a", "src-a", "a.md");
        put_chunk(&s, "chunk-b", "src-b", "b.md");
        upsert_url_index(&s, "/dup", "chunk-a").expect("upsert a");
        upsert_url_index(&s, "/dup", "chunk-b").expect("upsert b");
        let resolved = resolve_canonical_url(&s, "/dup").expect("resolve").unwrap();
        assert_eq!(resolved.chunk_id, "chunk-b");
        assert_eq!(resolved.source_id, "src-b");
    }
}
