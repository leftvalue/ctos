//! tiktoken-rs wrapper.
//!
//! Two construction paths:
//!   * `new(encoding)` — a named OpenAI encoding (o200k_base, cl100k_base, …)
//!   * `from_model_bytes(bytes, pattern)` — a `tiktoken.model` BPE vocab plus a
//!     model-specific split pattern (used by builtin tiktoken models like Kimi).
//!
//! Token *counting* uses `encode_ordinary`, which ignores special tokens, so we
//! never need the model's special-token table for our purposes.

use std::collections::HashMap;
use std::hash::BuildHasherDefault;

use anyhow::{bail, Context, Result};
use base64::Engine;
use rustc_hash::FxHasher;
use tiktoken_rs::CoreBPE;

use super::Tokenizer;

/// The map type tiktoken-rs expects for its ranks.
type FxMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

pub struct TiktokenTokenizer {
    #[allow(dead_code)]
    label: String,
    source_label: &'static str,
    bpe: CoreBPE,
}

impl TiktokenTokenizer {
    /// Build from a named OpenAI encoding.
    pub fn new(_name: &str, encoding: &str) -> Result<Self> {
        let bpe = match encoding {
            "o200k_base" => tiktoken_rs::o200k_base()?,
            "cl100k_base" => tiktoken_rs::cl100k_base()?,
            "p50k_base" => tiktoken_rs::p50k_base()?,
            "p50k_edit" => tiktoken_rs::p50k_edit()?,
            "r50k_base" | "gpt2" => tiktoken_rs::r50k_base()?,
            other => bail!("unknown tiktoken encoding: {other:?}"),
        };
        Ok(Self {
            label: encoding.to_string(),
            source_label: "tiktoken",
            bpe,
        })
    }

    /// Build from a raw `tiktoken.model` (base64-encoded token + rank per line)
    /// and a model-specific split pattern.
    pub fn from_model_bytes(
        label: &str,
        source_label: &'static str,
        model_bytes: &[u8],
        pattern: &str,
    ) -> Result<Self> {
        let text = std::str::from_utf8(model_bytes).context("tiktoken.model is not valid UTF-8")?;
        let mut encoder: FxMap<Vec<u8>, u32> = FxMap::default();
        for (lineno, line) in text.lines().enumerate() {
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split(' ');
            let b64 = parts
                .next()
                .with_context(|| format!("tiktoken.model line {} malformed", lineno + 1))?;
            let rank_str = parts
                .next()
                .with_context(|| format!("tiktoken.model line {} missing rank", lineno + 1))?;
            let token = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .with_context(|| format!("tiktoken.model line {} bad base64", lineno + 1))?;
            let rank: u32 = rank_str
                .parse()
                .with_context(|| format!("tiktoken.model line {} bad rank", lineno + 1))?;
            encoder.insert(token, rank);
        }
        if encoder.is_empty() {
            bail!("tiktoken.model contained no entries");
        }

        // Counting uses encode_ordinary, which ignores special tokens; an empty
        // special-token table is sufficient and keeps counts model-accurate.
        let special: FxMap<String, u32> = FxMap::default();
        let bpe = CoreBPE::new(encoder, special, pattern)
            .map_err(|e| anyhow::anyhow!("failed to build tiktoken BPE: {e}"))?;
        Ok(Self {
            label: label.to_string(),
            source_label,
            bpe,
        })
    }
}

impl Tokenizer for TiktokenTokenizer {
    fn source_label(&self) -> &str {
        self.source_label
    }
    fn count(&self, text: &str) -> Result<f64> {
        // encode_ordinary => no special tokens, matching add_special_tokens=false.
        Ok(self.bpe.encode_ordinary(text).len() as f64)
    }
}
