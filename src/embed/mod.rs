//! Embedding engine — produce 384-dimensional dense vectors from text.
//!
//! Uses the `all-MiniLM-L6-v2` BERT model loaded via the Candle ML framework
//! for in-process, zero-network-call inference at runtime.
//!
//! # Quick start
//!
//! ```no_run
//! use graphtor_core::embed::{EmbeddingModel, embed_text};
//!
//! let model = EmbeddingModel::load("sentence-transformers/all-MiniLM-L6-v2")
//!     .expect("model load failed");
//! let vec = embed_text(&model, "hello world").expect("embed failed");
//! assert_eq!(vec.len(), 384);
//! ```

#![allow(clippy::module_name_repetitions)]

pub mod model;
pub mod pool;
pub mod resolver;

pub use model::EmbeddingModel;
pub use pool::mean_pool;
pub use resolver::{resolve_embedding_model, ResolverCaller, DEFAULT_MODEL_ID};

use crate::error::GraphtorError;

/// Embed a single text string.
///
/// Returns a `Vec<f32>` of length 384 (the hidden size of
/// `all-MiniLM-L6-v2`).
///
/// # Errors
///
/// Propagates [`GraphtorError::Embed`] from the underlying model forward pass
/// or tokenisation step.
pub fn embed_text(model: &EmbeddingModel, text: &str) -> Result<Vec<f32>, GraphtorError> {
    model.embed_one(text)
}

/// Embed a batch of text strings.
///
/// Returns one 384-dimensional vector per input string in the same order as
/// `texts`.
///
/// # Errors
///
/// Returns [`GraphtorError::Embed`] on the first text that fails to embed.
pub fn embed_batch(model: &EmbeddingModel, texts: &[&str]) -> Result<Vec<Vec<f32>>, GraphtorError> {
    texts.iter().map(|t| model.embed_one(t)).collect()
}
