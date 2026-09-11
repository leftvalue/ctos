//! Markdown output: GitHub-compatible pipe tables, one block per model.

use crate::model::{ModelReport, Report};
use crate::util::{fmt_tokens, group_int, human_bytes};

use super::RenderOpts;

fn md_escape(s: &str) -> String {
    s.replace('|', "\\|")
}

fn lang_table(mr: &ModelReport, opts: RenderOpts) -> String {
    let mut out = String::new();
    out.push_str("| Language | files | lines | bytes | tokens |\n");
    out.push_str("|---|--:|--:|--:|--:|\n");
    for l in &super::processed_languages(mr, opts) {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            md_escape(&l.language),
            group_int(l.files as u64),
            group_int(l.lines),
            human_bytes(l.bytes),
            fmt_tokens(l.tokens, mr.approx),
        ));
    }
    let (files, lines, bytes, tokens) = mr.code_totals();
    out.push_str(&format!(
        "| **SUM** | **{}** | **{}** | **{}** | **{}** |\n",
        group_int(files as u64),
        group_int(lines),
        human_bytes(bytes),
        fmt_tokens(tokens, mr.approx),
    ));
    out
}

fn file_table(mr: &ModelReport) -> String {
    let mut out = String::new();
    out.push_str("| File | Language | lines | bytes | tokens |\n");
    out.push_str("|---|---|--:|--:|--:|\n");
    for f in &mr.files {
        let lines = f.lines.map(group_int).unwrap_or_else(|| "-".to_string());
        let tok = match f.tokens {
            Some(v) => fmt_tokens(v, mr.approx),
            None => "-".to_string(),
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            md_escape(&f.rel_path),
            md_escape(&f.language),
            lines,
            human_bytes(f.bytes),
            tok,
        ));
    }
    out
}

fn skill_table(mr: &ModelReport) -> String {
    let mut out = String::new();
    out.push_str("| skill | L1 | L2 | L3(tok) | L3(bytes) | status |\n");
    out.push_str("|---|--:|--:|--:|--:|---|\n");
    for s in &mr.skills {
        let status = s.status.label();
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            md_escape(&s.skill),
            s.l1.map(|v| fmt_tokens(v, mr.approx))
                .unwrap_or_else(|| "-".into()),
            s.l2.map(|v| fmt_tokens(v, mr.approx))
                .unwrap_or_else(|| "-".into()),
            fmt_tokens(s.l3_tokens, mr.approx),
            human_bytes(s.l3_bytes),
            md_escape(&status),
        ));
    }
    out
}

pub fn render(report: &Report, opts: RenderOpts) -> String {
    let mut out = String::new();
    if !opts.quiet {
        out.push_str(&format!(
            "# {} v{}\n\n",
            report.tool_name, report.tool_version
        ));
        let text_files = report.files_scanned.saturating_sub(report.binary_files);
        out.push_str(&format!(
            "{} files scanned ({} text, {} binary).\n\n",
            report.files_scanned, text_files, report.binary_files
        ));
    }

    for mr in &report.reports {
        let approx = if mr.approx { " (~approx)" } else { "" };
        out.push_str(&format!("## tokenizer: {}{}\n\n", mr.model, approx));

        if opts.by_file && !opts.by_file_by_lang {
            out.push_str(&file_table(mr));
        } else if opts.by_file_by_lang {
            out.push_str(&lang_table(mr, opts));
            out.push_str("\n### by file\n\n");
            out.push_str(&file_table(mr));
        } else {
            out.push_str(&lang_table(mr, opts));
        }
        out.push('\n');

        if !mr.skills.is_empty() {
            out.push_str("### Skill layers\n\n");
            out.push_str(&skill_table(mr));
            out.push_str(&format!(
                "\nresident L1 total: {} tok · peak injection: {} tok\n\n",
                fmt_tokens(mr.resident_l1(), mr.approx),
                fmt_tokens(mr.peak_injection(), mr.approx),
            ));
        }
    }

    out
}
