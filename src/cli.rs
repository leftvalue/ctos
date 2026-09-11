//! Command-line interface definition (clap derive).
//!
//! Command shapes (per spec §5.1):
//!   ctos <PATH> [OPTIONS]          # default = count
//!   ctos check <PATH> [OPTIONS]    # budget gate
//!   ctos models                    # list registry models & sources
//!   ctos calibrate --model <m> <PATH>

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

pub const ABOUT: &str = "ctos — count tokens of skill";
pub const LONG_ABOUT: &str = "\
ctos (count tokens of skill) — a cloc-style, cross-platform token counter.

It walks a path and counts the tokens of every plain-text file under several
tokenizers, aggregated cloc-style by language. Binary files are listed by size
only. When a directory contains a SKILL.md, an Agent-Skill L1/L2/L3 breakdown is
layered on top. `ctos check` doubles as a CI budget gate for skill size.

(not related to Watch Dogs' ctOS.)";

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Format {
    Table,
    Json,
}

/// Visual style for `table` output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TableStyle {
    /// Full box-drawing borders (default).
    Boxed,
    /// cloc-style: no outer box, dashed rules between sections.
    Plain,
}

/// Options shared by `count` (default) and `check`.
#[derive(Debug, Clone, Args)]
pub struct CommonArgs {
    /// Path to scan (file or directory). Required unless a subcommand is used.
    pub path: Option<PathBuf>,

    /// Tokenizer model to use; repeatable. Defaults to `default_models` from the
    /// registry (ships as qwen3). Ignored when `--all-models` is set.
    #[arg(short = 'm', long = "model", value_name = "NAME")]
    pub models: Vec<String>,

    /// Use every model in the registry. Models whose tokenizer cannot be loaded
    /// (e.g. a builtin not vendored in this build) are warned about and skipped
    /// instead of failing the whole run.
    #[arg(short = 'a', long = "all-models")]
    pub all_models: bool,

    /// Output format.
    #[arg(long, value_enum, default_value_t = Format::Table)]
    pub format: Format,

    /// Table style for `--format table`.
    #[arg(long, value_enum, default_value_t = TableStyle::Boxed)]
    pub style: TableStyle,

    /// Write output to a file instead of stdout.
    #[arg(short = 'o', long, value_name = "PATH")]
    pub output: Option<PathBuf>,

    /// Custom budgets.toml (falls back to built-in defaults).
    #[arg(long, value_name = "PATH")]
    pub budgets: Option<PathBuf>,

    /// Custom models.toml registry (falls back to built-in defaults).
    #[arg(long = "models-config", value_name = "PATH")]
    pub models_config: Option<PathBuf>,

    /// Do not honor .gitignore; traverse everything.
    #[arg(long = "no-ignore")]
    pub no_ignore: bool,

    /// Per-file detail (expands L3 files / by-file code listing).
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Show a per-file code listing instead of language aggregation.
    #[arg(long = "by-file")]
    pub by_file: bool,

    /// Only emit what the exit code requires.
    #[arg(short = 'q', long)]
    pub quiet: bool,
}

#[derive(Debug, Parser)]
#[command(
    name = "ctos",
    version,
    about = ABOUT,
    long_about = LONG_ABOUT,
    propagate_version = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Default (count) arguments, used when no subcommand is given.
    #[command(flatten)]
    pub count: CommonArgs,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Count tokens (explicit form of the default command).
    Count(CommonArgs),

    /// Budget gate for CI: validate skills against budgets.toml.
    Check {
        #[command(flatten)]
        common: CommonArgs,

        /// Baseline results JSON for growth diffing.
        #[arg(long, value_name = "PATH")]
        baseline: Option<PathBuf>,
    },

    /// List every model in the registry and its source.
    Models {
        /// Custom models.toml registry.
        #[arg(long = "models-config", value_name = "PATH")]
        models_config: Option<PathBuf>,

        /// Output format.
        #[arg(long, value_enum, default_value_t = Format::Table)]
        format: Format,
    },

    /// Emit per-layer counts for a skill under one model, for manual calibration.
    Calibrate {
        /// Model to calibrate against.
        #[arg(short = 'm', long = "model", value_name = "NAME")]
        model: String,

        /// Path to the skill (directory containing SKILL.md).
        path: PathBuf,

        /// Custom models.toml registry.
        #[arg(long = "models-config", value_name = "PATH")]
        models_config: Option<PathBuf>,
    },
}
