//! Tokenizer abstraction and registry.
//!
//! A model name maps to a source string in `models.toml`. Four source kinds
//! are supported:
//!   builtin:<key>        vendored tokenizer.json (offline, feature `hf`)
//!   file:<path>          any local tokenizer.json (feature `hf`)
//!   tiktoken:<encoding>  a tiktoken encoding (feature `tiktoken`)
//!   claude-approx        chars / chars_per_token estimate (always available)

use std::sync::Arc;

use anyhow::{bail, Result};

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
    /// Resolve the model names to use.
    ///
    /// Precedence: `all_models` (or a lone `-m all`) → every registered model;
    /// otherwise an explicit `selected` list; otherwise the config default.
    fn resolve_names(cfg: &ModelsConfig, selected: &[String], all_models: bool) -> Vec<String> {
        let wants_all =
            all_models || selected.len() == 1 && selected[0].eq_ignore_ascii_case("all");
        if wants_all {
            cfg.names()
        } else if selected.is_empty() {
            cfg.default_selection()
        } else {
            selected.to_vec()
        }
    }

    /// Build a registry from config.
    ///
    /// Models that cannot be built — an unknown name, or a tokenizer that fails
    /// to load (e.g. a `builtin:` not vendored in this build) — are reported as
    /// warnings on stderr and skipped, so one bad model never fails the whole
    /// run. It is an error only if *no* usable model remains.
    pub fn build(cfg: &ModelsConfig, selected: &[String], all_models: bool) -> Result<Self> {
        let names = Self::resolve_names(cfg, selected, all_models);

        let mut models = Vec::with_capacity(names.len());
        let mut skipped = 0usize;
        for name in names {
            let spec = match cfg.get(&name) {
                Some(s) => s,
                None => {
                    eprintln!("ctos: warning: unknown model '{name}', skipping");
                    skipped += 1;
                    continue;
                }
            };
            match build_tokenizer(&name, spec) {
                Ok(tokenizer) => models.push(ModelEntry {
                    name,
                    tokenizer,
                    overhead_l1: spec.overhead_l1(),
                }),
                Err(e) => {
                    eprintln!("ctos: warning: skipping model '{name}': {e}");
                    skipped += 1;
                }
            }
        }
        if models.is_empty() {
            bail!(
                "no usable tokenizer models{}",
                if skipped > 0 {
                    " (all selected models were skipped; see warnings above)"
                } else {
                    " selected"
                }
            );
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
