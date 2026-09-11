//! HuggingFace `tokenizers` wrapper (loads tokenizer.json).

use anyhow::{Context, Result};

use super::Tokenizer;

pub struct HfTokenizer {
    source_label: &'static str,
    inner: tokenizers::Tokenizer,
}

impl HfTokenizer {
    pub fn from_bytes(_name: &str, source_label: &'static str, bytes: &[u8]) -> Result<Self> {
        let inner: tokenizers::Tokenizer =
            serde_json::from_slice(bytes).context("failed to parse tokenizer.json")?;
        Ok(Self {
            source_label,
            inner,
        })
    }

    pub fn from_file(name: &str, source_label: &'static str, path: &str) -> Result<Self> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read tokenizer file: {path}"))?;
        Self::from_bytes(name, source_label, &bytes)
    }
}

impl Tokenizer for HfTokenizer {
    fn source_label(&self) -> &str {
        self.source_label
    }
    fn count(&self, text: &str) -> Result<f64> {
        let encoding = self
            .inner
            .encode(text, false)
            .map_err(|e| anyhow::anyhow!("tokenizer encode failed: {e}"))?;
        Ok(encoding.get_ids().len() as f64)
    }
}
