//! Deterministic stratified sampling for `--estimate` mode.
//!
//! Within one language the chars->token ratio is fairly stable (measured CV
//! 5-13% for common languages), while across languages it varies wildly — so
//! sampling is stratified by language. Big files are visited first (they
//! dominate totals); each consumes at most `SLICE_CAP` chars of the per-language
//! budget via a head slice, and files past the budget are estimated with the
//! language-level ratio. Everything is deterministic: the plan depends only on
//! file sizes / language / path, never on the model or randomness.

use std::collections::BTreeMap;

use crate::walk::ScannedFile;

/// Default per-language character budget for sampling.
pub const DEFAULT_SAMPLE_BUDGET: usize = 524_288;
/// Maximum characters encoded from any single file (head slice).
pub const SLICE_CAP: usize = 65_536;

/// Per-file sampling decision, aligned with the input file list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplePlan {
    /// Encode the whole file exactly.
    Full,
    /// Encode only the first `n` chars; extrapolate by in-file ratio.
    Slice(usize),
    /// Do not encode; estimate with the language ratio.
    Skip,
}

impl SamplePlan {
    /// Characters that will actually be encoded for a file with this plan.
    pub fn encoded_chars(&self, total_chars: usize) -> usize {
        match self {
            SamplePlan::Full => total_chars,
            SamplePlan::Slice(n) => (*n).min(total_chars),
            SamplePlan::Skip => 0,
        }
    }

    pub fn is_sampled(&self) -> bool {
        !matches!(self, SamplePlan::Skip)
    }
}

/// Build the sampling plan for all files (text files get a real plan; binary
/// files get `Skip` but are never consulted by the counting loop).
///
/// SKILL.md files are always `Full` (exact) and do not consume budget — their
/// L1/L2 numbers feed the budget gate.
pub fn plan_sampling(
    files: &[ScannedFile],
    budget_per_lang: usize,
    slice_cap: usize,
) -> Vec<SamplePlan> {
    let mut by_lang: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, f) in files.iter().enumerate() {
        if f.content.is_some() {
            by_lang.entry(f.language.as_str()).or_default().push(i);
        }
    }

    let mut plans = vec![SamplePlan::Skip; files.len()];

    for idxs in by_lang.values() {
        let mut idxs = idxs.clone();
        // Largest first (they dominate totals); ties broken by path for
        // determinism.
        idxs.sort_by(|&a, &b| {
            files[b]
                .bytes
                .cmp(&files[a].bytes)
                .then_with(|| files[a].rel_path.cmp(&files[b].rel_path))
        });

        let mut budget = budget_per_lang;
        for i in idxs {
            let f = &files[i];
            let text = f.content.as_ref().expect("text file");
            let chars = text.chars().count();

            let is_skill_md = f.path.file_name().map(|n| n == "SKILL.md").unwrap_or(false);

            if is_skill_md || chars == 0 {
                // Always exact; empty files cost nothing anyway.
                plans[i] = SamplePlan::Full;
                continue;
            }
            if budget == 0 {
                plans[i] = SamplePlan::Skip;
                continue;
            }
            let take = chars.min(slice_cap).min(budget);
            plans[i] = if take == chars {
                SamplePlan::Full
            } else {
                SamplePlan::Slice(take)
            };
            budget -= take;
        }
    }

    plans
}

/// One encoded sample: (chars encoded, tokens observed).
pub type Sample = (u64, f64);

/// Language ratio (tokens per char) from sampled slices; 0 when unsampled.
pub fn lang_ratio(samples: &[Sample]) -> f64 {
    let chars: u64 = samples.iter().map(|s| s.0).sum();
    if chars == 0 {
        return 0.0;
    }
    let tokens: f64 = samples.iter().map(|s| s.1).sum();
    tokens / chars as f64
}

/// Weighted coefficient of variation of the per-slice ratios (0..1); `None`
/// when fewer than 2 samples or a degenerate mean.
fn ratio_cv(samples: &[Sample]) -> Option<f64> {
    if samples.len() < 2 {
        return None;
    }
    let chars: u64 = samples.iter().map(|s| s.0).sum();
    if chars == 0 {
        return None;
    }
    let mean = lang_ratio(samples);
    if mean <= 0.0 {
        return None;
    }
    let variance: f64 = samples
        .iter()
        .map(|&(c, t)| {
            let w = c as f64 / chars as f64;
            let r = if c > 0 { t / c as f64 } else { 0.0 };
            w * (r - mean) * (r - mean)
        })
        .sum();
    Some(variance.sqrt() / mean)
}

/// Heuristic error bound for the whole estimate: per-language ratio CVs,
/// weighted by each language's share of *estimated* characters (error only
/// comes from text that was not exactly encoded — a fully-sampled run bounds
/// to 0). Languages with fewer than 2 samples contribute 0. Returns a
/// fraction (e.g. 0.018 = ±1.8%).
pub fn error_bound(entries: &[(u64, &[Sample])], total_chars: u64) -> f64 {
    if total_chars == 0 {
        return 0.0;
    }
    entries
        .iter()
        .filter_map(|(estimated_chars, samples)| {
            ratio_cv(samples).map(|cv| cv * (*estimated_chars as f64 / total_chars as f64))
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(rel: &str, lang: &str, content: &str) -> ScannedFile {
        ScannedFile {
            path: std::path::PathBuf::from(rel),
            rel_path: rel.to_string(),
            language: lang.to_string(),
            bytes: content.len() as u64,
            is_binary: false,
            lines: Some(content.lines().count() as u64),
            content: Some(content.to_string()),
        }
    }

    #[test]
    fn plan_is_deterministic() {
        let files: Vec<ScannedFile> = (0..20)
            .map(|i| file(&format!("f{i}.rs"), "Rust", "fn main() {} // pad"))
            .collect();
        let a = plan_sampling(&files, 100, 50);
        let b = plan_sampling(&files, 100, 50);
        assert_eq!(a, b);
    }

    #[test]
    fn big_files_first_and_budget_order() {
        let files = vec![
            file("small.rs", "Rust", "let x = 1;"),
            file("big.rs", "Rust", &"x".repeat(1000)),
            file("mid.rs", "Rust", &"y".repeat(200)),
        ];
        // Budget 300, cap 100: big -> Slice(100), mid -> Slice(100), small -> Full.
        let plans = plan_sampling(&files, 300, 100);
        assert_eq!(plans[0], SamplePlan::Full); // small.rs fits in the rest
        assert_eq!(plans[1], SamplePlan::Slice(100));
        assert_eq!(plans[2], SamplePlan::Slice(100));
    }

    #[test]
    fn budget_exhaustion_skips_the_rest() {
        let files: Vec<ScannedFile> = (0..10)
            .map(|i| file(&format!("a{i}.py"), "Python", &"z".repeat(50)))
            .collect();
        let plans = plan_sampling(&files, 120, 100);
        let sampled = plans.iter().filter(|p| p.is_sampled()).count();
        // 120 budget / 50 per file => first 2 files Full (100), third partial.
        assert_eq!(sampled, 3);
        assert!(matches!(plans[9], SamplePlan::Skip));
    }

    #[test]
    fn skill_md_always_full_even_without_budget() {
        let files = vec![
            file("my/SKILL.md", "Markdown", &"s".repeat(10_000)),
            file("my/asset.py", "Python", &"z".repeat(100)),
        ];
        let plans = plan_sampling(&files, 0, 100);
        assert_eq!(plans[0], SamplePlan::Full);
        assert!(matches!(plans[1], SamplePlan::Skip));
    }

    #[test]
    fn languages_have_separate_budgets() {
        let files = vec![
            file("a.rs", "Rust", &"r".repeat(1000)),
            file("b.py", "Python", &"p".repeat(1000)),
        ];
        let plans = plan_sampling(&files, 300, 150);
        assert_eq!(plans[0], SamplePlan::Slice(150));
        assert_eq!(plans[1], SamplePlan::Slice(150));
    }

    #[test]
    fn empty_file_is_full() {
        let files = vec![file("empty.rs", "Rust", "")];
        assert_eq!(plan_sampling(&files, 10, 10)[0], SamplePlan::Full);
    }

    #[test]
    fn ratio_and_extrapolation_math() {
        // 100 chars -> 25 tokens, 300 chars -> 60 tokens => ratio 0.2125
        let samples = vec![(100, 25.0), (300, 60.0)];
        let r = lang_ratio(&samples);
        assert!((r - 85.0 / 400.0).abs() < 1e-12);
        // A 1000-char unsampled file estimates to ratio * 1000.
        assert!((r * 1000.0 - 212.5).abs() < 1e-9);
        // Slice extrapolation: 50 chars -> 12 tokens over a 200-char file.
        let est = 12.0 * (200.0 / 50.0);
        assert_eq!(est, 48.0);
    }

    #[test]
    fn ratio_of_empty_is_zero() {
        assert_eq!(lang_ratio(&[]), 0.0);
        assert_eq!(lang_ratio(&[(0, 0.0)]), 0.0);
    }

    #[test]
    fn error_bound_weights_estimated_chars_only() {
        // Rust: fully sampled (0 estimated chars) -> contributes nothing.
        // Python: 1000 estimated chars with wild ratios -> dominates.
        let rust = vec![(500, 100.0), (500, 102.0)]; // cv ~1%
        let py = vec![(500, 100.0), (500, 200.0)]; // cv ~33%
        let bound = error_bound(&[(0, &rust), (1000, &py)], 2000);
        assert!(bound > 0.1 && bound < 0.25, "bound = {bound}");
        // Fully sampled everything -> zero bound.
        assert_eq!(error_bound(&[(0, &rust), (0, &py)], 2000), 0.0);
        // Weighting: half of Python's chars estimated.
        let half = error_bound(&[(0, &rust), (500, &py)], 2000);
        let full = error_bound(&[(0, &rust), (1000, &py)], 2000);
        assert!((half - full / 2.0).abs() < 1e-12);
    }

    #[test]
    fn encoded_chars_matches_plan() {
        assert_eq!(SamplePlan::Full.encoded_chars(123), 123);
        assert_eq!(SamplePlan::Slice(50).encoded_chars(123), 50);
        assert_eq!(SamplePlan::Slice(999).encoded_chars(123), 123);
        assert_eq!(SamplePlan::Skip.encoded_chars(123), 0);
    }
}
