//! `--estimate` fast mode: JSON metadata, skill L1/L2 exactness, accuracy,
//! the `check` gate, and exact-mode output cleanliness.

mod common;

use common::{code, fixture, run, stdout};

fn json_output(args: &[&str]) -> (i32, serde_json::Value) {
    let out = run(args);
    (
        code(&out),
        serde_json::from_str(&stdout(&out)).expect("valid JSON"),
    )
}

fn total_code_tokens(v: &serde_json::Value) -> f64 {
    v["code"][0]["languages"]
        .as_array()
        .expect("languages array")
        .iter()
        .map(|l| l["tokens"].as_f64().unwrap_or(0.0))
        .sum()
}

#[test]
fn estimate_emits_metadata_and_valid_json() {
    let path = fixture("skills");
    let (exit, v) = json_output(&[
        "--estimate",
        "-m",
        "gpt-4o",
        "--format",
        "json",
        path.to_str().unwrap(),
    ]);
    // The fixture set intentionally contains an INVALID skill -> exit 1.
    assert_eq!(exit, 1);
    let e = &v["code"][0]["estimate"];
    assert!(
        e.is_object(),
        "code block must carry an `estimate` object in estimate mode"
    );
    assert_eq!(e["total_files"].as_u64().unwrap(), 5);
    assert!(e["sampled_files"].as_u64().unwrap() >= 1);
    assert!(e["sampled_chars"].as_u64().unwrap() <= e["total_chars"].as_u64().unwrap());
}

#[test]
fn estimate_is_deterministic() {
    let path = fixture("skills");
    let args = [
        "--estimate",
        "--sample-budget",
        "40",
        "-m",
        "gpt-4o",
        "--format",
        "json",
        path.to_str().unwrap(),
    ];
    let (_, a) = json_output(&args);
    let (_, b) = json_output(&args);
    assert_eq!(a, b, "estimate mode must be fully deterministic");
}

#[test]
fn skill_l1_l2_stay_exact_in_estimate_mode() {
    let path = fixture("skills");
    let p = path.to_str().unwrap();
    // Tiny budget to force Slice/Skip on regular files; SKILL.md must still
    // be fully encoded, so L1/L2 must match the exact run bit-for-bit.
    let (_, exact) = json_output(&["-m", "gpt-4o", "--format", "json", p]);
    let (_, est) = json_output(&[
        "--estimate",
        "--sample-budget",
        "20",
        "-m",
        "gpt-4o",
        "--format",
        "json",
        p,
    ]);

    let pick = |v: &serde_json::Value, skill: &str| {
        v["results"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["skill"] == skill)
            .unwrap_or_else(|| panic!("{skill} present"))
            .clone()
    };

    for skill in ["alpha", "beta"] {
        let a = pick(&exact, skill);
        let b = pick(&est, skill);
        assert_eq!(
            a["l1"].as_f64().unwrap(),
            b["l1"].as_f64().unwrap(),
            "{skill} L1 must be exact in estimate mode"
        );
        assert_eq!(
            a["l2"].as_f64().unwrap(),
            b["l2"].as_f64().unwrap(),
            "{skill} L2 must be exact in estimate mode"
        );
    }
}

#[test]
fn estimate_total_within_10_percent_on_fixtures() {
    let path = fixture("skills");
    let p = path.to_str().unwrap();
    let (_, exact) = json_output(&["-m", "gpt-4o", "--format", "json", p]);
    let (_, est) = json_output(&[
        "--estimate",
        "--sample-budget",
        "30",
        "-m",
        "gpt-4o",
        "--format",
        "json",
        p,
    ]);
    let ex = total_code_tokens(&exact);
    let es = total_code_tokens(&est);
    let err = (es - ex).abs() / ex;
    assert!(
        err <= 0.10,
        "estimate error {:.1} (fraction) exceeds 10% (exact {ex}, est {es})",
        err
    );
}

#[test]
fn check_rejects_estimate() {
    let path = fixture("skills");
    let out = run(&[
        "check",
        "--estimate",
        "-m",
        "gpt-4o",
        path.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 2, "check + --estimate must be a usage error");
    assert!(String::from_utf8_lossy(&out.stderr).contains("--estimate"));
}

#[test]
fn sample_budget_zero_is_rejected() {
    let path = fixture("skills");
    let out = run(&[
        "--estimate",
        "--sample-budget",
        "0",
        "-m",
        "gpt-4o",
        path.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 2);
}

#[test]
fn exact_mode_json_has_no_estimate_fields() {
    let path = fixture("skills");
    let (_, v) = json_output(&["-m", "gpt-4o", "--format", "json", path.to_str().unwrap()]);
    assert!(
        v["code"][0].get("estimate").is_none(),
        "exact mode must not emit estimate metadata"
    );
    let raw = stdout(&run(&[
        "-m",
        "gpt-4o",
        "--format",
        "json",
        path.to_str().unwrap(),
    ]));
    assert!(
        !raw.contains("\"estimated\""),
        "exact mode must not emit per-file estimated flags"
    );
}
