//! Tokenizer abstraction and registry.
//!
//! A model name maps to a source string in `models.toml`. Four source kinds
//! are supported:
//!   builtin:<key>        vendored tokenizer.json (offline, feature `hf`)
//!   file:<path>          any local tokenizer.json (feature `hf`)
//!   tiktoken:<encoding>  a tiktoken encoding (feature `tiktoken`)
//!   claude-approx        chars / chars_per_token estimate (always available)

use std::sync::Arc;

use anyhow::{anyhow, bail, Result};

use crate::config::{ModelSpec, ModelsConfig};

#[cfg(feature = "hf")]
mod builtin;
mod claude;
#[cfg(feature = "hf")]
mod hf;
#[cfg(feature = "tiktoken")]
mod tiktoken;

/// A tokenizer that can turn text into a token count.
pub trait Tokenizer: Send + Sync {
    /// Short label for the source kind: builtin/file/tiktoken/claude-approx.
    fn source_label(&self) -> &str;
    /// Whether counts are approximate (rendered with a `~` prefix).
    fn approx(&self) -> bool {
        false
    }
    /// Count tokens of `text` (with add_special_tokens = false).
    fn count(&self, text: &str) -> Result<f64>;
}

/// A resolved registry entry.
pub struct ModelEntry {
    pub name: String,
    pub tokenizer: Arc<dyn Tokenizer>,
    pub overhead_l1: f64,
}

/// The active set of models for a run.
pub struct Registry {
    pub models: Vec<ModelEntry>,
}

impl Registry {
    /// Build a registry from config. When `selected` is empty, all models are
    /// used; otherwise only the named subset (error on unknown names).
    pub fn build(cfg: &ModelsConfig, selected: &[String]) -> Result<Self> {
        let names = if selected.is_empty() {
            cfg.names()
        } else {
            selected.to_vec()
        };
        let mut models = Vec::with_capacity(names.len());
        for name in names {
            let spec = cfg
                .get(&name)
                .ok_or_else(|| anyhow!("unknown model '{name}' (not found in registry)"))?;
            let tokenizer = build_tokenizer(&name, spec)?;
            models.push(ModelEntry {
                name,
                tokenizer,
                overhead_l1: spec.overhead_l1(),
            });
        }
        if models.is_empty() {
            bail!("no models selected and registry is empty");
        }
        Ok(Self { models })
    }
}

fn build_tokenizer(name: &str, spec: &ModelSpec) -> Result<Arc<dyn Tokenizer>> {
    let source = spec.source.as_str();

    if let Some(key) = source.strip_prefix("builtin:") {
        #[cfg(feature = "hf")]
        {
            return builtin::load(name, key);
        }
        #[cfg(not(feature = "hf"))]
        {
            let _ = key;
            bail!("model '{name}' uses builtin: but the crate was built without the 'hf' feature");
        }
    }

    if let Some(path) = source.strip_prefix("file:") {
        #[cfg(feature = "hf")]
        {
            return Ok(Arc::new(hf::HfTokenizer::from_file(name, "file", path)?));
        }
        #[cfg(not(feature = "hf"))]
        {
            let _ = path;
            bail!("model '{name}' uses file: but the crate was built without the 'hf' feature");
        }
    }

    if let Some(encoding) = source.strip_prefix("tiktoken:") {
        #[cfg(feature = "tiktoken")]
        {
            return Ok(Arc::new(tiktoken::TiktokenTokenizer::new(name, encoding)?));
        }
        #[cfg(not(feature = "tiktoken"))]
        {
            let _ = encoding;
            bail!("model '{name}' uses tiktoken: but the crate was built without the 'tiktoken' feature");
        }
    }

    if source == "claude-approx" {
        return Ok(Arc::new(claude::ClaudeApprox::new(
            name,
            spec.chars_per_token(),
        )));
    }

    bail!("model '{name}' has an unrecognized source: {source:?}")
}
