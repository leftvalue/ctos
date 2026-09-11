//! Budget gate: validate skills against budgets.toml, optionally diff a baseline.
//!
//! Exit-code contract (spec §6.3):
//!   0  all pass
//!   1  some skill over budget / growth over / INVALID
//!   2  runtime error (handled by main via anyhow)

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::config::BudgetsConfig;
use crate::model::{Report, SkillStatus};
use crate::util::fmt_tokens;

/// Relaxation factor applied to budgets for approximate tokenizers.
const APPROX_SLACK: f64 = 1.1;

/// One line of the check report.
#[derive(Debug, Clone)]
pub struct CheckRow {
    pub model: String,
    pub skill: String,
    pub l1: Option<f64>,
    pub l2: Option<f64>,
    pub baseline_l2: Option<f64>,
    pub growth: Option<f64>,
    pub verdict: String,
}

#[derive(Debug, Clone)]
pub struct CheckOutcome {
    pub rows: Vec<CheckRow>,
    /// True when any skill failed or is invalid, or L1 total exceeded.
    pub failed: bool,
    pub notes: Vec<String>,
}

// ---- Baseline (JSON produced by `ctos --format json`) ----

#[derive(Debug, Deserialize)]
struct BaselineRoot {
    #[serde(default)]
    results: Vec<BaselineResult>,
}

#[derive(Debug, Deserialize)]
struct BaselineResult {
    skill: String,
    model: String,
    #[serde(default)]
    l2: f64,
}

fn load_baseline(path: &Path) -> Result<HashMap<(String, String), f64>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read baseline: {}", path.display()))?;
    let root: BaselineRoot =
        serde_json::from_str(&text).context("failed to parse baseline JSON")?;
    let mut map = HashMap::new();
    for r in root.results {
        map.insert((r.model, r.skill), r.l2);
    }
    Ok(map)
}

pub fn evaluate(
    report: &Report,
    budgets: &BudgetsConfig,
    baseline: Option<&Path>,
) -> Result<CheckOutcome> {
    let g = &budgets.global;
    let baseline_map = match baseline {
        Some(p) => Some(load_baseline(p)?),
        None => None,
    };

    let mut rows = Vec::new();
    let mut notes = Vec::new();
    let mut failed = false;

    for mr in &report.reports {
        let slack = if mr.approx { APPROX_SLACK } else { 1.0 };
        let l1_max = g.l1_max * slack;
        let l2_max = g.l2_max * slack;
        let l1_total_max = g.l1_total_max * slack;

        let mut l1_total = 0.0;

        for s in &mr.skills {
            let mut tags: Vec<String> = Vec::new();
            let mut row_failed = false;

            if s.status.is_invalid() {
                let reason = match &s.status {
                    SkillStatus::Invalid(r) => r.clone(),
                    _ => "invalid".to_string(),
                };
                rows.push(CheckRow {
                    model: mr.model.clone(),
                    skill: s.skill.clone(),
                    l1: None,
                    l2: None,
                    baseline_l2: None,
                    growth: None,
                    verdict: format!("INVALID ({reason})"),
                });
                failed = true;
                continue;
            }

            let l1 = s.l1.unwrap_or(0.0);
            let l2 = s.l2.unwrap_or(0.0);
            l1_total += l1;

            if l1 > l1_max {
                tags.push("L1_OVER".to_string());
                row_failed = true;
            }
            if l2 > l2_max {
                tags.push("L2_OVER".to_string());
                row_failed = true;
            }

            // Growth diff vs baseline.
            let mut baseline_l2 = None;
            let mut growth = None;
            if let Some(map) = &baseline_map {
                if let Some(&base) = map.get(&(mr.model.clone(), s.skill.clone())) {
                    baseline_l2 = Some(base);
                    if base > 0.0 {
                        let ratio = (l2 - base) / base;
                        growth = Some(ratio);
                        if ratio > g.diff_ratio && !s.growth_ok {
                            tags.push("GROWTH_OVER".to_string());
                            row_failed = true;
                        }
                    }
                }
            }

            if row_failed {
                failed = true;
            }
            let verdict = if tags.is_empty() {
                "OK".to_string()
            } else {
                tags.join(",")
            };

            rows.push(CheckRow {
                model: mr.model.clone(),
                skill: s.skill.clone(),
                l1: Some(l1),
                l2: Some(l2),
                baseline_l2,
                growth,
                verdict,
            });
        }

        if l1_total > l1_total_max {
            failed = true;
            notes.push(format!(
                "[{}] resident L1 total {} exceeds l1_total_max {}",
                mr.model,
                fmt_tokens(l1_total, mr.approx),
                fmt_tokens(l1_total_max, mr.approx),
            ));
        }
    }

    Ok(CheckOutcome {
        rows,
        failed,
        notes,
    })
}

/// Render the check outcome as a text report aligned by skill.
pub fn render_report(outcome: &CheckOutcome, has_baseline: bool) -> String {
    let mut out = String::new();
    out.push_str("ctos check — skill budget gate\n\n");

    let header = if has_baseline {
        format!(
            "{:<24} {:<14} {:>8} {:>8} {:>10} {:>8}  {}\n",
            "skill", "model", "L1", "L2", "baseL2", "growth", "verdict"
        )
    } else {
        format!(
            "{:<24} {:<14} {:>8} {:>8}  {}\n",
            "skill", "model", "L1", "L2", "verdict"
        )
    };
    out.push_str(&header);
    out.push_str(&"-".repeat(header.len().min(100)));
    out.push('\n');

    for r in &outcome.rows {
        let l1 =
            r.l1.map(|v| fmt_tokens(v, false))
                .unwrap_or_else(|| "-".into());
        let l2 =
            r.l2.map(|v| fmt_tokens(v, false))
                .unwrap_or_else(|| "-".into());
        if has_baseline {
            let base = r
                .baseline_l2
                .map(|v| fmt_tokens(v, false))
                .unwrap_or_else(|| "-".into());
            let growth = r
                .growth
                .map(|v| format!("{:+.1}%", v * 100.0))
                .unwrap_or_else(|| "-".into());
            out.push_str(&format!(
                "{:<24} {:<14} {:>8} {:>8} {:>10} {:>8}  {}\n",
                r.skill, r.model, l1, l2, base, growth, r.verdict
            ));
        } else {
            out.push_str(&format!(
                "{:<24} {:<14} {:>8} {:>8}  {}\n",
                r.skill, r.model, l1, l2, r.verdict
            ));
        }
    }

    for note in &outcome.notes {
        out.push_str(&format!("\n! {note}"));
    }
    if !outcome.notes.is_empty() {
        out.push('\n');
    }

    out.push_str(&format!(
        "\nresult: {}\n",
        if outcome.failed { "FAIL" } else { "PASS" }
    ));
    out
}
