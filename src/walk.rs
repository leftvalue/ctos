//! Filesystem traversal + text/binary detection + UTF-8 lossy reading.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ignore::WalkBuilder;

use crate::lang::language_of;

const SKILL_FILE: &str = "SKILL.md";
/// Number of head bytes sampled for binary sniffing.
const SNIFF_LEN: usize = 8192;

/// A file discovered during the walk, with its content already read (for text).
#[derive(Debug, Clone)]
pub struct ScannedFile {
    pub path: PathBuf,
    pub rel_path: String,
    pub language: String,
    pub bytes: u64,
    pub is_binary: bool,
    /// UTF-8 (lossy) content for text files; `None` for binary.
    pub content: Option<String>,
}

/// Result of scanning a path.
#[derive(Debug, Clone)]
pub struct Scan {
    /// The root that was scanned.
    pub root: PathBuf,
    pub files: Vec<ScannedFile>,
    /// Directories that directly contain a SKILL.md.
    pub skill_dirs: Vec<PathBuf>,
}

/// Heuristic: a file is binary if its head sample contains a NUL byte or is not
/// predominantly valid UTF-8.
fn is_binary_sample(sample: &[u8]) -> bool {
    if sample.contains(&0) {
        return true;
    }
    match std::str::from_utf8(sample) {
        Ok(_) => false,
        Err(e) => {
            let valid = e.valid_up_to();
            valid * 100 < sample.len() * 70
        }
    }
}

/// Build a path relative to `base` for display.
fn rel_display(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Scan `root`, honoring .gitignore unless `no_ignore` is set.
pub fn scan(root: &Path, no_ignore: bool) -> Result<Scan> {
    let root = root
        .canonicalize()
        .with_context(|| format!("path not found: {}", root.display()))?;

    let base = root.parent().unwrap_or(&root).to_path_buf();

    let mut builder = WalkBuilder::new(&root);
    builder
        .hidden(false)
        .git_ignore(!no_ignore)
        .git_global(!no_ignore)
        .git_exclude(!no_ignore)
        .ignore(!no_ignore)
        .parents(!no_ignore);

    let mut files = Vec::new();
    let mut skill_dirs = Vec::new();

    for dent in builder.build() {
        let dent = match dent {
            Ok(d) => d,
            Err(_) => continue,
        };
        let path = dent.path();
        if !path.is_file() {
            continue;
        }

        let raw = match fs::read(path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let bytes = raw.len() as u64;
        let sample = &raw[..raw.len().min(SNIFF_LEN)];
        let is_binary = is_binary_sample(sample);

        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();
        let language = language_of(&file_name).to_string();

        let content = if is_binary {
            None
        } else {
            Some(String::from_utf8_lossy(&raw).into_owned())
        };

        if file_name == SKILL_FILE {
            if let Some(parent) = path.parent() {
                let p = parent.to_path_buf();
                if !skill_dirs.contains(&p) {
                    skill_dirs.push(p);
                }
            }
        }

        files.push(ScannedFile {
            rel_path: rel_display(&base, path),
            path: path.to_path_buf(),
            language,
            bytes,
            is_binary,
            content,
        });
    }

    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    skill_dirs.sort();

    Ok(Scan {
        root,
        files,
        skill_dirs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_binary_by_nul() {
        assert!(is_binary_sample(b"abc\0def"));
        assert!(!is_binary_sample(b"plain text"));
        assert!(!is_binary_sample("héllo utf8".as_bytes()));
    }
}
