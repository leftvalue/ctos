//! Claude approximation: chars / chars_per_token. Always available (no crate).

use anyhow::Result;

use super::Tokenizer;

pub struct ClaudeApprox {
    chars_per_token: f64,
}

impl ClaudeApprox {
    pub fn new(_name: &str, chars_per_token: f64) -> Self {
        let cpt = if chars_per_token > 0.0 {
            chars_per_token
        } else {
            4.0
        };
        Self {
            chars_per_token: cpt,
        }
    }
}

impl Tokenizer for ClaudeApprox {
    fn source_label(&self) -> &str {
        "claude-approx"
    }
    fn approx(&self) -> bool {
        true
    }
    fn count(&self, text: &str) -> Result<f64> {
        Ok(text.chars().count() as f64 / self.chars_per_token)
    }
}
