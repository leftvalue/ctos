//! Table rendering (comfy-table, cloc-style). Multi-model output is split into
//! one block per model.

use comfy_table::{Cell, CellAlignment, ContentArrangement, Table};

use crate::model::{ModelReport, Report, SkillStatus};
use crate::util::{fmt_tokens, group_int, human_bytes};

use super::RenderOpts;

fn preset_table() -> Table {
    let mut t = Table::new();
    t.load_preset(comfy_table::presets::UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);
    t
}

fn right(s: impl Into<String>) -> Cell {
    Cell::new(s.into()).set_alignment(CellAlignment::Right)
}

fn header_line(report: &Report) -> String {
    format!(
        "{} v{} — count tokens of skill\nroot: {}\n",
        report.tool_name,
        report.tool_version,
        report.root.display()
    )
}

fn model_banner(mr: &ModelReport) -> String {
    let approx = if mr.approx { " ~approx" } else { "" };
    format!("tokenizer: {} ({}){}", mr.model, mr.source_label, approx)
}

fn render_language_table(mr: &ModelReport) -> String {
    let mut t = preset_table();
    t.set_header(vec![
        Cell::new("Language"),
        right("files"),
        right("bytes"),
        right("tokens"),
    ]);
    for l in &mr.languages {
        t.add_row(vec![
            Cell::new(&l.language),
            right(group_int(l.files as u64)),
            right(human_bytes(l.bytes)),
            right(fmt_tokens(l.tokens, mr.approx)),
        ]);
    }
    let (files, bytes, tokens) = mr.code_totals();
    t.add_row(vec![
        Cell::new("SUM"),
        right(group_int(files as u64)),
        right(human_bytes(bytes)),
        right(fmt_tokens(tokens, mr.approx)),
    ]);
    t.to_string()
}

fn render_by_file_table(mr: &ModelReport) -> String {
    let mut t = preset_table();
    t.set_header(vec![
        Cell::new("File"),
        Cell::new("Language"),
        right("bytes"),
        right("tokens"),
    ]);
    for f in &mr.files {
        let tok = match f.tokens {
            Some(v) => fmt_tokens(v, mr.approx),
            None => "-".to_string(),
        };
        t.add_row(vec![
            Cell::new(&f.rel_path),
            Cell::new(&f.language),
            right(human_bytes(f.bytes)),
            right(tok),
        ]);
    }
    t.to_string()
}

fn render_skill_table(mr: &ModelReport, verbose: bool) -> String {
    let mut out = String::new();
    let mut t = preset_table();
    t.set_header(vec![
        Cell::new("skill"),
        right("L1"),
        right("L2"),
        right("L3(tok)"),
        right("L3(bytes)"),
        Cell::new("status"),
    ]);

    let mut valid = 0usize;
    let mut sum_l1 = 0.0;
    let mut sum_l2 = 0.0;
    let mut sum_l3_tok = 0.0;
    let mut sum_l3_bytes = 0u64;

    for s in &mr.skills {
        match &s.status {
            SkillStatus::Invalid(reason) => {
                t.add_row(vec![
                    Cell::new(&s.skill),
                    Cell::new(format!("INVALID ({reason})")).set_alignment(CellAlignment::Left),
                ]);
            }
            status => {
                valid += 1;
                let l1 = s.l1.unwrap_or(0.0);
                let l2 = s.l2.unwrap_or(0.0);
                sum_l1 += l1;
                sum_l2 += l2;
                sum_l3_tok += s.l3_tokens;
                sum_l3_bytes += s.l3_bytes;
                t.add_row(vec![
                    Cell::new(&s.skill),
                    right(fmt_tokens(l1, mr.approx)),
                    right(fmt_tokens(l2, mr.approx)),
                    right(fmt_tokens(s.l3_tokens, mr.approx)),
                    right(human_bytes(s.l3_bytes)),
                    Cell::new(status.label()),
                ]);
            }
        }
    }

    t.add_row(vec![
        Cell::new(format!("TOTAL ({valid} valid skills)")),
        right(fmt_tokens(sum_l1, mr.approx)),
        right(fmt_tokens(sum_l2, mr.approx)),
        right(fmt_tokens(sum_l3_tok, mr.approx)),
        right(human_bytes(sum_l3_bytes)),
        Cell::new(""),
    ]);

    out.push_str(&t.to_string());
    out.push('\n');
    out.push_str(&format!(
        "resident L1 total: {} tok  →  peak injection: {} tok\n",
        fmt_tokens(mr.resident_l1(), mr.approx),
        fmt_tokens(mr.peak_injection(), mr.approx),
    ));

    if verbose {
        for s in &mr.skills {
            if s.l3_files.is_empty() {
                continue;
            }
            out.push_str(&format!("\n  {} L3 files:\n", s.skill));
            for f in &s.l3_files {
                let tok = if f.is_binary {
                    "- (binary)".to_string()
                } else {
                    fmt_tokens(f.tokens, mr.approx)
                };
                out.push_str(&format!(
                    "    {:<50} {:>10}  {}\n",
                    f.rel_path,
                    tok,
                    human_bytes(f.bytes)
                ));
            }
        }
    }

    out
}

pub fn render(report: &Report, opts: RenderOpts) -> String {
    let mut out = String::new();
    if !opts.quiet {
        out.push_str(&header_line(report));
    }

    for mr in &report.reports {
        out.push('\n');
        out.push_str(&model_banner(mr));
        out.push('\n');

        if opts.by_file {
            out.push_str(&render_by_file_table(mr));
        } else {
            out.push_str(&render_language_table(mr));
        }
        out.push('\n');

        if !mr.skills.is_empty() {
            out.push_str("\nSkill layers (L1 metadata / L2 body / L3 assets):\n");
            out.push_str(&render_skill_table(mr, opts.verbose));
        }
    }

    out
}
