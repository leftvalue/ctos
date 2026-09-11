//! Table rendering. Supports two styles selectable via `--style`:
//!   * `boxed` (default): full box-drawing borders (comfy-table).
//!   * `plain`: cloc-style — no outer box, dashed rules between sections.
//!
//! Multi-model output is split into one block per model.

use comfy_table::{Cell, CellAlignment, ContentArrangement, Table};

use crate::cli::TableStyle;
use crate::model::{ModelReport, Report, SkillStatus};
use crate::util::{fmt_tokens, group_int, human_bytes};

use super::RenderOpts;

// ---------------------------------------------------------------------------
// A small style-agnostic grid: build rows once, render boxed or plain.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum Align {
    Left,
    Right,
}

enum Row {
    /// A data row. May have fewer cells than there are columns (ragged), in
    /// which case trailing columns are treated as empty / free text.
    Cells(Vec<String>),
    /// A horizontal rule (only drawn in plain style; boxed already separates).
    Rule,
}

struct Grid {
    headers: Vec<(String, Align)>,
    rows: Vec<Row>,
}

impl Grid {
    fn new(headers: Vec<(&str, Align)>) -> Self {
        Self {
            headers: headers
                .into_iter()
                .map(|(s, a)| (s.to_string(), a))
                .collect(),
            rows: Vec::new(),
        }
    }

    fn row<I: IntoIterator<Item = String>>(&mut self, cells: I) {
        self.rows.push(Row::Cells(cells.into_iter().collect()));
    }

    fn rule(&mut self) {
        self.rows.push(Row::Rule);
    }

    fn render(&self, style: TableStyle) -> String {
        match style {
            TableStyle::Boxed => self.render_boxed(),
            TableStyle::Plain => self.render_plain(),
        }
    }

    fn render_boxed(&self) -> String {
        let mut t = Table::new();
        t.load_preset(comfy_table::presets::UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic);
        let ncols = self.headers.len();
        t.set_header(self.headers.iter().map(|(h, a)| {
            let c = Cell::new(h);
            if *a == Align::Right {
                c.set_alignment(CellAlignment::Right)
            } else {
                c
            }
        }));
        for row in &self.rows {
            if let Row::Cells(cells) = row {
                let mut padded = cells.clone();
                padded.resize(ncols, String::new());
                t.add_row(padded.into_iter().enumerate().map(|(i, s)| {
                    let c = Cell::new(s);
                    if self.headers[i].1 == Align::Right {
                        c.set_alignment(CellAlignment::Right)
                    } else {
                        c
                    }
                }));
            }
            // Rule rows are implicit in the boxed preset.
        }
        t.to_string()
    }

    fn render_plain(&self) -> String {
        let ncols = self.headers.len();
        let mut widths: Vec<usize> = self.headers.iter().map(|(h, _)| chars(h)).collect();
        for row in &self.rows {
            if let Row::Cells(cells) = row {
                // Only full-width rows contribute to column widths; ragged rows
                // (e.g. an INVALID skill message) are free text and must not
                // stretch column 0.
                if cells.len() == ncols {
                    for (i, c) in cells.iter().enumerate() {
                        widths[i] = widths[i].max(chars(c));
                    }
                }
            }
        }
        let total = widths.iter().sum::<usize>() + 2 * (ncols.saturating_sub(1));
        let rule = "-".repeat(total);

        let mut out = String::new();
        // Top rule, header, header rule — cloc-style.
        out.push_str(&rule);
        out.push('\n');
        out.push_str(&fmt_row(
            &self
                .headers
                .iter()
                .map(|(h, _)| h.clone())
                .collect::<Vec<_>>(),
            &widths,
            &self.headers,
        ));
        out.push('\n');
        out.push_str(&rule);
        out.push('\n');
        for row in &self.rows {
            match row {
                Row::Cells(cells) => {
                    out.push_str(&fmt_row(cells, &widths, &self.headers));
                    out.push('\n');
                }
                Row::Rule => {
                    out.push_str(&rule);
                    out.push('\n');
                }
            }
        }
        out
    }
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

/// Format one plain-style row. Ragged rows (fewer cells than columns) put the
/// first cell in column 0 and join the remainder as free text after it.
fn fmt_row(cells: &[String], widths: &[usize], headers: &[(String, Align)]) -> String {
    let ncols = widths.len();
    if cells.len() < ncols && !cells.is_empty() {
        // First column aligned, rest as free text (e.g. INVALID message).
        let mut s = pad("", &cells[0], widths[0], Align::Left);
        let rest = cells[1..].join(" ");
        if !rest.is_empty() {
            s.push_str("  ");
            s.push_str(&rest);
        }
        return s;
    }
    let mut parts = Vec::with_capacity(ncols);
    for i in 0..ncols {
        let val = cells.get(i).map(String::as_str).unwrap_or("");
        parts.push(pad("", val, widths[i], headers[i].1));
    }
    parts.join("  ")
}

/// Pad `val` to `width` chars with the given alignment. `_unused` kept for
/// signature symmetry.
fn pad(_unused: &str, val: &str, width: usize, align: Align) -> String {
    let n = chars(val);
    let fill = width.saturating_sub(n);
    match align {
        Align::Left => format!("{val}{}", " ".repeat(fill)),
        Align::Right => format!("{}{val}", " ".repeat(fill)),
    }
}

// ---------------------------------------------------------------------------
// Header / banners.
// ---------------------------------------------------------------------------

fn header_line(report: &Report) -> String {
    let mut out = format!(
        "{} v{} — count tokens of skill\nroot: {}\n",
        report.tool_name,
        report.tool_version,
        report.root.display()
    );

    let text_files = report.files_scanned.saturating_sub(report.binary_files);
    out.push_str(&format!(
        "      {} files scanned.  ({} text, {} binary)\n",
        report.files_scanned, text_files, report.binary_files
    ));

    // cloc-style stats line: version, wall time, throughput.
    let secs = report.elapsed_secs.max(1e-9);
    let files_per_s = report.files_scanned as f64 / secs;
    let lines_per_s = report.total_lines as f64 / secs;
    out.push_str(&format!(
        "github.com/leftvalue/ctos v{}  T={:.2} s ({:.1} files/s, {:.1} lines/s)\n",
        report.tool_version, report.elapsed_secs, files_per_s, lines_per_s,
    ));
    out
}

fn model_banner(mr: &ModelReport) -> String {
    let approx = if mr.approx { " ~approx" } else { "" };
    format!("tokenizer: {} ({}){}", mr.model, mr.source_label, approx)
}

// ---------------------------------------------------------------------------
// Tables.
// ---------------------------------------------------------------------------

fn language_grid(mr: &ModelReport) -> Grid {
    let mut g = Grid::new(vec![
        ("Language", Align::Left),
        ("files", Align::Right),
        ("lines", Align::Right),
        ("bytes", Align::Right),
        ("tokens", Align::Right),
    ]);
    for l in &mr.languages {
        g.row([
            l.language.clone(),
            group_int(l.files as u64),
            group_int(l.lines),
            human_bytes(l.bytes),
            fmt_tokens(l.tokens, mr.approx),
        ]);
    }
    g.rule();
    let (files, lines, bytes, tokens) = mr.code_totals();
    g.row([
        "SUM".to_string(),
        group_int(files as u64),
        group_int(lines),
        human_bytes(bytes),
        fmt_tokens(tokens, mr.approx),
    ]);
    g
}

fn skill_grid(mr: &ModelReport) -> (Grid, f64, f64) {
    let mut g = Grid::new(vec![
        ("skill", Align::Left),
        ("L1", Align::Right),
        ("L2", Align::Right),
        ("L3(tok)", Align::Right),
        ("L3(bytes)", Align::Right),
        ("status", Align::Left),
    ]);

    let mut valid = 0usize;
    let (mut sum_l1, mut sum_l2, mut sum_l3_tok) = (0.0, 0.0, 0.0);
    let mut sum_l3_bytes = 0u64;

    for s in &mr.skills {
        match &s.status {
            SkillStatus::Invalid(reason) => {
                g.row([s.skill.clone(), format!("INVALID ({reason})")]);
            }
            status => {
                valid += 1;
                let l1 = s.l1.unwrap_or(0.0);
                let l2 = s.l2.unwrap_or(0.0);
                sum_l1 += l1;
                sum_l2 += l2;
                sum_l3_tok += s.l3_tokens;
                sum_l3_bytes += s.l3_bytes;
                g.row([
                    s.skill.clone(),
                    fmt_tokens(l1, mr.approx),
                    fmt_tokens(l2, mr.approx),
                    fmt_tokens(s.l3_tokens, mr.approx),
                    human_bytes(s.l3_bytes),
                    status.label(),
                ]);
            }
        }
    }

    g.rule();
    g.row([
        format!("TOTAL ({valid} valid skills)"),
        fmt_tokens(sum_l1, mr.approx),
        fmt_tokens(sum_l2, mr.approx),
        fmt_tokens(sum_l3_tok, mr.approx),
        human_bytes(sum_l3_bytes),
        String::new(),
    ]);

    (g, mr.resident_l1(), mr.peak_injection())
}

fn render_skill_section(mr: &ModelReport, style: TableStyle, verbose: bool) -> String {
    let (g, resident, peak) = skill_grid(mr);
    let mut out = g.render(style);
    out.push('\n');
    out.push_str(&format!(
        "resident L1 total: {} tok  →  peak injection: {} tok\n",
        fmt_tokens(resident, mr.approx),
        fmt_tokens(peak, mr.approx),
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
            out.push_str(&super::tree::render_by_file_tree(mr));
        } else {
            out.push_str(&language_grid(mr).render(opts.style));
        }
        out.push('\n');

        if !mr.skills.is_empty() {
            out.push_str("\nSkill layers (L1 metadata / L2 body / L3 assets):\n");
            out.push_str(&render_skill_section(mr, opts.style, opts.verbose));
        }
    }

    out
}
