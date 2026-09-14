//! Orchestration: scan × registry -> unified Report.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::Result;

use crate::estimate::{self, SamplePlan};
use crate::model::{
    EstimateInfo, FileEntry, L3File, LangStat, ModelReport, Report, SkillResult, SkillStatus,
};
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
    // Per-model work in *bytes* of encoded text. Exact mode: every text file
    // once. Estimate mode: only the sampled slices (plus L1/L2, always exact).
    // Byte weighting keeps the bar and ETA honest when file sizes vary wildly.
    let noop = ProgressReporter::new(false);
    let progress = opts.progress.as_deref().unwrap_or(&noop);

    // The sampling plan is model-independent (depends only on file sizes,
    // languages and paths), so all models share the same sample set.
    let plans = if opts.estimate {
        Some(estimate::plan_sampling(
            &scanned.files,
            opts.sample_budget,
            estimate::SLICE_CAP,
        ))
    } else {
        None
    };

    let per_model_work: u64 =
        count_tokenize_work(&scanned.files, &scanned.skill_dirs, plans.as_deref());

    let mut reports = Vec::with_capacity(registry.models.len());
    for model in &registry.models {
        if per_model_work > 0 {
            progress.begin_tokenize(per_model_work, &model.name);
        }
        let report = compute_model_report(
            model,
            &scanned.files,
            &scanned.skill_dirs,
            progress,
            plans.as_deref(),
        )?;
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
        skipped_tokenizer_files: scanned.skipped_tokenizer_files,
        elapsed_secs: start.elapsed().as_secs_f64(),
    })
}

/// Bytes of text one model will encode for these files/skills. Exact mode:
/// every text file once (skill L3 assets reuse the main-loop counts). Estimate
/// mode: only the bytes of the sampled slices. L1/L2 texts always count fully.
fn count_tokenize_work(
    files: &[ScannedFile],
    skill_dirs: &[std::path::PathBuf],
    plans: Option<&[SamplePlan]>,
) -> u64 {
    let mut total: u64 = 0;

    for (i, f) in files.iter().enumerate() {
        let Some(text) = f.content.as_ref() else {
            continue;
        };
        let encoded = match plans {
            None => text.len(),
            Some(plans) => match plans[i] {
                SamplePlan::Full => text.len(),
                // Byte length of the first `n` chars (char-boundary safe).
                SamplePlan::Slice(n) => text
                    .char_indices()
                    .nth(n)
                    .map(|(b, _)| b)
                    .unwrap_or(text.len()),
                SamplePlan::Skip => 0,
            },
        };
        total += encoded as u64;
    }

    for dir in skill_dirs {
        let skill_md = files
            .iter()
            .find(|f| f.path == dir.join(SKILL_FILE))
            .and_then(|f| f.content.as_ref());
        if let Some(md) = skill_md {
            if let SkillParse::Valid {
                l1_text, l2_text, ..
            } = parse_skill_md(md)
            {
                total += l1_text.len() as u64 + l2_text.len() as u64;
            }
        }
    }

    total
}

fn compute_model_report(
    model: &ModelEntry,
    files: &[ScannedFile],
    skill_dirs: &[std::path::PathBuf],
    progress: &ProgressReporter,
    plans: Option<&[SamplePlan]>,
) -> Result<ModelReport> {
    let tok = &model.tokenizer;
    let approx = tok.approx();

    // ---- Phase A: per-file token values ----
    // Exact mode: encode every text file once (identical to the original path).
    // Estimate mode: encode only sampled slices (pass 1), then fill skipped
    // files from per-language ratios (pass 2).
    let mut file_tokens: Vec<Option<f64>> = Vec::with_capacity(files.len());
    let mut file_estimated = vec![false; files.len()];
    let mut lang_samples: BTreeMap<&str, Vec<estimate::Sample>> = BTreeMap::new();

    match plans {
        None => {
            for f in files {
                let t = match &f.content {
                    Some(text) => {
                        // Announce the file *before* the (potentially slow) encode so
                        // a huge file is visible in the bar while being chewed on.
                        progress.show_file(&f.rel_path);
                        let n = tok.count(text)?;
                        progress.tick_bytes(text.len() as u64);
                        Some(n)
                    }
                    None => None,
                };
                file_tokens.push(t);
            }
        }
        Some(plans) => {
            // Pass 1: encode the sampled files.
            for (i, f) in files.iter().enumerate() {
                let Some(text) = f.content.as_ref() else {
                    file_tokens.push(None);
                    continue;
                };
                let full_chars = text.chars().count();
                match plans[i] {
                    SamplePlan::Full => {
                        progress.show_file(&f.rel_path);
                        let n = tok.count(text)?;
                        progress.tick_bytes(text.len() as u64);
                        lang_samples
                            .entry(f.language.as_str())
                            .or_default()
                            .push((full_chars as u64, n));
                        file_tokens.push(Some(n));
                    }
                    SamplePlan::Slice(n) => {
                        progress.show_file(&f.rel_path);
                        let head: String = text.chars().take(n).collect();
                        let head_chars = head.chars().count();
                        let head_tok = tok.count(&head)?;
                        progress.tick_bytes(head.len() as u64);
                        // Extrapolate by the file's own head ratio (within-file
                        // homogeneity beats cross-file).
                        let est = head_tok * (full_chars as f64 / head_chars.max(1) as f64);
                        lang_samples
                            .entry(f.language.as_str())
                            .or_default()
                            .push((head_chars as u64, head_tok));
                        file_tokens.push(Some(est));
                        file_estimated[i] = true;
                    }
                    SamplePlan::Skip => {
                        file_tokens.push(None); // placeholder, filled in pass 2
                    }
                }
            }
            // Pass 2: estimate skipped files from per-language ratios.
            let ratios: BTreeMap<&str, f64> = lang_samples
                .iter()
                .map(|(l, s)| (*l, estimate::lang_ratio(s)))
                .collect();
            for (i, f) in files.iter().enumerate() {
                if file_tokens[i].is_none() {
                    if let Some(text) = f.content.as_ref() {
                        let chars = text.chars().count();
                        let r = ratios.get(f.language.as_str()).copied().unwrap_or(0.0);
                        file_tokens[i] = Some(r * chars as f64);
                        file_estimated[i] = true;
                    }
                }
            }
        }
    }

    // ---- Phase B: aggregation (shared by both modes) ----
    let mut file_entries = Vec::with_capacity(files.len());
    let mut lang_map: BTreeMap<String, LangStat> = BTreeMap::new();

    for (i, f) in files.iter().enumerate() {
        let tokens = file_tokens[i];
        let entry = FileEntry {
            rel_path: f.rel_path.clone(),
            language: f.language.clone(),
            bytes: f.bytes,
            is_binary: f.is_binary,
            lines: f.lines,
            tokens,
            estimated: file_estimated[i],
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

    // Path -> token count from the main loop, so the skill L3 pass reuses the
    // already-encoded values instead of re-tokenizing every asset a second time.
    let token_of: std::collections::HashMap<&std::path::Path, f64> = files
        .iter()
        .zip(file_entries.iter())
        .filter_map(|(f, e)| e.tokens.map(|t| (f.path.as_path(), t)))
        .collect();

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
        // Token counts are reused from the main loop (encode once).
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
            let t = token_of.get(f.path.as_path()).copied().unwrap_or(0.0);
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
                progress.tick_bytes((l1_text.len() + l2_text.len()) as u64);
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

    // ---- Estimate metadata (estimate mode only) ----
    let estimate_info = plans.map(|plans| {
        let mut sampled_files = 0usize;
        let mut total_files = 0usize;
        let mut sampled_chars = 0u64;
        let mut total_chars = 0u64;
        // Per-language chars that were NOT exactly encoded (Skip: all chars;
        // Slice: the extrapolated tail). The error bound only comes from these.
        let mut lang_estimated_chars: BTreeMap<&str, u64> = BTreeMap::new();

        for (i, f) in files.iter().enumerate() {
            let Some(text) = f.content.as_ref() else {
                continue;
            };
            total_files += 1;
            let chars = text.chars().count() as u64;
            total_chars += chars;
            let encoded = plans[i].encoded_chars(chars as usize) as u64;
            if plans[i].is_sampled() {
                sampled_files += 1;
                sampled_chars += encoded;
            }
            *lang_estimated_chars.entry(f.language.as_str()).or_insert(0) += chars - encoded;
        }

        let bound_entries: Vec<(u64, &[estimate::Sample])> = lang_samples
            .iter()
            .map(|(l, s)| {
                (
                    lang_estimated_chars.get(l).copied().unwrap_or(0),
                    s.as_slice(),
                )
            })
            .collect();
        let error_bound = estimate::error_bound(&bound_entries, total_chars);

        EstimateInfo {
            sampled_files,
            total_files,
            sampled_chars,
            total_chars,
            error_bound,
        }
    });

    Ok(ModelReport {
        model: model.name.clone(),
        source_label: tok.source_label().to_string(),
        approx,
        languages,
        files: file_entries,
        skills,
        estimate: estimate_info,
    })
}
