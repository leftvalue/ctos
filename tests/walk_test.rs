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

/// Build a temp project with vendored tokenizer artifacts.
fn make_tokenizer_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ctos_tok_test_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("tokenizers/qwen")).unwrap();
    fs::create_dir_all(dir.join("tokenizers/kimi")).unwrap();
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("tokenizers/qwen/tokenizer.json"), "{\"vocab\":{}}").unwrap();
    fs::write(dir.join("tokenizers/kimi/tiktoken.model"), "aGFsbw== 0\n").unwrap();
    fs::write(dir.join("src/a.rs"), "fn main() {}\n").unwrap();
    dir
}

#[test]
fn tokenizer_artifacts_are_skipped_by_default() {
    let dir = make_tokenizer_project("default");
    let paths = text_file_count(&["-m", "claude", "--format", "json", dir.to_str().unwrap()]);
    assert!(
        !paths
            .iter()
            .any(|p| p.ends_with("tokenizer.json") || p.ends_with("tiktoken.model")),
        "tokenizer artifacts must be skipped by default, got {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p.ends_with("src/a.rs")),
        "regular sources must be counted, got {paths:?}"
    );
    // -v reports how many artifacts were skipped.
    let out = run(&[
        "-v",
        "-m",
        "claude",
        "--format",
        "json",
        dir.to_str().unwrap(),
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("skipped 2 tokenizer artifact file(s)"),
        "-v should report 2 skipped artifacts, stderr: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn count_tokenizers_flag_includes_artifacts() {
    let dir = make_tokenizer_project("flag");
    let paths = text_file_count(&[
        "--count-tokenizers",
        "-m",
        "claude",
        "--format",
        "json",
        dir.to_str().unwrap(),
    ]);
    assert!(
        paths.iter().any(|p| p.ends_with("tokenizer.json")),
        "--count-tokenizers must include tokenizer.json, got {paths:?}"
    );
    assert!(
        paths.iter().any(|p| p.ends_with("tiktoken.model")),
        "--count-tokenizers must include tiktoken.model, got {paths:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn explicit_tokenizer_path_is_always_counted() {
    let dir = make_tokenizer_project("explicit");
    let file = dir.join("tokenizers/qwen/tokenizer.json");
    let paths = text_file_count(&["-m", "claude", "--format", "json", file.to_str().unwrap()]);
    // The rel path of an explicit file root is just the file name (base is
    // its parent directory).
    assert!(
        paths.iter().any(|p| p.ends_with("tokenizer.json")),
        "explicit tokenizer path should be counted even by default, got {paths:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}
