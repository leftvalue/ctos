//! CSV output (hand-written escaping, no external crate).
//!
//! A leading `scope` column distinguishes row types:
//!   * `lang` — one row per language aggregation
//!   * `file` — one row per file (only with --by-file / --by-file-by-lang)
//!   * `skill` — one row per skill (three-layer)
//!
//! Columns: scope,model,name,language,files,lines,bytes,tokens,status

use crate::model::{Report, SkillStatus};

use super::RenderOpts;

/// Quote a field if it contains comma, quote, CR or LF; double embedded quotes.
fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn num(n: u64) -> String {
    n.to_string()
}

fn tok(v: f64) -> String {
    (v.round() as u64).to_string()
}

fn row(cells: &[String]) -> String {
    let mut line = cells.iter().map(|c| field(c)).collect::<Vec<_>>().join(",");
    line.push('\n');
    line
}

pub fn render(report: &Report, opts: RenderOpts) -> String {
    let mut out = String::new();
    // Header.
    out.push_str(&row(&[
        "scope".into(),
        "model".into(),
        "name".into(),
        "language".into(),
        "files".into(),
        "lines".into(),
        "bytes".into(),
        "tokens".into(),
        "status".into(),
    ]));

    for mr in &report.reports {
        // Language rows (respect sort + summary-cutoff).
        for l in &super::processed_languages(mr, opts) {
            out.push_str(&row(&[
                "lang".into(),
                mr.model.clone(),
                l.language.clone(),
                l.language.clone(),
                num(l.files as u64),
                num(l.lines),
                num(l.bytes),
                tok(l.tokens),
                String::new(),
            ]));
        }

        // File rows when a per-file view was requested.
        if opts.by_file || opts.by_file_by_lang {
            for f in &mr.files {
                out.push_str(&row(&[
                    "file".into(),
                    mr.model.clone(),
                    f.rel_path.clone(),
                    f.language.clone(),
                    String::new(),
                    f.lines.map(num).unwrap_or_default(),
                    num(f.bytes),
                    f.tokens.map(tok).unwrap_or_default(),
                    if f.is_binary {
                        "binary".into()
                    } else {
                        String::new()
                    },
                ]));
            }
        }

        // Skill rows.
        for s in &mr.skills {
            let status = match &s.status {
                SkillStatus::Ok => "OK".to_string(),
                SkillStatus::Invalid(r) => format!("INVALID: {r}"),
            };
            out.push_str(&row(&[
                "skill".into(),
                mr.model.clone(),
                s.skill.clone(),
                String::new(),
                String::new(),
                String::new(),
                num(s.l3_bytes),
                s.l1.map(tok).unwrap_or_default(),
                status,
            ]));
        }
    }

    out
}
