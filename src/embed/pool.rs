//! Mean-pooling over token embeddings.
//!
//! Reduces a `[batch, seq_len, hidden_size]` tensor to a
//! `[batch, hidden_size]` sentence embedding by averaging only the
//! real (non-padding) token representations indicated by `attention_mask`.

#![allow(clippy::module_name_repetitions)]

use candle_core::{DType, Result, Tensor};

/// Compute the mean of token embeddings, masking out padding positions.
///
/// Given a hidden-states tensor of shape `[batch, seq_len, hidden_size]` and a
/// binary attention-mask tensor of shape `[batch, seq_len]`, returns a
/// `[batch, hidden_size]` tensor where each row is the mean of the unmasked
/// token representations.
///
/// # Errors
///
/// Returns a [`candle_core::Error`] if tensor broadcasting, type-casting, or
/// arithmetic operations fail.
pub fn mean_pool(hidden_states: &Tensor, attention_mask: &Tensor) -> Result<Tensor> {
    let (batch, seq_len, hidden) = hidden_states.dims3()?;
    let mask = attention_mask
        .unsqueeze(2)?
        .broadcast_as((batch, seq_len, hidden))?
        .to_dtype(DType::F32)?;
    let sum = hidden_states.to_dtype(DType::F32)?.mul(&mask)?.sum(1)?;
    let count = attention_mask
        .to_dtype(DType::F32)?
        .sum(1)?
        .unsqueeze(1)?
        .broadcast_as((batch, hidden))?;
    sum.div(&count)
}
