//! ctos — count tokens of skill.
//!
//! (not related to Watch Dogs' ctOS.)

mod check;
mod cli;
mod config;
mod count;
mod estimate;
mod lang;
mod model;
mod output;
mod progress;
mod skill;
mod tokenizer;
mod update;
mod util;
mod walk;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};

use cli::{Cli, Command, CommonArgs, Format};
use config::{BudgetsConfig, ModelsConfig};
use output::RenderOpts;
use tokenizer::Registry;
use walk::{FilterConfig, ScanOpts};

/// Exit codes are a CI contract (spec §6.3): 0 ok, 1 gate failure, 2 runtime error.
fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("ctos: error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.command {
        None => run_count(&cli.count),
        Some(Command::Count(args)) => run_count(&args),
        Some(Command::Check { common, baseline }) => run_check(&common, baseline.as_deref()),
        Some(Command::Models {
            models_config,
            format,
        }) => run_models(models_config.as_deref(), format),
        Some(Command::Calibrate {
            model,
            path,
            models_config,
        }) => run_calibrate(&model, &path, models_config.as_deref()),
        Some(Command::Completions { shell }) => run_completions(shell),
        Some(Command::Update { check, yes }) => update::run(check, yes).map(|_| ExitCode::SUCCESS),
    }
}

/// Print a shell completion script to stdout. Always exits 0.
fn run_completions(shell: clap_complete::Shell) -> Result<ExitCode> {
    let mut cmd = Cli::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
    Ok(ExitCode::SUCCESS)
}

fn require_paths(args: &CommonArgs) -> Result<&[PathBuf]> {
    if args.paths.is_empty() {
        anyhow::bail!("at least one PATH argument is required (use `-` for stdin)");
    }
    Ok(&args.paths)
}

fn build_registry(args: &CommonArgs) -> Result<Registry> {
    let models_cfg = ModelsConfig::load(args.models_config.as_deref(), args.verbose)?;
    Registry::build(&models_cfg, &args.models, args.all_models)
}

/// Parse `--max-file-size` (MB) into bytes.
fn max_file_size_bytes(args: &CommonArgs) -> Option<u64> {
    args.max_file_size.map(|mb| (mb * 1_048_576.0) as u64)
}

fn scan_opts(args: &CommonArgs) -> Result<ScanOpts> {
    // Live progress: only when stderr is a terminal, not silenced by
    // --quiet / --no-progress (pipes and CI logs stay clean automatically).
    let progress_enabled =
        !args.quiet && !args.no_progress && progress::ProgressReporter::enabled_by_default();

    if args.estimate && args.sample_budget == 0 {
        anyhow::bail!("--sample-budget must be at least 1 character");
    }

    Ok(ScanOpts {
        no_ignore: args.no_ignore,
        stdin_name: args.stdin_name.clone(),
        progress: Some(std::sync::Arc::new(progress::ProgressReporter::new(
            progress_enabled,
        ))),
        estimate: args.estimate,
        sample_budget: if args.estimate { args.sample_budget } else { 0 },
        count_tokenizer_files: args.count_tokenizers,
        filter: FilterConfig {
            exclude_dirs: args.exclude_dir.clone(),
            include_exts: args.include_ext.clone(),
            exclude_exts: args.exclude_ext.clone(),
            include_langs: args.include_lang.clone(),
            exclude_langs: args.exclude_lang.clone(),
            max_file_size_bytes: max_file_size_bytes(args),
        },
    })
}

fn render_opts(args: &CommonArgs) -> Result<RenderOpts> {
    let summary_cutoff = match &args.summary_cutoff {
        Some(s) => Some(output::Cutoff::parse(s).context("invalid --summary-cutoff")?),
        None => None,
    };
    Ok(RenderOpts {
        verbose: args.verbose,
        by_file: args.by_file,
        by_file_by_lang: args.by_file_by_lang,
        quiet: args.quiet,
        style: args.style,
        sort: args.sort,
        hide_rate: args.hide_rate,
        summary_cutoff,
    })
}

fn run_count(args: &CommonArgs) -> Result<ExitCode> {
    let paths = require_paths(args)?;
    let registry = build_registry(args)?;
    let report = count::run(paths, &registry, &scan_opts(args)?)?;

    if args.verbose && report.skipped_tokenizer_files > 0 {
        eprintln!(
            "[ctos] skipped {} tokenizer artifact file(s) — use --count-tokenizers to include them",
            report.skipped_tokenizer_files
        );
    }

    output::emit(
        &report,
        args.format,
        args.output.as_deref(),
        render_opts(args)?,
    )?;

    // In plain count mode, INVALID skills still set exit code 1 (spec §4.2.4).
    let has_invalid = report
        .reports
        .iter()
        .flat_map(|r| &r.skills)
        .any(|s| s.status.is_invalid());
    Ok(if has_invalid {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn run_check(args: &CommonArgs, baseline: Option<&Path>) -> Result<ExitCode> {
    if args.estimate {
        anyhow::bail!(
            "--estimate cannot be combined with `check`: the budget gate must run on exact counts"
        );
    }
    let paths = require_paths(args)?;
    let registry = build_registry(args)?;
    let budgets = BudgetsConfig::load(args.budgets.as_deref(), args.verbose)?;

    let report = count::run(paths, &registry, &scan_opts(args)?)?;
    if args.verbose && report.skipped_tokenizer_files > 0 {
        eprintln!(
            "[ctos] skipped {} tokenizer artifact file(s) — use --count-tokenizers to include them",
            report.skipped_tokenizer_files
        );
    }
    let outcome = check::evaluate(&report, &budgets, baseline)?;

    match args.format {
        Format::Table | Format::Md | Format::Csv => {
            let text = check::render_report(&outcome, baseline.is_some());
            output::write_out(&text, args.output.as_deref())?;
        }
        Format::Json => {
            output::emit(
                &report,
                Format::Json,
                args.output.as_deref(),
                render_opts(args)?,
            )?;
        }
    }

    Ok(if outcome.failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}

fn run_models(models_config: Option<&Path>, format: Format) -> Result<ExitCode> {
    let cfg = ModelsConfig::load(models_config, false)?;
    match format {
        Format::Json => {
            let list: Vec<_> = cfg
                .models
                .iter()
                .map(|(name, spec)| {
                    serde_json::json!({
                        "name": name,
                        "source": spec.source,
                        "overhead_l1": spec.overhead_l1(),
                    })
                })
                .collect();
            let out = serde_json::to_string_pretty(&serde_json::json!({ "models": list }))?;
            output::write_out(&out, None)?;
        }
        _ => {
            let mut out = String::from("ctos models — registered tokenizers\n\n");
            out.push_str(&format!("{:<18} {}\n", "model", "source"));
            out.push_str(&"-".repeat(48));
            out.push('\n');
            for (name, spec) in &cfg.models {
                out.push_str(&format!("{:<18} {}\n", name, spec.source));
            }
            output::write_out(&out, None)?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn run_calibrate(model: &str, path: &Path, models_config: Option<&Path>) -> Result<ExitCode> {
    let cfg = ModelsConfig::load(models_config, false)?;
    let registry = Registry::build(&cfg, std::slice::from_ref(&model.to_string()), false)?;
    let paths = [path.to_path_buf()];
    let report = count::run(&paths, &registry, &ScanOpts::default())?;

    let mut out = format!("ctos calibrate — model: {model}\n\n");
    for mr in &report.reports {
        if mr.skills.is_empty() {
            out.push_str("no SKILL.md found under the given path.\n");
        }
        for s in &mr.skills {
            out.push_str(&format!("skill: {}\n", s.skill));
            out.push_str(&format!("  path: {}\n", s.rel_path));
            match &s.status {
                model::SkillStatus::Invalid(reason) => {
                    out.push_str(&format!("  status: INVALID ({reason})\n"));
                }
                _ => {
                    out.push_str(&format!(
                        "  L1: {}  L2: {}  L3(tok): {}  L3(bytes): {}\n",
                        s.l1.unwrap_or(0.0),
                        s.l2.unwrap_or(0.0),
                        s.l3_tokens,
                        s.l3_bytes,
                    ));
                }
            }
        }
    }
    out.push_str(
        "\nCompare these against your client's reported token count and adjust\n\
         overhead_l1 / chars_per_token in models.toml. See calibration/README.md.\n",
    );
    output::write_out(&out, None)?;
    Ok(ExitCode::SUCCESS)
}
