//! `exa-agent` library entrypoint. `run()` parses, dispatches, and is the single funnel that
//! maps a `CliError` to an exit code and an error envelope (arch §10).

#![forbid(unsafe_code)]

pub mod cli;
pub mod error;
pub mod output;
pub mod registry;
pub mod request;

use clap::Parser;

use cli::{Cli, Command, GlobalArgs, SchemaCmd};
use error::{CliError, Diag};
use output::envelope::{capabilities, ErrorEnvelope};
use output::{emit_stdout, resolve_mode, stdout_is_tty, OutputMode};

/// Parse args, dispatch, and return the process exit code.
pub fn run() -> i32 {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => return handle_clap_error(e),
    };

    match dispatch(&cli) {
        Ok(code) => code,
        Err(err) => {
            let env = ErrorEnvelope::from_error(&err);
            // Errors go to stderr; stdout stays empty (contracts §1/§5).
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&env.to_json()).unwrap_or_default()
            );
            err.category() as i32
        }
    }
}

/// clap's default exit 2 collides with `auth` (D23): help/version → stdout exit 0; every other
/// parse error → exit 1 + an `exa.cli.error.v1` envelope on stderr.
fn handle_clap_error(e: clap::Error) -> i32 {
    use clap::error::ErrorKind;
    match e.kind() {
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
            print!("{e}");
            0
        }
        kind => {
            let code = match kind {
                ErrorKind::UnknownArgument => "unknown_flag",
                ErrorKind::InvalidSubcommand | ErrorKind::MissingSubcommand => "unknown_subcommand",
                ErrorKind::InvalidValue | ErrorKind::ValueValidation => "invalid_value",
                ErrorKind::MissingRequiredArgument => "missing_required_argument",
                _ => "invalid_value",
            };
            let err = CliError::Usage(Diag::new(code, first_line(&e.to_string())));
            let env = ErrorEnvelope::from_error(&err);
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&env.to_json()).unwrap_or_default()
            );
            err.category() as i32
        }
    }
}

fn first_line(s: &str) -> String {
    s.lines()
        .next()
        .unwrap_or(s)
        .trim_start_matches("error: ")
        .to_string()
}

fn dispatch(cli: &Cli) -> Result<i32, CliError> {
    let pretty = want_pretty(&cli.globals);
    match &cli.command {
        Command::Capabilities => {
            emit_stdout(&capabilities(), pretty);
            Ok(0)
        }
        Command::Schema { sub } => match sub {
            SchemaCmd::List => {
                let list: Vec<_> = registry::REGISTRY
                    .iter()
                    .map(|op| {
                        serde_json::json!({
                            "command": op.command(),
                            "method": op.method.as_str(),
                            "apiPath": op.api_path,
                            "operationId": op.operation_id,
                        })
                    })
                    .collect();
                emit_stdout(
                    &serde_json::json!({
                        "schema": "exa.cli.schema_list.v1",
                        "ok": true,
                        "count": list.len(),
                        "operations": list,
                    }),
                    pretty,
                );
                Ok(0)
            }
        },
        Command::Search(args) => {
            let op = registry::lookup_by_segments(&["search"]).expect("search is in the registry");
            let flag_values = [
                ("query", Some(args.query.clone())),
                ("num-results", args.num_results.map(|n| n.to_string())),
                ("type", args.r#type.clone()),
                ("category", args.category.clone()),
            ];
            let spec = request::build_body(op, &flag_values)?;
            if cli.globals.print_request || cli.globals.dry_run {
                emit_stdout(&spec.preview(), pretty);
                Ok(0)
            } else {
                Err(not_implemented(
                    "search",
                    "transport lands in the next milestone",
                ))
            }
        }
        Command::Raw(args) => {
            if cli.globals.print_request || cli.globals.dry_run {
                let body: serde_json::Value = match &args.body {
                    Some(b) => serde_json::from_str(b).unwrap_or(serde_json::Value::Null),
                    None => serde_json::Value::Null,
                };
                emit_stdout(
                    &serde_json::json!({
                        "schema": "exa.cli.request_preview.v1",
                        "ok": true,
                        "command": "raw",
                        "request": { "method": args.method.to_uppercase(), "path": args.path, "body": body },
                        "dryRun": true,
                    }),
                    pretty,
                );
                Ok(0)
            } else {
                Err(not_implemented(
                    "raw",
                    "transport lands in the next milestone",
                ))
            }
        }
    }
}

fn not_implemented(cmd: &str, detail: &str) -> CliError {
    CliError::Usage(
        Diag::new(
            "not_implemented",
            format!("`{cmd}` is recognized but not yet wired in this build: {detail}"),
        )
        .with_suggestion("exa-agent capabilities".to_string()),
    )
}

/// Pretty when `--pretty`, or in a TTY without `--compact`. JSON envelope is the default in a pipe.
fn want_pretty(g: &GlobalArgs) -> bool {
    let env_output = std::env::var("EXA_OUTPUT").ok();
    let explicit = explicit_mode(g);
    let mode = resolve_mode(explicit, env_output.as_deref(), stdout_is_tty());
    if g.pretty {
        return true;
    }
    if g.compact {
        return false;
    }
    matches!(mode, OutputMode::Human) // TTY default
}

fn explicit_mode(g: &GlobalArgs) -> Option<OutputMode> {
    if g.raw {
        return Some(OutputMode::Raw);
    }
    if g.json {
        return Some(OutputMode::Json);
    }
    if g.ndjson {
        return Some(OutputMode::Ndjson);
    }
    g.format.map(|f| match f {
        cli::Format::Human => OutputMode::Human,
        cli::Format::Json => OutputMode::Json,
        cli::Format::Ndjson => OutputMode::Ndjson,
    })
}
