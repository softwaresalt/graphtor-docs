//! Unified embedded database module for `LocalDocRAG`.
//!
//! This module exposes a single [`DataStore`] type backed by `CozoDB` (an
//! embedded Datalog/graph/vector database). The store holds these stored
//! relations:
//!
//! - `doc_sources` — registered documentation sources (Git repos, local dirs)
//! - `doc_chunks` — parsed and normalised document chunks with metadata and
//!   optional `embedding: <F32; 384>?` column indexed by a native HNSW index
//! - `doc_edges` — directed hyperlink edges between chunks
//! - `doc_code` — code snippets extracted from chunks
//! - `doc_url_index` — maps a document's globally unique `canonical_url` to its
//!   entry chunk, enabling cross-source link resolution during traversal
//!
//! ## Usage
//!
//! ```rust,ignore
//! use graphtor_core::db::DataStore;
//!
//! let store = DataStore::open_mem()?;
//! store.ensure_schema()?;
//! ```

pub mod chunks;
pub mod edges;
pub mod nodes;
pub mod schema;
pub mod search;
pub mod store;
pub mod traverse;
pub mod urls;
pub mod vectors;

pub use chunks::{
    delete_chunks_by_path, get_chunk, get_vectors_for, list_chunks_by_path, list_chunks_for_source,
    upsert_chunk, upsert_chunks_batch, ChunkRecord,
};
pub use edges::{
    delete_code_for_chunk, delete_edges_for_chunk, list_edges_from_chunk, upsert_code_snippet,
    upsert_code_snippets_batch, upsert_edge, upsert_edges_batch, CodeRecord, EdgeRecord,
};
pub use nodes::{get_source, list_sources, upsert_source, SourceRecord};
pub use schema::{
    apply_v4_prune, ensure_schema, mark_v4_migration_complete, needs_v4_migration,
    prune_v4_data_for_rebuild,
};
pub use search::{search_by_text, search_similar, SearchResult};
pub use store::{DataStore, DbStatus};
pub use traverse::{find_related_chunks, TraversalResult};
pub use urls::{
    delete_url_index_for_chunks, register_document_url, resolve_canonical_url, upsert_url_index,
    upsert_url_index_batch, UrlResolution,
};
pub use vectors::{delete_vectors_by_chunk_ids, search_by_vector, upsert_vector};
