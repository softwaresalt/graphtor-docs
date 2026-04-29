//! Source node CRUD operations.
//!
//! Manages `doc_sources`, the relation that records each documentation
//! source (Git repositories and local directories) registered for ingestion.

use std::collections::BTreeMap;

use cozo::DataValue;
use tracing::debug;

use super::store::DataStore;
use crate::error::GraphtorError;

/// A record representing a registered documentation source.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceRecord {
    /// Unique identifier for the source (e.g. `"ms-azure-core"`).
    pub source_id: String,
    /// Clone URL or filesystem path for the source.
    pub url: String,
    /// Source kind: `"git"` or `"local"`.
    pub kind: String,
    /// Human-readable display name.
    pub name: String,
    /// ISO-8601 timestamp of the last completed sync, if any.
    pub synced_at: Option<String>,
}

/// Upsert a documentation source record.
///
/// Replaces any existing record with the same `source_id`.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query failure.
pub fn upsert_source(store: &DataStore, rec: &SourceRecord) -> Result<(), GraphtorError> {
    let script = r"
        ?[source_id, url, kind, name, synced_at]
            <- [[$source_id, $url, $kind, $name, $synced_at]]
        :put doc_sources { source_id => url, kind, name, synced_at }
    ";
    let mut params = BTreeMap::new();
    params.insert(
        "source_id".to_string(),
        DataValue::Str(rec.source_id.as_str().into()),
    );
    params.insert("url".to_string(), DataValue::Str(rec.url.as_str().into()));
    params.insert("kind".to_string(), DataValue::Str(rec.kind.as_str().into()));
    params.insert("name".to_string(), DataValue::Str(rec.name.as_str().into()));
    params.insert("synced_at".to_string(), opt_str(rec.synced_at.as_deref()));
    store.mutate(script, params)?;
    debug!(source_id = %rec.source_id, "upserted doc_sources record");
    Ok(())
}

/// Retrieve a single documentation source by its identifier.
///
/// Returns `Ok(None)` if no matching source exists.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query or row-decode failure.
pub fn get_source(
    store: &DataStore,
    source_id: &str,
) -> Result<Option<SourceRecord>, GraphtorError> {
    let script = r"
        ?[source_id, url, kind, name, synced_at]
            := *doc_sources{ source_id, url, kind, name, synced_at },
               source_id = $id
    ";
    let mut params = BTreeMap::new();
    params.insert("id".to_string(), DataValue::Str(source_id.into()));
    let rows = store.query(script, params)?;
    rows.rows
        .into_iter()
        .next()
        .map(|row| row_to_source(&row))
        .transpose()
}

/// List all registered documentation sources.
///
/// # Errors
///
/// Returns [`GraphtorError::Database`] on query or row-decode failure.
pub fn list_sources(store: &DataStore) -> Result<Vec<SourceRecord>, GraphtorError> {
    let script = r"
        ?[source_id, url, kind, name, synced_at]
            := *doc_sources{ source_id, url, kind, name, synced_at }
    ";
    let rows = store.query(script, BTreeMap::new())?;
    rows.rows.iter().map(|row| row_to_source(row)).collect()
}

// ── Row decoders ─────────────────────────────────────────────────────────────

fn row_to_source(row: &[DataValue]) -> Result<SourceRecord, GraphtorError> {
    let source_id = require_str(row, 0, "source_id")?;
    let url = require_str(row, 1, "url")?;
    let kind = require_str(row, 2, "kind")?;
    let name = require_str(row, 3, "name")?;
    let synced_at = opt_col_str(row, 4);
    Ok(SourceRecord {
        source_id,
        url,
        kind,
        name,
        synced_at,
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
