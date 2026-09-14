//! Tree rendering for the `--by-file` view.
//!
//! Instead of a flat list of long paths, files are shown in a `tree(1)`-style
//! hierarchy with right-aligned metric columns (lines / bytes / tokens) so the
//! structure is easy to scan.

use std::collections::BTreeMap;

use crate::model::{FileEntry, ModelReport};
use crate::util::{fmt_tokens, group_int, human_bytes};

/// A directory node in the file tree.
#[derive(Default)]
struct Dir<'a> {
    subdirs: BTreeMap<String, Dir<'a>>,
    files: BTreeMap<String, &'a FileEntry>,
}

impl<'a> Dir<'a> {
    fn insert(&mut self, components: &[&str], entry: &'a FileEntry) {
        match components {
            [] => {}
            [name] => {
                self.files.insert((*name).to_string(), entry);
            }
            [head, tail @ ..] => {
                self.subdirs
                    .entry((*head).to_string())
                    .or_default()
                    .insert(tail, entry);
            }
        }
    }
}

/// One rendered row: the tree label (with connectors) plus optional metrics.
struct Row {
    label: String,
    metrics: Option<(String, String, String, String)>, // language, lines, bytes, tokens
}

fn metrics_for(entry: &FileEntry, approx: bool) -> (String, String, String, String) {
    let lines = entry
        .lines
        .map(group_int)
        .unwrap_or_else(|| "-".to_string());
    let tokens = match entry.tokens {
        // In estimate mode, only actually-estimated files get the `~` prefix.
        Some(v) => fmt_tokens(v, approx || entry.estimated),
        None => "-".to_string(),
    };
    (
        entry.language.clone(),
        lines,
        human_bytes(entry.bytes),
        tokens,
    )
}

fn walk(dir: &Dir, prefix: &str, approx: bool, rows: &mut Vec<Row>) {
    // Directories first, then files; each group alphabetical (BTreeMap order).
    let n_dirs = dir.subdirs.len();
    let n_files = dir.files.len();
    let total = n_dirs + n_files;

    let mut idx = 0;
    for (name, sub) in &dir.subdirs {
        let last = idx == total - 1;
        let connector = if last { "└── " } else { "├── " };
        rows.push(Row {
            label: format!("{prefix}{connector}{name}/"),
            metrics: None,
        });
        let child_prefix = format!("{prefix}{}", if last { "    " } else { "│   " });
        walk(sub, &child_prefix, approx, rows);
        idx += 1;
    }
    for (name, entry) in &dir.files {
        let last = idx == total - 1;
        let connector = if last { "└── " } else { "├── " };
        rows.push(Row {
            label: format!("{prefix}{connector}{name}"),
            metrics: Some(metrics_for(entry, approx)),
        });
        idx += 1;
    }
}

/// Visible width (chars). Paths/box-drawing chars are single-width here.
fn width(s: &str) -> usize {
    s.chars().count()
}

pub fn render_by_file_tree(mr: &ModelReport) -> String {
    if mr.files.is_empty() {
        return "(no files)\n".to_string();
    }

    // Build the tree.
    let mut root = Dir::default();
    for f in &mr.files {
        let comps: Vec<&str> = f.rel_path.split('/').filter(|s| !s.is_empty()).collect();
        root.insert(&comps, f);
    }

    let mut rows = Vec::new();
    walk(&root, "", mr.approx, &mut rows);

    // Column widths.
    let label_w = rows.iter().map(|r| width(&r.label)).max().unwrap_or(0);
    let (mut lang_w, mut lines_w, mut bytes_w, mut tok_w) = (
        "Language".len(),
        "lines".len(),
        "bytes".len(),
        "tokens".len(),
    );
    for r in &rows {
        if let Some((lang, lines, bytes, tok)) = &r.metrics {
            lang_w = lang_w.max(width(lang));
            lines_w = lines_w.max(width(lines));
            bytes_w = bytes_w.max(width(bytes));
            tok_w = tok_w.max(width(tok));
        }
    }

    let mut out = String::new();
    // Header.
    out.push_str(&format!(
        "{:<label_w$}  {:<lang_w$}  {:>lines_w$}  {:>bytes_w$}  {:>tok_w$}\n",
        "File", "Language", "lines", "bytes", "tokens",
    ));

    for r in &rows {
        match &r.metrics {
            Some((lang, lines, bytes, tok)) => {
                // Pad label manually (format! width counts bytes, not chars).
                let pad = " ".repeat(label_w.saturating_sub(width(&r.label)));
                out.push_str(&format!(
                    "{}{}  {:<lang_w$}  {:>lines_w$}  {:>bytes_w$}  {:>tok_w$}\n",
                    r.label, pad, lang, lines, bytes, tok,
                ));
            }
            None => {
                out.push_str(&r.label);
                out.push('\n');
            }
        }
    }

    // Totals line.
    let (files, lines, bytes, tokens) = mr.code_totals();
    out.push_str(&format!(
        "\n{} files · {} lines · {} · {} tokens\n",
        group_int(files as u64),
        group_int(lines),
        human_bytes(bytes),
        fmt_tokens(tokens, mr.values_approx()),
    ));

    out
}
