//! Progress bar behavior: stdout stays byte-identical regardless of the
//! progress settings, and stderr stays clean when the bar is disabled or
//! stderr is not a terminal (integration tests always run with piped stderr).

mod common;

use common::{code, fixture, run, stdout};

#[test]
fn no_progress_leaves_stdout_identical() {
    let path = fixture("skills");
    let p = path.to_str().unwrap();

    let plain = stdout(&run(&["-m", "gpt-4o", "--format", "json", p]));
    let flagged = stdout(&run(&[
        "-m",
        "gpt-4o",
        "--format",
        "json",
        "--no-progress",
        p,
    ]));

    assert_eq!(
        plain, flagged,
        "--no-progress must not change stdout output"
    );
}

#[test]
fn quiet_implies_no_progress_stderr_clean() {
    // Test harness pipes stderr => the bar is auto-hidden either way; this
    // guards the contract that nothing leaks to stderr under -q.
    let path = fixture("code");
    let out = run(&["-q", "-m", "gpt-4o", path.to_str().unwrap()]);
    assert_eq!(code(&out), 0);
    assert!(
        out.stderr.is_empty(),
        "quiet mode must not write progress to stderr"
    );
}

#[test]
fn no_progress_still_reports_correctly() {
    let path = fixture("skills");
    let out = run(&[
        "--no-progress",
        "-m",
        "gpt-4o",
        "--format",
        "json",
        path.to_str().unwrap(),
    ]);
    // fixtures/skills contains an INVALID skill: exit 1 is the contract.
    assert_eq!(code(&out), 1);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(v["code"][0]["model"], "gpt-4o");
}
