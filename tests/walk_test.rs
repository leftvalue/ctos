//! Traversal behavior: VCS directories are never counted, dotfiles still are,
//! and explicit roots are exempt from pruning.

mod common;

use std::fs;
use std::path::PathBuf;

use common::{run, stdout};

/// Build a temp project with VCS dirs, a dotfile, and regular sources.
fn make_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ctos_walk_test_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join(".git")).unwrap();
    fs::create_dir_all(dir.join(".svn")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(dir.join(".git/config"), "[core]\n").unwrap();
    fs::write(dir.join(".svn/wc.db"), b"\x00\x01\x02db").unwrap();
    fs::write(dir.join("src/a.rs"), "fn main() {}\n").unwrap();
    fs::write(dir.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(dir.join(".env"), "KEY=value\n").unwrap();
    dir
}

fn text_file_count(args: &[&str]) -> Vec<String> {
    let out = run(args);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    v["code"][0]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn vcs_directories_are_skipped_by_default() {
    let dir = make_project("default");
    let paths = text_file_count(&["-m", "claude", "--format", "json", dir.to_str().unwrap()]);
    let names: Vec<&str> = paths.iter().map(|s| s.as_str()).collect();
    assert!(
        !names
            .iter()
            .any(|p| p.contains("/.git/") || p.contains("/.svn/")),
        "VCS internals must not be counted, got {names:?}"
    );
    // src/a.rs, .gitignore and .env are real content.
    for expected in ["src/a.rs", ".gitignore", ".env"] {
        assert!(
            names.iter().any(|p| p.ends_with(expected)),
            "{expected} should be counted, got {names:?}"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn vcs_directories_stay_skipped_under_no_ignore() {
    let dir = make_project("noignore");
    let paths = text_file_count(&[
        "--no-ignore",
        "-m",
        "claude",
        "--format",
        "json",
        dir.to_str().unwrap(),
    ]);
    assert!(
        !paths
            .iter()
            .any(|p| p.contains("/.git/") || p.contains("/.svn/")),
        "--no-ignore must not re-include VCS internals, got {paths:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn explicit_vcs_root_is_still_counted() {
    let dir = make_project("explicitroot");
    let git_dir = dir.join(".git");
    let paths = text_file_count(&[
        "-m",
        "claude",
        "--format",
        "json",
        git_dir.to_str().unwrap(),
    ]);
    // An explicitly passed .git path is honored (root exemption).
    assert!(
        paths.iter().any(|p| p.ends_with(".git/HEAD")),
        "explicit VCS root should be counted, got {paths:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}
