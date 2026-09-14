//! Shared data model produced by the count layer and consumed by output/check.

use std::path::PathBuf;

/// One scanned file with its per-model token count.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub rel_path: String,
    pub language: String,
    pub bytes: u64,
    pub is_binary: bool,
    /// Physical line count for text files; `None` for binary.
    pub lines: Option<u64>,
    /// `None` for binary files (only bytes are meaningful).
    pub tokens: Option<f64>,
    /// True when `tokens` was estimated (sampling) rather than exactly encoded.
    pub estimated: bool,
}

/// Sampling statistics for `--estimate` runs.
#[derive(Debug, Clone)]
pub struct EstimateInfo {
    /// Files that were (partially) encoded: Full or Slice.
    pub sampled_files: usize,
    /// Total text files considered.
    pub total_files: usize,
    /// Characters actually encoded.
    pub sampled_chars: u64,
    /// Total characters across text files.
    pub total_chars: u64,
    /// Heuristic error bound as a fraction (e.g. 0.018 = ±1.8%).
    pub error_bound: f64,
}

/// Language aggregation row (cloc-style).
#[derive(Debug, Clone)]
pub struct LangStat {
    pub language: String,
    pub files: usize,
    pub lines: u64,
    pub bytes: u64,
    pub tokens: f64,
}

/// Status of a single skill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillStatus {
    Ok,
    Invalid(String),
}

impl SkillStatus {
    pub fn label(&self) -> String {
        match self {
            SkillStatus::Ok => "OK".to_string(),
            SkillStatus::Invalid(reason) => format!("INVALID ({reason})"),
        }
    }
    pub fn is_invalid(&self) -> bool {
        matches!(self, SkillStatus::Invalid(_))
    }
}

/// A single L3 asset file.
#[derive(Debug, Clone)]
pub struct L3File {
    pub rel_path: String,
    pub tokens: f64,
    pub bytes: u64,
    pub is_binary: bool,
}

/// Three-layer breakdown of one skill under one model.
#[derive(Debug, Clone)]
pub struct SkillResult {
    pub skill: String,
    pub rel_path: String,
    pub l1: Option<f64>,
    pub l2: Option<f64>,
    pub l3_tokens: f64,
    pub l3_bytes: u64,
    pub l3_files: Vec<L3File>,
    pub status: SkillStatus,
    /// Whether the SKILL.md carries a `ctos-ok-growth` marker.
    pub growth_ok: bool,
    /// Issues attached during check (human-readable).
    pub issues: Vec<String>,
}

/// Everything computed for one model.
#[derive(Debug, Clone)]
pub struct ModelReport {
    pub model: String,
    pub source_label: String,
    pub approx: bool,
    pub languages: Vec<LangStat>,
    pub files: Vec<FileEntry>,
    pub skills: Vec<SkillResult>,
    /// Present when the run used `--estimate` (sampling).
    pub estimate: Option<EstimateInfo>,
}

impl ModelReport {
    /// Whether code/L3 token values should display with a `~` prefix:
    /// approximate tokenizer (claude-approx) OR estimate-mode sampling.
    /// Skill L1/L2 stay exact even in estimate mode.
    pub fn values_approx(&self) -> bool {
        self.approx || self.estimate.is_some()
    }

    /// Returns (files, lines, bytes, tokens) code totals.
    pub fn code_totals(&self) -> (usize, u64, u64, f64) {
        let files = self.files.len();
        let lines = self.languages.iter().map(|l| l.lines).sum();
        let bytes = self.files.iter().map(|f| f.bytes).sum();
        let tokens = self.languages.iter().map(|l| l.tokens).sum();
        (files, lines, bytes, tokens)
    }

    /// Sum of L1 across valid skills (resident context cost).
    pub fn resident_l1(&self) -> f64 {
        self.skills.iter().filter_map(|s| s.l1).sum()
    }

    /// Resident L1 + the largest single skill L2 (peak injection cost).
    pub fn peak_injection(&self) -> f64 {
        let max_l2 = self
            .skills
            .iter()
            .filter_map(|s| s.l2)
            .fold(0.0_f64, f64::max);
        self.resident_l1() + max_l2
    }
}

/// Top-level report across all models.
#[derive(Debug, Clone)]
pub struct Report {
    pub tool_name: String,
    pub tool_version: String,
    pub roots: Vec<PathBuf>,
    pub models: Vec<String>,
    pub reports: Vec<ModelReport>,
    /// Total files scanned (text + binary).
    pub files_scanned: usize,
    /// How many of those were binary (tokens skipped).
    pub binary_files: usize,
    /// Total physical lines across text files.
    pub total_lines: u64,
    /// Wall-clock seconds for scan + all-model counting.
    pub elapsed_secs: f64,
}
