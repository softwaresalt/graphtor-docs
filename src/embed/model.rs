//! `EmbeddingModel` — loads and runs the `all-MiniLM-L6-v2` BERT model for
//! dense text embedding via the Candle ML framework.
//!
//! Two construction paths are provided:
//!
//! - [`EmbeddingModel::load`] — downloads the model from the Hugging Face Hub
//!   and caches it locally (requires an internet connection on first run).
//! - [`EmbeddingModel::from_path`] — loads a pre-downloaded model directory
//!   from disk with no network access required.

#![allow(clippy::module_name_repetitions)]

use std::path::Path;
use std::sync::{Arc, Mutex};

use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use tokenizers::{PaddingParams, Tokenizer, TruncationParams};

use crate::embed::pool::mean_pool;
use crate::error::GraphtorError;

/// Sequence length limit imposed on tokenizer input.
const MAX_LENGTH: usize = 512;

// ---------------------------------------------------------------------------
// Private inner state
// ---------------------------------------------------------------------------

struct Inner {
    model: BertModel,
    tokenizer: Tokenizer,
    device: Device,
}

// ---------------------------------------------------------------------------
// Public struct
// ---------------------------------------------------------------------------

/// A loaded instance of the `all-MiniLM-L6-v2` sentence-embedding model.
///
/// Cloning is cheap — the inner state is reference-counted.
#[derive(Clone)]
pub struct EmbeddingModel {
    inner: Arc<Mutex<Inner>>,
}

impl EmbeddingModel {
    /// Download the `all-MiniLM-L6-v2` model from the Hugging Face Hub and
    /// construct an [`EmbeddingModel`].
    ///
    /// On first call the weights and tokenizer are downloaded to the local
    /// Hugging Face cache (`~/.cache/huggingface/`). Subsequent calls reuse
    /// the cached files with no network access.
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Embed`] if the Hub download, file I/O, model
    /// weight loading, or tokenizer construction fails.
    pub fn load(model_id: &str) -> Result<Self, GraphtorError> {
        let api = hf_hub::api::sync::Api::new().map_err(|e| GraphtorError::Embed {
            message: format!("hf-hub init failed: {e}"),
            chunk_id: None,
        })?;
        let repo = api.model(model_id.to_string());

        let config_path = repo.get("config.json").map_err(|e| GraphtorError::Embed {
            message: format!("could not fetch config.json: {e}"),
            chunk_id: None,
        })?;
        let tokenizer_path = repo
            .get("tokenizer.json")
            .map_err(|e| GraphtorError::Embed {
                message: format!("could not fetch tokenizer.json: {e}"),
                chunk_id: None,
            })?;
        let weights_path = repo
            .get("model.safetensors")
            .map_err(|e| GraphtorError::Embed {
                message: format!("could not fetch model.safetensors: {e}"),
                chunk_id: None,
            })?;

        Self::build(&config_path, &tokenizer_path, &weights_path)
    }

    /// Load the model from a local directory that contains `config.json`,
    /// `tokenizer.json`, and `model.safetensors`.
    ///
    /// No network access is performed.
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Embed`] if the directory, any required file,
    /// the model weights, or the tokenizer cannot be read.
    pub fn from_path(dir: &Path) -> Result<Self, GraphtorError> {
        let config_path = dir.join("config.json");
        let tokenizer_path = dir.join("tokenizer.json");
        let weights_path = dir.join("model.safetensors");

        for p in [&config_path, &tokenizer_path, &weights_path] {
            if !p.exists() {
                return Err(GraphtorError::Embed {
                    message: format!("required file not found: {}", p.display()),
                    chunk_id: None,
                });
            }
        }

        Self::build(&config_path, &tokenizer_path, &weights_path)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Construct the model from fully-resolved file paths.
    fn build(
        config_path: &Path,
        tokenizer_path: &Path,
        weights_path: &Path,
    ) -> Result<Self, GraphtorError> {
        let config_json =
            std::fs::read_to_string(config_path).map_err(|e| GraphtorError::Embed {
                message: format!("could not read config.json: {e}"),
                chunk_id: None,
            })?;
        let bert_config: BertConfig =
            serde_json::from_str(&config_json).map_err(|e| GraphtorError::Embed {
                message: format!("could not parse config.json: {e}"),
                chunk_id: None,
            })?;

        let device = Device::Cpu;
        let tensors = candle_core::safetensors::load(weights_path, &device).map_err(|e| {
            GraphtorError::Embed {
                message: format!("could not load safetensors weights: {e}"),
                chunk_id: None,
            }
        })?;
        let vb = VarBuilder::from_tensors(tensors, DType::F32, &device);

        let model = BertModel::load(vb, &bert_config).map_err(|e| GraphtorError::Embed {
            message: format!("could not build BERT model: {e}"),
            chunk_id: None,
        })?;

        let mut tokenizer =
            Tokenizer::from_file(tokenizer_path).map_err(|e| GraphtorError::Embed {
                message: format!("could not load tokenizer: {e}"),
                chunk_id: None,
            })?;

        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: MAX_LENGTH,
                ..Default::default()
            }))
            .map_err(|e| GraphtorError::Embed {
                message: format!("could not configure truncation: {e}"),
                chunk_id: None,
            })?;
        tokenizer.with_padding(None::<PaddingParams>);

        Ok(Self {
            inner: Arc::new(Mutex::new(Inner {
                model,
                tokenizer,
                device,
            })),
        })
    }

    /// Embed a single text string and return its 384-dimensional vector.
    ///
    /// # Errors
    ///
    /// Returns [`GraphtorError::Embed`] if tokenisation, tensor construction,
    /// the BERT forward pass, or mean-pooling fails.
    pub(crate) fn embed_one(&self, text: &str) -> Result<Vec<f32>, GraphtorError> {
        let guard = self.inner.lock().map_err(|e| GraphtorError::Embed {
            message: format!("model lock poisoned: {e}"),
            chunk_id: None,
        })?;

        let encoding = guard
            .tokenizer
            .encode(text, true)
            .map_err(|e| GraphtorError::Embed {
                message: format!("tokenization failed: {e}"),
                chunk_id: None,
            })?;

        let ids: Vec<u32> = encoding.get_ids().to_vec();
        let type_ids: Vec<u32> = encoding.get_type_ids().to_vec();
        let mask: Vec<u32> = encoding.get_attention_mask().to_vec();
        let seq_len = ids.len();

        let input_ids = Tensor::from_vec(ids, (1, seq_len), &guard.device).map_err(|e| {
            GraphtorError::Embed {
                message: format!("input_ids tensor: {e}"),
                chunk_id: None,
            }
        })?;
        let token_type_ids =
            Tensor::from_vec(type_ids, (1, seq_len), &guard.device).map_err(|e| {
                GraphtorError::Embed {
                    message: format!("token_type_ids tensor: {e}"),
                    chunk_id: None,
                }
            })?;
        let attention_mask = Tensor::from_vec(mask, (1, seq_len), &guard.device).map_err(|e| {
            GraphtorError::Embed {
                message: format!("attention_mask tensor: {e}"),
                chunk_id: None,
            }
        })?;

        let hidden = guard
            .model
            .forward(&input_ids, &token_type_ids, Some(&attention_mask))
            .map_err(|e| GraphtorError::Embed {
                message: format!("BERT forward pass failed: {e}"),
                chunk_id: None,
            })?;

        let pooled = mean_pool(&hidden, &attention_mask).map_err(|e| GraphtorError::Embed {
            message: format!("mean pooling failed: {e}"),
            chunk_id: None,
        })?;

        let vec = pooled
            .squeeze(0)
            .map_err(|e| GraphtorError::Embed {
                message: format!("squeeze failed: {e}"),
                chunk_id: None,
            })?
            .to_vec1::<f32>()
            .map_err(|e| GraphtorError::Embed {
                message: format!("tensor to vec failed: {e}"),
                chunk_id: None,
            })?;

        Ok(vec)
    }
}
