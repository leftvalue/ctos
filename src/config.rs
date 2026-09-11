//! Loading of `models.toml` (model registry) and `budgets.toml`.
//!
//! Built-in defaults are embedded from `config/*.toml` and used when no custom
//! path is provided. Parse failures on a custom file are hard errors (exit 2).

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

const DEFAULT_MODELS_TOML: &str = include_str!("../config/models.toml");
const DEFAULT_BUDGETS_TOML: &str = include_str!("../config/budgets.toml");

pub const DEFAULT_OVERHEAD_L1: f64 = 24.0;
pub const DEFAULT_CHARS_PER_TOKEN: f64 = 4.0;

/// One model entry as declared in models.toml.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelSpec {
    pub source: String,
    #[serde(default)]
    pub overhead_l1: Option<f64>,
    #[serde(default)]
    pub chars_per_token: Option<f64>,
}

impl ModelSpec {
    pub fn overhead_l1(&self) -> f64 {
        self.overhead_l1.unwrap_or(DEFAULT_OVERHEAD_L1)
    }
    pub fn chars_per_token(&self) -> f64 {
        self.chars_per_token.unwrap_or(DEFAULT_CHARS_PER_TOKEN)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ModelsFile {
    #[serde(default)]
    models: BTreeMap<String, ModelSpec>,
}

/// Ordered model registry (insertion order follows the TOML/BTreeMap order).
#[derive(Debug, Clone)]
pub struct ModelsConfig {
    pub models: Vec<(String, ModelSpec)>,
}

impl ModelsConfig {
    pub fn load(path: Option<&Path>, verbose: bool) -> Result<Self> {
        let text = match path {
            Some(p) => std::fs::read_to_string(p)
                .with_context(|| format!("failed to read models config: {}", p.display()))?,
            None => {
                if verbose {
                    eprintln!("[ctos] using built-in default model registry");
                }
                DEFAULT_MODELS_TOML.to_string()
            }
        };
        let parsed: ModelsFile = toml::from_str(&text)
            .context("failed to parse models config (expected models.toml schema)")?;
        let models = parsed.models.into_iter().collect();
        Ok(Self { models })
    }

    pub fn get(&self, name: &str) -> Option<&ModelSpec> {
        self.models.iter().find(|(n, _)| n == name).map(|(_, s)| s)
    }

    pub fn names(&self) -> Vec<String> {
        self.models.iter().map(|(n, _)| n.clone()).collect()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GlobalBudgets {
    pub l1_max: f64,
    pub l2_max: f64,
    pub l1_total_max: f64,
    pub diff_ratio: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BudgetsConfig {
    pub global: GlobalBudgets,
}

impl BudgetsConfig {
    pub fn load(path: Option<&Path>, verbose: bool) -> Result<Self> {
        let text = match path {
            Some(p) => std::fs::read_to_string(p)
                .with_context(|| format!("failed to read budgets config: {}", p.display()))?,
            None => {
                if verbose {
                    eprintln!("[ctos] using built-in default budgets");
                }
                DEFAULT_BUDGETS_TOML.to_string()
            }
        };
        let parsed: BudgetsConfig = toml::from_str(&text)
            .context("failed to parse budgets config (expected budgets.toml schema)")?;
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_models_parse() {
        let cfg = ModelsConfig::load(None, false).unwrap();
        assert!(cfg.get("qwen3").is_some());
        assert!(cfg.get("claude").is_some());
        assert_eq!(cfg.get("qwen3").unwrap().overhead_l1(), 24.0);
        assert_eq!(cfg.get("claude").unwrap().chars_per_token(), 4.0);
    }

    #[test]
    fn default_budgets_parse() {
        let b = BudgetsConfig::load(None, false).unwrap();
        assert_eq!(b.global.l1_max, 100.0);
        assert_eq!(b.global.l1_total_max, 2000.0);
    }
}
