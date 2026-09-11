//! Output dispatch (table / json / md / csv) and optional file write-out.

pub mod csv;
pub mod json;
pub mod md;
pub mod table;
pub mod tree;

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

use crate::cli::{Format, SortKey, TableStyle};
use crate::model::{LangStat, ModelReport, Report};

/// A parsed `--summary-cutoff <metric>:<N>[%]`.
#[derive(Debug, Clone, Copy)]
pub struct Cutoff {
    pub metric: CutoffMetric,
    pub value: f64,
    pub is_percent: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CutoffMetric {
    Tokens,
    Files,
    Lines,
    Bytes,
}

impl Cutoff {
    /// Parse `tokens:100`, `files:2`, `lines:5%`, etc.
    pub fn parse(s: &str) -> Result<Self> {
        let (metric_str, num_str) = s
            .split_once(':')
            .with_context(|| format!("expected <metric>:<N>, got {s:?}"))?;
        let metric = match metric_str.trim().to_ascii_lowercase().as_str() {
            "tokens" | "t" => CutoffMetric::Tokens,
            "files" | "f" => CutoffMetric::Files,
            "lines" | "l" => CutoffMetric::Lines,
            "bytes" | "b" => CutoffMetric::Bytes,
            other => anyhow::bail!("unknown cutoff metric {other:?} (tokens|files|lines|bytes)"),
        };
        let num_str = num_str.trim();
        let (is_percent, digits) = match num_str.strip_suffix('%') {
            Some(d) => (true, d.trim()),
            None => (false, num_str),
        };
        let value: f64 = digits
            .parse()
            .with_context(|| format!("invalid cutoff number {digits:?}"))?;
        Ok(Self {
            metric,
            value,
            is_percent,
        })
    }
}

/// Options that affect rendering.
#[derive(Debug, Clone, Copy)]
pub struct RenderOpts {
    pub verbose: bool,
    pub by_file: bool,
    pub by_file_by_lang: bool,
    pub quiet: bool,
    pub style: TableStyle,
    pub sort: SortKey,
    pub hide_rate: bool,
    pub summary_cutoff: Option<Cutoff>,
}

/// Return the language rows for a model report, after applying `--sort` and
/// `--summary-cutoff`. Shared by table/md/csv renderers.
pub fn processed_languages(mr: &ModelReport, opts: RenderOpts) -> Vec<LangStat> {
    let mut langs = mr.languages.clone();

    // Apply summary-cutoff: fold small languages into an "Other" row.
    if let Some(cut) = opts.summary_cutoff {
        let (_, total_lines, total_bytes, total_tokens) = mr.code_totals();
        let total_files: u64 = langs.iter().map(|l| l.files as u64).sum();
        let threshold = |metric_total: f64| {
            if cut.is_percent {
                metric_total * cut.value / 100.0
            } else {
                cut.value
            }
        };
        let metric_val = |l: &LangStat| -> f64 {
            match cut.metric {
                CutoffMetric::Tokens => l.tokens,
                CutoffMetric::Files => l.files as f64,
                CutoffMetric::Lines => l.lines as f64,
                CutoffMetric::Bytes => l.bytes as f64,
            }
        };
        let total = match cut.metric {
            CutoffMetric::Tokens => total_tokens,
            CutoffMetric::Files => total_files as f64,
            CutoffMetric::Lines => total_lines as f64,
            CutoffMetric::Bytes => total_bytes as f64,
        };
        let thr = threshold(total);

        let mut kept: Vec<LangStat> = Vec::new();
        let mut other = LangStat {
            language: "Other".to_string(),
            files: 0,
            lines: 0,
            bytes: 0,
            tokens: 0.0,
        };
        let mut folded = 0;
        for l in langs.into_iter() {
            if metric_val(&l) < thr {
                other.files += l.files;
                other.lines += l.lines;
                other.bytes += l.bytes;
                other.tokens += l.tokens;
                folded += 1;
            } else {
                kept.push(l);
            }
        }
        if folded > 0 {
            kept.push(other);
        }
        langs = kept;
    }

    // Sort, keeping any "Other" bucket last.
    let other = langs
        .iter()
        .position(|l| l.language == "Other")
        .map(|pos| langs.remove(pos));
    sort_langs(&mut langs, opts.sort);
    if let Some(other) = other {
        langs.push(other);
    }
    langs
}

fn sort_langs(langs: &mut [LangStat], sort: SortKey) {
    match sort {
        SortKey::Name => langs.sort_by(|a, b| a.language.cmp(&b.language)),
        SortKey::Files => {
            langs.sort_by(|a, b| b.files.cmp(&a.files).then(a.language.cmp(&b.language)))
        }
        SortKey::Lines => {
            langs.sort_by(|a, b| b.lines.cmp(&a.lines).then(a.language.cmp(&b.language)))
        }
        SortKey::Bytes => {
            langs.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(a.language.cmp(&b.language)))
        }
        SortKey::Tokens => langs.sort_by(|a, b| {
            b.tokens
                .partial_cmp(&a.tokens)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.language.cmp(&b.language))
        }),
    }
}

/// Render a report to the chosen format and write to stdout or a file.
pub fn emit(
    report: &Report,
    format: Format,
    output: Option<&Path>,
    opts: RenderOpts,
) -> Result<()> {
    let rendered = match format {
        Format::Table => table::render(report, opts),
        Format::Json => json::render(report)?,
        Format::Md => md::render(report, opts),
        Format::Csv => csv::render(report, opts),
    };
    write_out(&rendered, output)
}

pub fn write_out(text: &str, output: Option<&Path>) -> Result<()> {
    match output {
        Some(p) => {
            std::fs::write(p, text)
                .with_context(|| format!("failed to write output: {}", p.display()))?;
        }
        None => {
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            lock.write_all(text.as_bytes())?;
            if !text.ends_with('\n') {
                lock.write_all(b"\n")?;
            }
        }
    }
    Ok(())
}
