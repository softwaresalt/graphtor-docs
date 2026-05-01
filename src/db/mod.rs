//! Unified embedded database module for `LocalDocRAG`.
//!
//! This module exposes a single [`DataStore`] type backed by `CozoDB` (an
//! embedded Datalog/graph/vector database). The store holds five stored
//! relations:
//!
//! - `doc_sources` — registered documentation sources (Git repos, local dirs)
//! - `doc_chunks` — parsed and normalised document chunks with metadata
//! - `doc_edges` — directed hyperlink edges between chunks
//! - `doc_code` — code snippets extracted from chunks
//! - `doc_vectors` — 384-dimensional embeddings for semantic search
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
pub mod vectors;

pub use chunks::{
    delete_chunks_by_path, get_chunk, list_chunks_for_source, upsert_chunk, ChunkRecord,
};
pub use edges::{
    delete_code_for_chunk, delete_edges_for_chunk, list_edges_from_chunk, upsert_code_snippet,
    upsert_edge, CodeRecord, EdgeRecord,
};
pub use nodes::{get_source, list_sources, upsert_source, SourceRecord};
pub use schema::ensure_schema;
pub use search::{search_by_text, search_similar, SearchResult};
pub use store::DataStore;
pub use traverse::{find_related_chunks, TraversalResult};
pub use vectors::{delete_vectors_by_chunk_ids, search_by_vector, upsert_vector};
