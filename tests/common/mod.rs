//! Shared helpers for integration tests.
#![allow(dead_code)]

use std::path::PathBuf;
use std::process::{Command, Output};

/// Absolute path to a fixture under tests/fixtures/.
pub fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel)
}

/// Run the built ctos binary with args; returns the process Output.
pub fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ctos"))
        .args(args)
        .output()
        .expect("failed to run ctos binary")
}

pub fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn code(out: &Output) -> i32 {
    out.status.code().unwrap_or(-1)
}
