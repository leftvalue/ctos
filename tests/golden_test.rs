//! Golden test: fixture skills under a deterministic, offline tokenizer.
//!
//! Uses gpt-4o (tiktoken o200k_base), which is version-locked via Cargo.lock.
//! When the tokenizer library changes token counts, this snapshot must be
//! updated explicitly — that is itself the regression signal for the tokenizer
//! layer.

mod common;

use common::{fixture, run, stdout};

#[test]
fn skills_gpt4o_matches_golden() {
    let path = fixture("skills");
    let out = run(&["-m", "gpt-4o", "--format", "json", path.to_str().unwrap()]);
    // The fixture set intentionally contains an INVALID skill, so the exit code
    // is 1; we validate the JSON payload, not the status here.
    let produced: serde_json::Value =
        serde_json::from_str(&stdout(&out)).expect("output is valid JSON");

    let golden_str = include_str!("golden/skills_gpt-4o.json");
    let golden: serde_json::Value = serde_json::from_str(golden_str).expect("golden is valid JSON");

    assert_eq!(
        produced, golden,
        "output diverged from golden snapshot.\n\
         If this is an intentional tokenizer change, regenerate with:\n  \
         ctos -m gpt-4o --format json tests/fixtures/skills > tests/golden/skills_gpt-4o.json"
    );
}
