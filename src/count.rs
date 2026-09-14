//! Orchestration: scan × registry -> unified Report.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;

use crate::model::{FileEntry, L3File, LangStat, ModelReport, Report, SkillResult, SkillStatus};
use crate::progress::ProgressReporter;
use crate::skill::{parse_skill_md, SkillParse};
use crate::tokenizer::{ModelEntry, Registry};
use crate::walk::{scan_many, ScanOpts, ScannedFile};

const SKILL_FILE: &str = "SKILL.md";

/// Compute the full report for one or more paths across all models.
pub fn run(paths: &[PathBuf], registry: &Registry, opts: &ScanOpts) -> Result<Report> {
    let start = Instant::now();
    let scanned = scan_many(paths, opts)?;

    let files_scanned = scanned.files.len();
    let binary_files = scanned.files.iter().filter(|f| f.is_binary).count();
    let total_lines: u64 = scanned.files.iter().filter_map(|f| f.lines).sum();

    // ---- Progress phase 2 setup (tokenize) ----
    // Exact per-model work: one tok.count per text file, one per L3 text file,
    // plus two (L1/L2) per skill whose SKILL.md parses valid.
    let noop = ProgressReporter::new(false);
    let progress = opts.progress.as_deref().unwrap_or(&noop);

    let per_model_work: u64 = count_tokenize_work(&scanned.files, &scanned.skill_dirs);

    let mut reports = Vec::with_capacity(registry.models.len());
    for model in &registry.models {
        progress.begin_tokenize(per_model_work, &model.name);
        let report = compute_model_report(model, &scanned.files, &scanned.skill_dirs, progress)?;
        reports.push(report);
    }
    progress.finish();

    Ok(Report {
        tool_name: "ctos".to_string(),
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        roots: scanned.roots,
        models: registry.models.iter().map(|m| m.name.clone()).collect(),
        reports,
        files_scanned,
        binary_files,
        total_lines,
        elapsed_secs: start.elapsed().as_secs_f64(),
    })
}

/// Number of tok.count calls one model will make for these files/skills.
fn count_tokenize_work(files: &[ScannedFile], skill_dirs: &[std::path::PathBuf]) -> u64 {
    let text_files = files.iter().filter(|f| f.content.is_some()).count() as u64;

    let mut l3_text = 0u64;
    let mut valid_skills = 0u64;
    for dir in skill_dirs {
        let skill_md = files
            .iter()
            .find(|f| f.path == dir.join(SKILL_FILE))
            .and_then(|f| f.content.as_ref());
        if let Some(md) = skill_md {
            if matches!(parse_skill_md(md), SkillParse::Valid { .. }) {
                valid_skills += 1;
            }
        }
        for f in files {
            if f.path.starts_with(dir) && f.path != dir.join(SKILL_FILE) && f.content.is_some() {
                l3_text += 1;
            }
        }
    }

    text_files + l3_text + 2 * valid_skills
}

fn compute_model_report(
    model: &ModelEntry,
    files: &[ScannedFile],
    skill_dirs: &[std::path::PathBuf],
    progress: &ProgressReporter,
) -> Result<ModelReport> {
    let tok = &model.tokenizer;
    let approx = tok.approx();

    // ---- Per-file code counting + language aggregation ----
    let mut file_entries = Vec::with_capacity(files.len());
    let mut lang_map: BTreeMap<String, LangStat> = BTreeMap::new();

    for f in files {
        let tokens = match &f.content {
            Some(text) => {
                let n = tok.count(text)?;
                progress.tick_tokenize();
                Some(n)
            }
            None => None,
        };
        let entry = FileEntry {
            rel_path: f.rel_path.clone(),
            language: f.language.clone(),
            bytes: f.bytes,
            is_binary: f.is_binary,
            lines: f.lines,
            tokens,
        };
        let ls = lang_map
            .entry(f.language.clone())
            .or_insert_with(|| LangStat {
                language: f.language.clone(),
                files: 0,
                lines: 0,
                bytes: 0,
                tokens: 0.0,
            });
        ls.files += 1;
        ls.lines += f.lines.unwrap_or(0);
        ls.bytes += f.bytes;
        ls.tokens += tokens.unwrap_or(0.0);
        file_entries.push(entry);
    }

    let mut languages: Vec<LangStat> = lang_map.into_values().collect();
    // Sort by tokens desc, then language name for stability.
    languages.sort_by(|a, b| {
        b.tokens
            .partial_cmp(&a.tokens)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.language.cmp(&b.language))
    });

    // ---- Skill three-layer analysis ----
    let mut skills = Vec::new();
    for dir in skill_dirs {
        let skill_name = dir
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| dir.to_string_lossy().into_owned());
        let rel_path = files
            .iter()
            .find(|f| f.path == dir.join(SKILL_FILE))
            .map(|f| {
                f.rel_path
                    .trim_end_matches(SKILL_FILE)
                    .trim_end_matches('/')
                    .to_string()
            })
            .unwrap_or_else(|| skill_name.clone());

        // The SKILL.md content.
        let skill_md = files.iter().find(|f| f.path == dir.join(SKILL_FILE));
        let skill_md = match skill_md.and_then(|f| f.content.as_ref()) {
            Some(c) => c,
            None => {
                skills.push(SkillResult {
                    skill: skill_name,
                    rel_path,
                    l1: None,
                    l2: None,
                    l3_tokens: 0.0,
                    l3_bytes: 0,
                    l3_files: vec![],
                    status: SkillStatus::Invalid("SKILL.md unreadable".to_string()),
                    growth_ok: false,
                    issues: vec![],
                });
                continue;
            }
        };

        let parsed = parse_skill_md(skill_md);

        // L3 = every file within the skill dir except its own SKILL.md.
        let mut l3_files = Vec::new();
        let mut l3_tokens = 0.0;
        let mut l3_bytes = 0u64;
        for f in files {
            if !f.path.starts_with(dir) {
                continue;
            }
            if f.path == dir.join(SKILL_FILE) {
                continue;
            }
            let t = match &f.content {
                Some(text) => {
                    let n = tok.count(text)?;
                    progress.tick_tokenize();
                    n
                }
                None => 0.0,
            };
            l3_tokens += t;
            l3_bytes += f.bytes;
            l3_files.push(L3File {
                rel_path: f.rel_path.clone(),
                tokens: t,
                bytes: f.bytes,
                is_binary: f.is_binary,
            });
        }

        match parsed {
            SkillParse::Valid {
                l1_text,
                l2_text,
                meta,
            } => {
                let l1 = tok.count(&l1_text)? + model.overhead_l1;
                let l2 = tok.count(&l2_text)?;
                progress.tick_tokenize();
                progress.tick_tokenize();
                skills.push(SkillResult {
                    skill: skill_name,
                    rel_path,
                    l1: Some(l1),
                    l2: Some(l2),
                    l3_tokens,
                    l3_bytes,
                    l3_files,
                    status: SkillStatus::Ok,
                    growth_ok: meta.growth_ok,
                    issues: vec![],
                });
            }
            SkillParse::Invalid(reason) => {
                skills.push(SkillResult {
                    skill: skill_name,
                    rel_path,
                    l1: None,
                    l2: None,
                    l3_tokens,
                    l3_bytes,
                    l3_files,
                    status: SkillStatus::Invalid(reason),
                    growth_ok: false,
                    issues: vec![],
                });
            }
        }
    }

    skills.sort_by(|a, b| a.skill.cmp(&b.skill));

    Ok(ModelReport {
        model: model.name.clone(),
        source_label: tok.source_label().to_string(),
        approx,
        languages,
        files: file_entries,
        skills,
    })
}
