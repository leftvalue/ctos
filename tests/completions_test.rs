//! `ctos completions <shell>` generates a valid script for each shell and
//! always exits 0 (it never scans or touches the filesystem).

mod common;

use common::{code, run, stdout};

#[test]
fn bash_completion_generates() {
    let out = run(&["completions", "bash"]);
    assert_eq!(code(&out), 0, "completions bash should exit 0");
    let script = stdout(&out);
    assert!(!script.is_empty(), "bash script must not be empty");
    assert!(
        script.contains("_ctos") && script.contains("complete"),
        "bash script should register a completion for ctos"
    );
}

#[test]
fn zsh_completion_generates() {
    let out = run(&["completions", "zsh"]);
    assert_eq!(code(&out), 0, "completions zsh should exit 0");
    let script = stdout(&out);
    assert!(
        script.contains("#compdef ctos") || script.contains("_ctos"),
        "zsh script should define a compdef for ctos"
    );
}

#[test]
fn fish_completion_generates() {
    let out = run(&["completions", "fish"]);
    assert_eq!(code(&out), 0, "completions fish should exit 0");
    let script = stdout(&out);
    assert!(
        script.contains("complete -c ctos"),
        "fish script should register completions for ctos"
    );
}

#[test]
fn completion_lists_subcommands() {
    // The generated script should reference the real subcommands, proving it is
    // derived from the live command tree rather than a stale hand-written stub.
    let script = stdout(&run(&["completions", "bash"]));
    for sub in ["check", "models", "calibrate", "completions"] {
        assert!(
            script.contains(sub),
            "bash completion should mention subcommand `{sub}`"
        );
    }
}
