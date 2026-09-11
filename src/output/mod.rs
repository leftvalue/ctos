//! Output dispatch (table / json) and optional file write-out.

pub mod json;
pub mod table;

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

use crate::cli::Format;
use crate::model::Report;

/// Options that affect rendering.
#[derive(Debug, Clone, Copy)]
pub struct RenderOpts {
    pub verbose: bool,
    pub by_file: bool,
    pub quiet: bool,
}

/// Render a report to the chosen format and write to stdout or a file.
pub fn emit(
    report: &Report,
    format: Format,
    output: Option<&Path>,
    opts: RenderOpts,
) -> Result<()> {
    let rendered = match format {
        Format::Table => table::render(report, opts),
        Format::Json => json::render(report)?,
    };
    write_out(&rendered, output)
}

pub fn write_out(text: &str, output: Option<&Path>) -> Result<()> {
    match output {
        Some(p) => {
            std::fs::write(p, text)
                .with_context(|| format!("failed to write output: {}", p.display()))?;
        }
        None => {
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            lock.write_all(text.as_bytes())?;
            if !text.ends_with('\n') {
                lock.write_all(b"\n")?;
            }
        }
    }
    Ok(())
}
