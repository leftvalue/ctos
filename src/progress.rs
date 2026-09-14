//! Two-phase live progress reporting on stderr.
//!
//! Phase 1 (scan):     total unknown -> spinner + files/bytes/rate counter.
//! Phase 2 (tokenize): total known   -> bar + current model + ETA.
//!
//! The bar only ever writes to stderr, so stdout (table/JSON/md/csv and the
//! golden tests) is untouched. When disabled (quiet, `--no-progress`, or
//! stderr is not a terminal) every method is a no-op.

use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use indicatif::{ProgressBar, ProgressStyle};

use crate::util::human_bytes;

const SPINNER_TEMPLATE: &str =
    "{spinner:.green} [1/2] scan {pos} files · {msg} [{elapsed_precise}]";
const BAR_TEMPLATE: &str = "{bar:28.cyan/blue} [2/2] tokenize {pos}/{len} {msg} ETA {eta_precise}";

impl std::fmt::Debug for ProgressReporter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgressReporter")
            .field("enabled", &self.enabled.load(Ordering::Relaxed))
            .finish()
    }
}

pub struct ProgressReporter {
    /// `None` => disabled; all methods no-op. Swapped at the phase transition.
    bar: Mutex<Option<ProgressBar>>,
    /// Whether the reporter was created enabled (survives the phase swap).
    enabled: AtomicBool,
    /// Start of the scan phase (for rate computation).
    start: Instant,
    /// Bytes accepted so far in the scan phase.
    scan_bytes: AtomicU64,
}

impl ProgressReporter {
    /// `enabled` should already fold in `quiet`, `--no-progress` and
    /// `stderr.is_terminal()` (see [`ProgressReporter::enabled_by_default`]).
    pub fn new(enabled: bool) -> Self {
        let bar = if enabled {
            Some(
                ProgressBar::new_spinner().with_style(
                    ProgressStyle::with_template(SPINNER_TEMPLATE)
                        .expect("valid spinner template")
                        .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
                ),
            )
        } else {
            None
        };
        Self {
            bar: Mutex::new(bar),
            enabled: AtomicBool::new(enabled),
            start: Instant::now(),
            scan_bytes: AtomicU64::new(0),
        }
    }

    /// Whether the progress bar should be on by default:
    /// stderr is a terminal (piped/CI output stays clean).
    pub fn enabled_by_default() -> bool {
        std::io::stderr().is_terminal()
    }

    /// Phase 1: one file accepted by the scanner.
    pub fn tick_scan_file(&self, bytes: u64) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let guard = self.bar.lock().expect("progress bar lock");
        let Some(bar) = guard.as_ref() else { return };

        let total = self.scan_bytes.fetch_add(bytes, Ordering::Relaxed) + bytes;
        let elapsed = self.start.elapsed().as_secs_f64().max(1e-9);
        let rate = total as f64 / elapsed;
        let msg = format!("{} · {}/s", human_bytes(total), human_bytes(rate as u64));
        bar.set_message(msg);
        bar.inc(1);
    }

    /// Phase 2: switch to the deterministic bar for `total_work` tok.count calls.
    pub fn begin_tokenize(&self, total_work: u64, model: &str) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let mut guard = self.bar.lock().expect("progress bar lock");
        // Close phase 1 cleanly before switching templates.
        if let Some(old) = guard.take() {
            old.finish_and_clear();
        }
        let bar = ProgressBar::new(total_work)
            .with_style(ProgressStyle::with_template(BAR_TEMPLATE).expect("valid bar template"));
        bar.set_message(model.to_string());
        *guard = Some(bar);
    }

    /// Phase 2: one tok.count completed.
    pub fn tick_tokenize(&self) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let guard = self.bar.lock().expect("progress bar lock");
        if let Some(bar) = guard.as_ref() {
            bar.inc(1);
        }
    }

    /// Clear any visible progress line. Idempotent.
    pub fn finish(&self) {
        let mut guard = self.bar.lock().expect("progress bar lock");
        if let Some(bar) = guard.take() {
            bar.finish_and_clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_reporter_is_noop() {
        let r = ProgressReporter::new(false);
        r.tick_scan_file(1024);
        r.begin_tokenize(10, "m");
        r.tick_tokenize();
        r.finish();
        // Reaching here without panic is the contract; nothing is drawn.
    }

    #[test]
    fn enabled_reporter_ticks_do_not_panic() {
        let r = ProgressReporter::new(true);
        r.tick_scan_file(10);
        r.tick_scan_file(20);
        r.begin_tokenize(4, "qwen3");
        r.tick_tokenize();
        r.finish();
    }

    #[test]
    fn finish_is_idempotent() {
        let r = ProgressReporter::new(true);
        r.finish();
        r.finish();
    }
}
