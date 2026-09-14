//! Filesystem traversal + text/binary detection + UTF-8 lossy reading.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use ignore::WalkBuilder;

use crate::lang::language_of;
use crate::progress::ProgressReporter;

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
    /// Physical line count for text files; `None` for binary.
    pub lines: Option<u64>,
    /// UTF-8 (lossy) content for text files; `None` for binary.
    pub content: Option<String>,
}

/// Filtering rules applied while collecting files.
#[derive(Debug, Clone, Default)]
pub struct FilterConfig {
    /// Directory names to exclude (any path component match prunes the subtree).
    pub exclude_dirs: Vec<String>,
    /// Whitelist of file extensions (without dot); empty = allow all.
    pub include_exts: Vec<String>,
    /// Blacklist of file extensions (without dot).
    pub exclude_exts: Vec<String>,
    /// Whitelist of languages; empty = allow all.
    pub include_langs: Vec<String>,
    /// Blacklist of languages.
    pub exclude_langs: Vec<String>,
    /// Skip files larger than this many bytes while traversing (explicit args exempt).
    pub max_file_size_bytes: Option<u64>,
}

impl FilterConfig {
    fn norm(list: &[String]) -> Vec<String> {
        list.iter().map(|s| s.trim().to_ascii_lowercase()).collect()
    }

    /// Whether a file (given its extension and language) passes ext/lang filters.
    /// `exclude` takes precedence; a non-empty include list acts as a whitelist.
    fn accepts(&self, ext: &str, language: &str) -> bool {
        let ext_l = ext.to_ascii_lowercase();
        let lang_l = language.to_ascii_lowercase();

        let exclude_exts = Self::norm(&self.exclude_exts);
        if exclude_exts.iter().any(|e| e == &ext_l) {
            return false;
        }
        let exclude_langs = Self::norm(&self.exclude_langs);
        if exclude_langs.iter().any(|l| l == &lang_l) {
            return false;
        }
        let include_exts = Self::norm(&self.include_exts);
        if !include_exts.is_empty() && !include_exts.iter().any(|e| e == &ext_l) {
            return false;
        }
        let include_langs = Self::norm(&self.include_langs);
        if !include_langs.is_empty() && !include_langs.iter().any(|l| l == &lang_l) {
            return false;
        }
        true
    }
}

/// Options controlling a scan.
#[derive(Debug, Clone, Default)]
pub struct ScanOpts {
    pub no_ignore: bool,
    pub filter: FilterConfig,
    /// Filename used to determine the language of stdin (`-`) input.
    pub stdin_name: Option<String>,
    /// Live progress reporter for the scan phase (None = no progress).
    pub progress: Option<Arc<ProgressReporter>>,
    /// Fast estimate mode: stratified sampling instead of exact encoding.
    pub estimate: bool,
    /// Per-language character budget for estimate-mode sampling.
    pub sample_budget: usize,
}

/// Result of scanning one or more paths.
#[derive(Debug, Clone)]
pub struct Scan {
    /// The roots that were scanned (for display).
    pub roots: Vec<PathBuf>,
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

/// The extension (lowercased, no dot) of a file name.
fn extension_of(file_name: &str) -> String {
    file_name
        .rsplit_once('.')
        .map(|(_, e)| e.to_string())
        .unwrap_or_default()
}

/// Build a `ScannedFile` from raw bytes; returns `None` if filtered out by
/// ext/lang rules.
fn make_scanned_file(
    path: PathBuf,
    rel_path: String,
    file_name: &str,
    raw: &[u8],
    filter: &FilterConfig,
) -> Option<ScannedFile> {
    let language = language_of(file_name).to_string();
    let ext = extension_of(file_name);
    if !filter.accepts(&ext, &language) {
        return None;
    }

    let bytes = raw.len() as u64;
    let sample = &raw[..raw.len().min(SNIFF_LEN)];
    let is_binary = is_binary_sample(sample);
    let content = if is_binary {
        None
    } else {
        Some(String::from_utf8_lossy(raw).into_owned())
    };
    let lines = content.as_ref().map(|c| c.lines().count() as u64);

    Some(ScannedFile {
        path,
        rel_path,
        language,
        bytes,
        is_binary,
        lines,
        content,
    })
}

/// Read all of stdin into a single virtual `ScannedFile`.
fn scan_stdin(opts: &ScanOpts) -> Result<ScannedFile> {
    let mut raw = Vec::new();
    std::io::stdin()
        .read_to_end(&mut raw)
        .context("failed to read stdin")?;

    let name = opts
        .stdin_name
        .clone()
        .unwrap_or_else(|| "<stdin>".to_string());
    let language = match &opts.stdin_name {
        Some(n) => language_of(n).to_string(),
        None => "Text/Other".to_string(),
    };
    let bytes = raw.len() as u64;
    let content = String::from_utf8_lossy(&raw).into_owned();
    let lines = content.lines().count() as u64;

    Ok(ScannedFile {
        path: PathBuf::from("-"),
        rel_path: name,
        language,
        bytes,
        is_binary: false,
        lines: Some(lines),
        content: Some(content),
    })
}

/// Scan a single directory/file root, appending into the given collections.
fn scan_one(
    root: &Path,
    opts: &ScanOpts,
    files: &mut Vec<ScannedFile>,
    skill_dirs: &mut Vec<PathBuf>,
) -> Result<PathBuf> {
    let canon = root
        .canonicalize()
        .with_context(|| format!("path not found: {}", root.display()))?;

    // A file passed explicitly on the command line is exempt from max-file-size.
    let root_is_file = canon.is_file();
    let base = canon.parent().unwrap_or(&canon).to_path_buf();

    let no_ignore = opts.no_ignore;
    let mut builder = WalkBuilder::new(&canon);
    builder
        .hidden(false)
        .git_ignore(!no_ignore)
        .git_global(!no_ignore)
        .git_exclude(!no_ignore)
        .ignore(!no_ignore)
        .parents(!no_ignore);

    // Prune excluded directories by name (any path component match).
    if !opts.filter.exclude_dirs.is_empty() {
        let excluded: Vec<String> = opts
            .filter
            .exclude_dirs
            .iter()
            .map(|s| s.trim().to_string())
            .collect();
        builder.filter_entry(move |dent| {
            let name = dent.file_name().to_string_lossy();
            !(dent.file_type().map(|ft| ft.is_dir()).unwrap_or(false)
                && excluded.iter().any(|e| e == name.as_ref()))
        });
    }

    for dent in builder.build() {
        let dent = match dent {
            Ok(d) => d,
            Err(_) => continue,
        };
        let path = dent.path();
        if !path.is_file() {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();

        let raw = match fs::read(path) {
            Ok(b) => b,
            Err(_) => continue,
        };

        // max-file-size: applies to traversed files, not explicit file roots.
        if let Some(limit) = opts.filter.max_file_size_bytes {
            let is_explicit_root = root_is_file && path == canon;
            if !is_explicit_root && raw.len() as u64 > limit {
                continue;
            }
        }

        let rel = rel_display(&base, path);
        let scanned =
            match make_scanned_file(path.to_path_buf(), rel, &file_name, &raw, &opts.filter) {
                Some(f) => f,
                None => continue,
            };

        if file_name == SKILL_FILE {
            if let Some(parent) = path.parent() {
                let p = parent.to_path_buf();
                if !skill_dirs.contains(&p) {
                    skill_dirs.push(p);
                }
            }
        }

        if let Some(progress) = &opts.progress {
            progress.tick_scan_file(scanned.bytes);
        }

        files.push(scanned);
    }

    Ok(canon)
}

/// Scan one or more paths (use `-` for stdin), honoring .gitignore unless
/// `no_ignore` is set and applying the given filters.
pub fn scan_many(paths: &[PathBuf], opts: &ScanOpts) -> Result<Scan> {
    let mut files = Vec::new();
    let mut skill_dirs = Vec::new();
    let mut roots = Vec::new();

    for p in paths {
        if p.as_os_str() == "-" {
            files.push(scan_stdin(opts)?);
            roots.push(PathBuf::from("-"));
            continue;
        }
        let canon = scan_one(p, opts, &mut files, &mut skill_dirs)?;
        roots.push(canon);
    }

    files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    files.dedup_by(|a, b| a.path == b.path && a.path != Path::new("-"));
    skill_dirs.sort();
    skill_dirs.dedup();

    Ok(Scan {
        roots,
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

    #[test]
    fn filter_exclude_takes_precedence() {
        let f = FilterConfig {
            include_exts: vec!["rs".into()],
            exclude_exts: vec!["rs".into()],
            ..Default::default()
        };
        // exclude wins over include
        assert!(!f.accepts("rs", "Rust"));
    }

    #[test]
    fn filter_include_is_whitelist() {
        let f = FilterConfig {
            include_langs: vec!["python".into()],
            ..Default::default()
        };
        assert!(f.accepts("py", "Python"));
        assert!(!f.accepts("rs", "Rust"));
    }

    #[test]
    fn filter_empty_accepts_all() {
        let f = FilterConfig::default();
        assert!(f.accepts("rs", "Rust"));
        assert!(f.accepts("", "Text/Other"));
    }
}
