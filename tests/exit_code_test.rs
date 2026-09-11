//! Exit-code contract (spec §6.3): 0 pass, 1 gate/invalid, 2 runtime error.

mod common;

use common::{code, fixture, run};

#[test]
fn exit_0_on_passing_check() {
    let path = fixture("skills/alpha");
    let out = run(&["check", "-m", "gpt-4o", path.to_str().unwrap()]);
    assert_eq!(code(&out), 0, "well-formed skill under budget should pass");
}

#[test]
fn exit_1_on_invalid_skill_in_check() {
    let path = fixture("skills"); // contains the INVALID 'broken' skill
    let out = run(&["check", "-m", "gpt-4o", path.to_str().unwrap()]);
    assert_eq!(code(&out), 1, "INVALID skill must fail the gate");
}

#[test]
fn exit_1_on_invalid_skill_in_count() {
    let path = fixture("skills");
    let out = run(&["-m", "gpt-4o", path.to_str().unwrap()]);
    assert_eq!(code(&out), 1, "INVALID skill sets exit 1 in count mode");
}

#[test]
fn exit_0_on_clean_code_dir() {
    let path = fixture("code");
    let out = run(&["-m", "gpt-4o", path.to_str().unwrap()]);
    assert_eq!(code(&out), 0, "pure code with no skills should be exit 0");
}

#[test]
fn exit_2_on_missing_path() {
    let out = run(&["-m", "gpt-4o", "/nonexistent/path/should/not/exist"]);
    assert_eq!(code(&out), 2, "missing path is a runtime error");
}
