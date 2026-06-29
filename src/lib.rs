//! `exa-agent` library entrypoint. `run()` parses, dispatches, and is the single funnel that
//! maps a `CliError` to an exit code and an error envelope (arch §10).

#![forbid(unsafe_code)]

pub mod cli;
pub mod error;
pub mod output;
pub mod redaction;
pub mod registry;
pub mod request;

use clap::Parser;

use cli::{command_path, Cli, Command, GlobalArgs, SchemaCmd};
use error::{CliError, Diag};
use output::envelope::{capabilities, ErrorEnvelope};
use output::{emit_stdout, resolve_mode, stdout_is_tty, OutputMode};
use request::RequestOverrides;

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
        Command::Schema {
            sub: SchemaCmd::List,
        } => {
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
        Command::Search(args) => {
            let op = registry::lookup_by_segments(&["search"]).expect("search is in the registry");
            let flag_values = [
                ("query", Some(args.query.clone())),
                ("num-results", args.num_results.map(|n| n.to_string())),
                ("type", args.r#type.map(|t| t.as_str().to_string())),
                ("category", args.category.map(|c| c.as_str().to_string())),
            ];
            let spec = request::build_request(
                op,
                &flag_values,
                RequestOverrides {
                    body: cli
                        .globals
                        .body
                        .as_deref()
                        .map(request::parse_body_source)
                        .transpose()?,
                    sets: &cli.globals.set,
                },
            )?;
            if cli.globals.print_request || cli.globals.dry_run {
                emit_stdout(&redacted_preview(&spec), pretty);
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
                let mut body = raw_body(&cli.globals)?;
                let query = raw_query_preview(&args.query)?;
                redaction::redact_json_value(&mut body);
                emit_stdout(
                    &serde_json::json!({
                        "schema": "exa.cli.request_preview.v1",
                        "ok": true,
                        "command": "raw",
                        "request": {
                            "method": args.method.to_uppercase(),
                            "path": args.path,
                            "query": query,
                            "body": body
                        },
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
        _ => Err(not_implemented(
            &command_path(&cli.command),
            "parser skeleton only in this wave",
        )),
    }
}

fn redacted_preview(spec: &request::RequestSpec) -> serde_json::Value {
    spec.preview_with_redactor(|body| {
        let mut redacted = body.clone();
        redaction::redact_json_value(&mut redacted);
        redacted
    })
}

fn raw_body(globals: &GlobalArgs) -> Result<serde_json::Value, CliError> {
    let mut body = match &globals.body {
        Some(raw) => {
            let source = request::parse_body_source(raw)?;
            request::read_body_source(source)?
        }
        None => serde_json::Value::Null,
    };
    for entry in &globals.set {
        let (path, value) = request::parse_set(entry)?;
        if body.is_null() {
            body = serde_json::Value::Object(serde_json::Map::new());
        }
        request::set_at_path(&mut body, &path, value)?;
    }
    Ok(body)
}

fn raw_query_preview(raw: &[String]) -> Result<Vec<serde_json::Value>, CliError> {
    raw.iter()
        .map(|item| {
            let (name, value) = item.split_once('=').ok_or_else(|| {
                CliError::Usage(
                    Diag::new(
                        "invalid_value",
                        format!("raw --query expects `key=value`, got `{item}`"),
                    )
                    .with_suggestion("exa-agent raw METHOD PATH --query key=value --dry-run"),
                )
            })?;
            if name.is_empty() {
                return Err(CliError::Usage(
                    Diag::new(
                        "invalid_value",
                        format!("raw --query expects a non-empty key in `{item}`"),
                    )
                    .with_suggestion("exa-agent raw METHOD PATH --query key=value --dry-run"),
                ));
            }
            let value = if redaction::is_secret_name(name) {
                redaction::REDACTED
            } else {
                value
            };
            Ok(serde_json::json!({ "name": name, "value": value }))
        })
        .collect()
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
