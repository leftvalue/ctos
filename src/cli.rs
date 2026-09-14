//! Command-line interface definition (clap derive).
//!
//! Command shapes (per spec §5.1):
//!   ctos <PATH> [OPTIONS]          # default = count
//!   ctos check <PATH> [OPTIONS]    # budget gate
//!   ctos models                    # list registry models & sources
//!   ctos calibrate --model <m> <PATH>

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

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
    Md,
    Csv,
}

/// Visual style for `table` output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TableStyle {
    /// Full box-drawing borders (default).
    Boxed,
    /// cloc-style: no outer box, dashed rules between sections.
    Plain,
}

/// Sort key for language / file tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SortKey {
    Tokens,
    Bytes,
    Lines,
    Files,
    Name,
}

/// Options shared by `count` (default) and `check`.
#[derive(Debug, Clone, Args)]
pub struct CommonArgs {
    /// Paths to scan (files or directories). Use `-` to read from stdin.
    /// Required unless a subcommand is used.
    pub paths: Vec<PathBuf>,

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

    /// Sort language/file rows by this key (default: tokens, descending).
    #[arg(long, value_enum, default_value_t = SortKey::Tokens)]
    pub sort: SortKey,

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

    /// Count vendored tokenizer artifact files (tokenizer.json,
    /// tiktoken.model). They are skipped by default: they are model
    /// artifacts, not project content. Explicitly passed paths are always
    /// counted.
    #[arg(long = "count-tokenizers")]
    pub count_tokenizers: bool,

    /// Exclude directories by name (comma-separated), e.g. `node_modules,test`.
    #[arg(long = "exclude-dir", value_name = "D1,D2,...", value_delimiter = ',')]
    pub exclude_dir: Vec<String>,

    /// Only count files with these extensions (comma-separated whitelist).
    #[arg(long = "include-ext", value_name = "e1,e2,...", value_delimiter = ',')]
    pub include_ext: Vec<String>,

    /// Do not count files with these extensions (comma-separated).
    #[arg(long = "exclude-ext", value_name = "e1,e2,...", value_delimiter = ',')]
    pub exclude_ext: Vec<String>,

    /// Only count these languages (comma-separated whitelist).
    #[arg(long = "include-lang", value_name = "L1,L2,...", value_delimiter = ',')]
    pub include_lang: Vec<String>,

    /// Do not count these languages (comma-separated).
    #[arg(long = "exclude-lang", value_name = "L1,L2,...", value_delimiter = ',')]
    pub exclude_lang: Vec<String>,

    /// Skip files larger than this many megabytes while traversing directories.
    /// Files passed explicitly on the command line are exempt.
    #[arg(long = "max-file-size", value_name = "MB")]
    pub max_file_size: Option<f64>,

    /// Aggregate languages below a threshold into an `Other` row.
    /// Format: `<metric>:<N>[%]` where metric is tokens|files|lines|bytes.
    #[arg(long = "summary-cutoff", value_name = "X:N")]
    pub summary_cutoff: Option<String>,

    /// Filename used to determine the language of stdin (`-`) input.
    #[arg(long = "stdin-name", value_name = "FILE")]
    pub stdin_name: Option<String>,

    /// Hide elapsed time and processing rates in the scan header
    /// (makes output deterministic).
    #[arg(long = "hide-rate")]
    pub hide_rate: bool,

    /// Disable the live progress bar. Implied by --quiet; also auto-hidden
    /// whenever stderr is not a terminal (pipes, CI logs).
    #[arg(long = "no-progress")]
    pub no_progress: bool,

    /// Fast estimate mode: stratified per-language sampling instead of exact
    /// encoding of every file. Values are marked with `~` and carry an error
    /// bound. SKILL.md L1/L2 stay exact; cannot be combined with `check`.
    #[arg(long)]
    pub estimate: bool,

    /// Per-language character budget for --estimate sampling (default 524288).
    /// Smaller = faster, larger = more accurate.
    #[arg(
        long = "sample-budget",
        value_name = "CHARS",
        default_value_t = crate::estimate::DEFAULT_SAMPLE_BUDGET
    )]
    pub sample_budget: usize,

    /// Per-file detail (expands L3 files / by-file listing).
    #[arg(short = 'v', long)]
    pub verbose: bool,

    /// Show a per-file tree view instead of language aggregation.
    #[arg(long = "by-file")]
    pub by_file: bool,

    /// Show a per-file tree view in addition to language aggregation.
    #[arg(long = "by-file-by-lang")]
    pub by_file_by_lang: bool,

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

    /// Generate a shell completion script (bash, zsh, fish, powershell, elvish).
    ///
    /// Print the script to stdout; install it into your shell's completion
    /// directory to enable Tab completion. See the README for per-shell steps.
    Completions {
        /// Target shell.
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Self-update: check GitHub Releases for a newer version and replace
    /// the running binary (download verified against the release digest).
    /// Docker installs are refused; cargo-installed binaries warn.
    Update {
        /// Only report the latest release; never download or install.
        #[arg(long)]
        check: bool,

        /// Skip the interactive confirmation prompt.
        #[arg(short = 'y', long)]
        yes: bool,
    },
}
