//! Builtin tokenizers: files vendored under `tokenizers/<key>/` and embedded at
//! build time by `build.rs`.
//!
//! A key may be backed by either:
//!   * `tokenizer.json`  → HuggingFace `tokenizers` (Qwen3, DeepSeek-V3, …)
//!   * `tiktoken.model`  → tiktoken-rs, with a per-key split pattern (Kimi-K2)

use std::io::Read;
use std::sync::Arc;

use anyhow::{bail, Context, Result};

use super::hf::HfTokenizer;
#[cfg(feature = "tiktoken")]
use super::tiktoken::TiktokenTokenizer;
use super::Tokenizer;

include!(concat!(env!("OUT_DIR"), "/builtin_data.rs"));

/// Decompress a gzip blob embedded at build time.
fn gunzip(gz: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(gz)
        .read_to_end(&mut out)
        .context("failed to decompress embedded tokenizer")?;
    Ok(out)
}

/// The tiktoken split pattern for a builtin tiktoken-model key.
///
/// Kimi-K2 ships a `tiktoken.model` plus a custom `pat_str` (from its
/// `tokenization_kimi.py`). The `&&` set-intersection subclasses are kept
/// verbatim — `fancy-regex` accepts them — so counts match the model exactly.
#[cfg(feature = "tiktoken")]
fn tiktoken_pattern(key: &str) -> Option<&'static str> {
    const KIMI_K2: &str = concat!(
        r"[\p{Han}]+",
        "|",
        r"[^\r\n\p{L}\p{N}]?[\p{Lu}\p{Lt}\p{Lm}\p{Lo}\p{M}&&[^\p{Han}]]*[\p{Ll}\p{Lm}\p{Lo}\p{M}&&[^\p{Han}]]+(?i:'s|'t|'re|'ve|'m|'ll|'d)?",
        "|",
        r"[^\r\n\p{L}\p{N}]?[\p{Lu}\p{Lt}\p{Lm}\p{Lo}\p{M}&&[^\p{Han}]]+[\p{Ll}\p{Lm}\p{Lo}\p{M}&&[^\p{Han}]]*(?i:'s|'t|'re|'ve|'m|'ll|'d)?",
        "|",
        r"\p{N}{1,3}",
        "|",
        r" ?[^\s\p{L}\p{N}]+[\r\n]*",
        "|",
        r"\s*[\r\n]+",
        "|",
        r"\s+(?!\S)",
        "|",
        r"\s+",
    );
    match key {
        "kimi-k2" => Some(KIMI_K2),
        _ => None,
    }
}

pub fn load(name: &str, key: &str) -> Result<Arc<dyn Tokenizer>> {
    // Prefer a HuggingFace tokenizer.json when present.
    if let Some(gz) = builtin_tokenizer_gz(key) {
        let bytes = gunzip(gz)?;
        return Ok(Arc::new(HfTokenizer::from_bytes(name, "builtin", &bytes)?));
    }

    // Otherwise try a vendored tiktoken.model with a known per-key pattern.
    #[cfg(feature = "tiktoken")]
    if let Some(gz) = builtin_tiktoken_model_gz(key) {
        let bytes = gunzip(gz)?;
        match tiktoken_pattern(key) {
            Some(pattern) => {
                return Ok(Arc::new(TiktokenTokenizer::from_model_bytes(
                    name, "builtin", &bytes, pattern,
                )?));
            }
            None => bail!(
                "builtin '{key}' bundles a tiktoken.model but ctos has no split \
                 pattern registered for it; add one in tokenizer/builtin.rs"
            ),
        }
    }

    bail!(
        "builtin tokenizer '{key}' is not bundled in this binary.\n  \
         Vendor it by placing the model's tokenizer.json (or tiktoken.model) under \
         tokenizers/{key}/ and rebuilding, or point the model at a \
         file:/path/to/tokenizer.json source instead.\n  \
         Bundled builtin keys: {:?}",
        BUILTIN_KEYS
    )
}
