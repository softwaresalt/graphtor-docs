//! Hyperlink reference extraction from an [`AstNode`] stream.
//!
//! [`extract`] walks the node list and collects every [`AstNode::Link`] into
//! a [`Reference`] record that the storage stage will persist as a
//! `REFERENCES` graph edge.

use crate::parse::types::{AstNode, Reference};

/// Extract all hyperlink references from `nodes` that belong to `chunk_id`.
///
/// For each [`AstNode::Link`] found, a [`Reference`] is produced with:
/// - the `source_chunk_id` set to `chunk_id`
/// - the raw `target_path` from the link URL (callers may normalise it later)
/// - the visible anchor text
/// - an optional `#fragment` extracted from the URL
///
/// # Panics
///
/// Does not panic.
#[must_use]
pub fn extract(nodes: &[AstNode], chunk_id: &str) -> Vec<Reference> {
    nodes
        .iter()
        .filter_map(|node| {
            if let AstNode::Link { url, text } = node {
                let (target_path, anchor) = split_anchor(url);
                Some(Reference {
                    source_chunk_id: chunk_id.to_string(),
                    target_path,
                    link_text: text.clone(),
                    anchor,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Split a URL into `(path, Option<fragment>)`.
fn split_anchor(url: &str) -> (String, Option<String>) {
    if let Some(pos) = url.find('#') {
        let path = url[..pos].to_string();
        let fragment = url[pos + 1..].to_string();
        let anchor = if fragment.is_empty() {
            None
        } else {
            Some(fragment)
        };
        (path, anchor)
    } else {
        (url.to_string(), None)
    }
}
