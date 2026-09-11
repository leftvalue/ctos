//! Counting behavior: code aggregation, binary handling, skill layers, edges.

mod common;

use common::{fixture, run, stdout};

fn json(args: &[&str]) -> serde_json::Value {
    let out = run(args);
    serde_json::from_str(&stdout(&out)).expect("valid JSON")
}

#[test]
fn code_dir_aggregates_by_language_and_skips_binary() {
    let path = fixture("code");
    let v = json(&["-m", "gpt-4o", "--format", "json", path.to_str().unwrap()]);
    let code = &v["code"][0];

    // Binary blob must be present with is_binary=true and null tokens.
    let files = code["files"].as_array().unwrap();
    let blob = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with("blob.bin"))
        .expect("blob.bin present");
    assert_eq!(blob["is_binary"], serde_json::json!(true));
    assert!(blob["tokens"].is_null(), "binary tokens must be null");

    // Rust + Python languages appear; Rust file counts >0 tokens.
    let langs = code["languages"].as_array().unwrap();
    assert!(langs.iter().any(|l| l["language"] == "Rust"));
    assert!(langs.iter().any(|l| l["language"] == "Python"));
}

#[test]
fn empty_body_skill_still_valid() {
    // beta has an empty body but valid frontmatter -> OK, small L2.
    let path = fixture("skills/beta");
    let v = json(&["-m", "gpt-4o", "--format", "json", path.to_str().unwrap()]);
    let beta = v["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["skill"] == "beta")
        .expect("beta present");
    assert_eq!(beta["status"], "OK");
    assert!(beta["l1"].as_f64().unwrap() > 0.0);
}

#[test]
fn invalid_skill_is_flagged() {
    let path = fixture("skills/broken");
    let v = json(&["-m", "gpt-4o", "--format", "json", path.to_str().unwrap()]);
    let broken = &v["results"][0];
    assert_eq!(broken["status"], "INVALID");
}

#[test]
fn claude_marks_approximate() {
    let path = fixture("code");
    let v = json(&["-m", "claude", "--format", "json", path.to_str().unwrap()]);
    assert_eq!(v["code"][0]["approx"], serde_json::json!(true));
}

#[test]
fn all_models_skips_unbuildable_and_succeeds() {
    // hunyuan is registered but its tokenizer is not vendored in test builds;
    // --all-models must warn+skip it and still succeed with the other models.
    let path = fixture("code");
    let out = run(&["--all-models", "--format", "json", path.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "one bad model must not fail the run"
    );

    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    let models: Vec<String> = v["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap().to_string())
        .collect();
    assert!(models.contains(&"qwen3".to_string()));
    assert!(models.contains(&"gpt-4o".to_string()));
    assert!(models.contains(&"claude".to_string()));
    assert!(
        !models.contains(&"hunyuan".to_string()),
        "unbuildable model should be skipped"
    );

    // The warning is emitted on stderr.
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("hunyuan"),
        "expected a warning about hunyuan"
    );
}

#[test]
fn m_all_is_an_alias_for_all_models() {
    let path = fixture("code");
    let out = run(&["-m", "all", "--format", "json", path.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert!(v["models"].as_array().unwrap().len() >= 2);
}
