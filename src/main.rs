//! ctos — count tokens of skill.
//!
//! (not related to Watch Dogs' ctOS.)

mod check;
mod cli;
mod config;
mod count;
mod lang;
mod model;
mod output;
mod skill;
mod tokenizer;
mod util;
mod walk;

use std::path::Path;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command, CommonArgs, Format};
use config::{BudgetsConfig, ModelsConfig};
use output::RenderOpts;
use tokenizer::Registry;

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
    }
}

fn require_path(args: &CommonArgs) -> Result<&Path> {
    args.path
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("a PATH argument is required"))
}

fn build_registry(args: &CommonArgs) -> Result<Registry> {
    let models_cfg = ModelsConfig::load(args.models_config.as_deref(), args.verbose)?;
    Registry::build(&models_cfg, &args.models, args.all_models)
}

fn render_opts(args: &CommonArgs) -> RenderOpts {
    RenderOpts {
        verbose: args.verbose,
        by_file: args.by_file,
        quiet: args.quiet,
    }
}

fn run_count(args: &CommonArgs) -> Result<ExitCode> {
    let path = require_path(args)?;
    let registry = build_registry(args)?;
    let report = count::run(path, &registry, args.no_ignore)?;

    output::emit(
        &report,
        args.format,
        args.output.as_deref(),
        render_opts(args),
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
    let path = require_path(args)?;
    let registry = build_registry(args)?;
    let budgets = BudgetsConfig::load(args.budgets.as_deref(), args.verbose)?;

    let report = count::run(path, &registry, args.no_ignore)?;
    let outcome = check::evaluate(&report, &budgets, baseline)?;

    match args.format {
        Format::Json => {
            output::emit(
                &report,
                Format::Json,
                args.output.as_deref(),
                render_opts(args),
            )?;
        }
        Format::Table => {
            let text = check::render_report(&outcome, baseline.is_some());
            output::write_out(&text, args.output.as_deref())?;
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
        Format::Table => {
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
    let report = count::run(path, &registry, false)?;

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
