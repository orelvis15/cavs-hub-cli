//! Global output mode (`--json` / `--quiet`), set once from the parsed CLI
//! flags and read from anywhere via [`mode`].
//!
//! `--json` switches the machine-readable commands to a single JSON document on
//! stdout; `--quiet` drops non-essential human lines (progress and trailing
//! summaries) while still printing the primary result and all errors. Info/
//! progress goes to stderr through [`info`] so `--json` stdout stays clean.

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, Default)]
pub struct OutputMode {
    pub json: bool,
    pub quiet: bool,
}

static MODE: OnceLock<OutputMode> = OnceLock::new();

/// Record the output mode once, at startup. Later calls are ignored (the mode
/// is fixed for the process lifetime).
pub fn init(json: bool, quiet: bool) {
    let _ = MODE.set(OutputMode { json, quiet });
}

pub fn mode() -> OutputMode {
    MODE.get().copied().unwrap_or_default()
}

pub fn is_json() -> bool {
    mode().json
}

pub fn is_quiet() -> bool {
    mode().quiet
}

/// Print a non-essential info/progress line to stderr, suppressed by `--quiet`
/// or `--json`. Errors and primary results never go through here.
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        if !$crate::output::is_quiet() && !$crate::output::is_json() {
            eprintln!($($arg)*);
        }
    }};
}

/// Serialize a value to a single pretty JSON line on stdout (used by the
/// `--json` branches).
pub fn emit_json<T: serde::Serialize>(value: &T) -> anyhow::Result<()> {
    let s = serde_json::to_string_pretty(value)?;
    println!("{s}");
    Ok(())
}
