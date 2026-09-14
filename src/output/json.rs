//! JSON output.
//!
//! The `results` array follows the schema pinned in ctos_plan.md §5.4 (field
//! names must not change). A parallel `code` block carries the language/file
//! aggregation that this build adds on top of the skill schema.

use anyhow::Result;
use serde::Serialize;

use crate::model::{ModelReport, Report, SkillStatus};

#[derive(Serialize)]
struct ToolInfo<'a> {
    name: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
struct SkillFile {
    path: String,
    tokens: f64,
}

#[derive(Serialize)]
struct SkillResultJson {
    skill: String,
    model: String,
    l1: f64,
    l2: f64,
    l3_tokens: f64,
    l3_bytes: u64,
    files: Vec<SkillFile>,
    status: String,
    issues: Vec<String>,
}

#[derive(Serialize)]
struct LangJson {
    language: String,
    files: usize,
    lines: u64,
    bytes: u64,
    tokens: f64,
}

#[derive(Serialize)]
struct CodeFileJson {
    path: String,
    language: String,
    bytes: u64,
    is_binary: bool,
    lines: Option<u64>,
    tokens: Option<f64>,
    /// Present (true) only when `tokens` was estimated by sampling.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    estimated: bool,
}

/// Sampling metadata for estimate-mode runs.
#[derive(Serialize)]
struct EstimateJson {
    sampled_files: usize,
    total_files: usize,
    sampled_chars: u64,
    total_chars: u64,
    /// Heuristic error bound as a fraction (0.018 = ±1.8%).
    error_bound: f64,
}

#[derive(Serialize)]
struct CodeBlockJson {
    model: String,
    approx: bool,
    languages: Vec<LangJson>,
    files: Vec<CodeFileJson>,
    /// Present when the run used `--estimate`.
    #[serde(skip_serializing_if = "Option::is_none")]
    estimate: Option<EstimateJson>,
}

#[derive(Serialize)]
struct Root<'a> {
    tool: ToolInfo<'a>,
    models: Vec<String>,
    results: Vec<SkillResultJson>,
    code: Vec<CodeBlockJson>,
}

fn status_str(s: &SkillStatus) -> String {
    match s {
        SkillStatus::Ok => "OK".to_string(),
        SkillStatus::Invalid(_) => "INVALID".to_string(),
    }
}

fn skill_results(mr: &ModelReport) -> Vec<SkillResultJson> {
    mr.skills
        .iter()
        .map(|s| {
            let mut issues = s.issues.clone();
            if let SkillStatus::Invalid(reason) = &s.status {
                issues.push(reason.clone());
            }
            SkillResultJson {
                skill: s.skill.clone(),
                model: mr.model.clone(),
                l1: s.l1.unwrap_or(0.0),
                l2: s.l2.unwrap_or(0.0),
                l3_tokens: s.l3_tokens,
                l3_bytes: s.l3_bytes,
                files: s
                    .l3_files
                    .iter()
                    .map(|f| SkillFile {
                        path: f.rel_path.clone(),
                        tokens: f.tokens,
                    })
                    .collect(),
                status: status_str(&s.status),
                issues,
            }
        })
        .collect()
}

fn code_block(mr: &ModelReport) -> CodeBlockJson {
    CodeBlockJson {
        model: mr.model.clone(),
        approx: mr.approx,
        languages: mr
            .languages
            .iter()
            .map(|l| LangJson {
                language: l.language.clone(),
                files: l.files,
                lines: l.lines,
                bytes: l.bytes,
                tokens: l.tokens,
            })
            .collect(),
        files: mr
            .files
            .iter()
            .map(|f| CodeFileJson {
                path: f.rel_path.clone(),
                language: f.language.clone(),
                bytes: f.bytes,
                is_binary: f.is_binary,
                lines: f.lines,
                tokens: f.tokens,
                estimated: f.estimated,
            })
            .collect(),
        estimate: mr.estimate.as_ref().map(|e| EstimateJson {
            sampled_files: e.sampled_files,
            total_files: e.total_files,
            sampled_chars: e.sampled_chars,
            total_chars: e.total_chars,
            error_bound: e.error_bound,
        }),
    }
}

pub fn render(report: &Report) -> Result<String> {
    let root = Root {
        tool: ToolInfo {
            name: &report.tool_name,
            version: &report.tool_version,
        },
        models: report.models.clone(),
        results: report.reports.iter().flat_map(skill_results).collect(),
        code: report.reports.iter().map(code_block).collect(),
    };
    Ok(serde_json::to_string_pretty(&root)?)
}
