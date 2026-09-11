//! Integration tests for the cloc-parity features:
//! multi-path, filtering, stdin, md/csv, by-file-by-lang, max-file-size,
//! sort and summary-cutoff, and --hide-rate determinism.

mod common;

use std::io::Write;
use std::process::{Command, Stdio};

use common::{code, fixture, run, stdout};

fn json(args: &[&str]) -> serde_json::Value {
    let out = run(args);
    serde_json::from_str(&stdout(&out)).expect("valid JSON")
}

#[test]
fn multi_path_aggregates() {
    let code_dir = fixture("code");
    let skills_dir = fixture("skills");
    let v = json(&[
        "-m",
        "gpt-4o",
        "--format",
        "json",
        code_dir.to_str().unwrap(),
        skills_dir.to_str().unwrap(),
    ]);
    // Languages from both trees should be present.
    let langs: Vec<String> = v["code"][0]["languages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["language"].as_str().unwrap().to_string())
        .collect();
    assert!(langs.contains(&"Rust".to_string()));
    assert!(langs.contains(&"Markdown".to_string()));
}

#[test]
fn exclude_lang_filters_out() {
    let code_dir = fixture("code");
    let v = json(&[
        "-m",
        "gpt-4o",
        "--format",
        "json",
        "--exclude-lang",
        "Rust",
        code_dir.to_str().unwrap(),
    ]);
    let langs: Vec<String> = v["code"][0]["languages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["language"].as_str().unwrap().to_string())
        .collect();
    assert!(!langs.contains(&"Rust".to_string()));
    assert!(langs.contains(&"Python".to_string()));
}

#[test]
fn include_ext_is_whitelist() {
    let code_dir = fixture("code");
    let v = json(&[
        "-m",
        "gpt-4o",
        "--format",
        "json",
        "--include-ext",
        "py",
        code_dir.to_str().unwrap(),
    ]);
    let langs: Vec<String> = v["code"][0]["languages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["language"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(langs, vec!["Python".to_string()]);
}

#[test]
fn stdin_dash_reads_stdin() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ctos"))
        .args([
            "-m",
            "gpt-4o",
            "--format",
            "json",
            "--stdin-name",
            "x.py",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn ctos");
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"def f():\n    return 1\n")
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    let langs: Vec<String> = v["code"][0]["languages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["language"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(langs, vec!["Python".to_string()]);
}

#[test]
fn md_output_has_pipe_table() {
    let code_dir = fixture("code");
    let out = run(&["-m", "gpt-4o", "--format", "md", code_dir.to_str().unwrap()]);
    let s = stdout(&out);
    assert!(s.contains("| Language | files | lines | bytes | tokens |"));
    assert!(s.contains("| **SUM** |"));
}

#[test]
fn csv_output_has_scope_column() {
    let code_dir = fixture("code");
    let out = run(&[
        "-m",
        "gpt-4o",
        "--format",
        "csv",
        code_dir.to_str().unwrap(),
    ]);
    let s = stdout(&out);
    assert!(s.starts_with("scope,model,name,language,files,lines,bytes,tokens,status"));
    assert!(s.lines().any(|l| l.starts_with("lang,gpt-4o,")));
}

#[test]
fn summary_cutoff_folds_into_other() {
    let skills_dir = fixture("skills");
    // Python has 1 file; with files:2 threshold it folds into Other.
    let out = run(&[
        "-m",
        "gpt-4o",
        "--hide-rate",
        "--summary-cutoff",
        "files:2",
        skills_dir.to_str().unwrap(),
    ]);
    let s = stdout(&out);
    assert!(s.contains("Other"));
}

#[test]
fn hide_rate_is_deterministic() {
    let code_dir = fixture("code");
    let a = stdout(&run(&[
        "-m",
        "gpt-4o",
        "--hide-rate",
        code_dir.to_str().unwrap(),
    ]));
    let b = stdout(&run(&[
        "-m",
        "gpt-4o",
        "--hide-rate",
        code_dir.to_str().unwrap(),
    ]));
    assert_eq!(a, b, "hide-rate output must be byte-identical across runs");
    assert!(!a.contains("files/s"));
}

#[test]
fn max_file_size_skips_but_explicit_is_exempt() {
    let code_dir = fixture("code");
    let hello = fixture("code/hello.rs");
    // ~10 bytes limit skips everything under the dir.
    let dir_out = run(&[
        "-m",
        "gpt-4o",
        "--hide-rate",
        "--max-file-size",
        "0.00001",
        code_dir.to_str().unwrap(),
    ]);
    assert!(stdout(&dir_out).contains("0 files scanned"));

    // But an explicitly-passed file is exempt.
    let file_out = run(&[
        "-m",
        "gpt-4o",
        "--hide-rate",
        "--max-file-size",
        "0.00001",
        hello.to_str().unwrap(),
    ]);
    assert!(stdout(&file_out).contains("1 files scanned"));
}

#[test]
fn bad_summary_cutoff_is_runtime_error() {
    let code_dir = fixture("code");
    let out = run(&[
        "-m",
        "gpt-4o",
        "--summary-cutoff",
        "nonsense",
        code_dir.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 2, "invalid --summary-cutoff must exit 2");
}
