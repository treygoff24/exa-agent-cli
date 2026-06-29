//! Output / envelope layer. The only place bytes reach stdout/stderr (arch §7).
//! Stdout carries data envelopes; stderr carries diagnostics and error envelopes (contracts §1).

pub mod envelope;

use std::io::IsTerminal;

/// Resolved output structure (contracts §2). Whitespace (pretty/compact) is orthogonal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputMode {
    Human,
    Json,
    Ndjson,
    Raw,
}

/// Format precedence (contracts §2): explicit flag > EXA_OUTPUT > auto (TTY=human, pipe=json, D3).
pub fn resolve_mode(
    explicit: Option<OutputMode>,
    env_output: Option<&str>,
    stdout_is_tty: bool,
) -> OutputMode {
    if let Some(m) = explicit {
        return m;
    }
    match env_output {
        Some("human") => OutputMode::Human,
        Some("json") => OutputMode::Json,
        Some("ndjson") => OutputMode::Ndjson,
        _ => {
            if stdout_is_tty {
                OutputMode::Human
            } else {
                OutputMode::Json
            }
        }
    }
}

pub fn stdout_is_tty() -> bool {
    std::io::stdout().is_terminal()
}

/// Serialize a value to stdout in the chosen mode (pretty when `pretty`).
pub fn emit_stdout(value: &serde_json::Value, pretty: bool) {
    let s = if pretty {
        serde_json::to_string_pretty(value).unwrap_or_default()
    } else {
        serde_json::to_string(value).unwrap_or_default()
    };
    println!("{s}");
}
