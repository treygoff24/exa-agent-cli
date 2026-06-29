//! `exa-agent` library entrypoint. `run()` parses, dispatches, and is the single funnel that
//! maps a `CliError` to an exit code and an error envelope (arch §10).

#![forbid(unsafe_code)]

pub mod auth;
pub mod cli;
pub mod config;
pub mod doctor;
pub mod error;
pub mod output;
pub mod redaction;
pub mod registry;
pub mod request;
pub mod transport;

use clap::Parser;
use std::io::{self, IsTerminal, Read};

use cli::{
    command_path, AuthCmd, Cli, Command, ConfigCmd, ConfigProfilesCmd, ContentsArgs, GlobalArgs,
    RobotDocsCmd, SchemaCmd, SearchArgs,
};
use error::{CliError, Diag};
use output::envelope::{
    capabilities, error_codes_json, response_envelope, ErrorEnvelope, ResponseEnvelopeArgs,
};
use output::{emit_ndjson, emit_raw, emit_stdout, resolve_mode, stdout_is_tty, OutputMode};
use request::RequestOverrides;
use transport::{execute_raw_with_request_id, parse_user_headers, RawExecuteParams, UreqTransport};

const MAX_CONTENTS_BATCH_SIZE: usize = 100;

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
        Command::Schema { sub } => dispatch_schema(sub, pretty),
        Command::RobotDocs { sub } => dispatch_robot_docs(sub, pretty),
        Command::Doctor(args) => {
            let checks = parse_checks(&args.check);
            doctor::validate_check_ids(&checks)?;
            let options = doctor::DoctorOptions {
                online: args.online,
                checks,
            };
            let ctx = doctor::DoctorCtx::from_process();
            let report = doctor::run_doctor(&options, &ctx);
            emit_stdout(&report.to_json(), pretty);
            Ok(doctor::doctor_exit_code(&report))
        }
        Command::Auth { sub } => dispatch_auth(sub, &cli.globals, pretty),
        Command::Config { sub } => dispatch_config(sub, pretty),
        Command::Search(args) => dispatch_search(args, &cli.globals, pretty),
        Command::Contents(args) => dispatch_contents(args, &cli.globals, pretty),
        Command::Raw(args) => dispatch_raw(args, &cli.globals, pretty),
        _ => Err(not_implemented(
            &command_path(&cli.command),
            "parser skeleton only in this wave",
        )),
    }
}

fn dispatch_search(args: &SearchArgs, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["search"]).expect("search is in the registry");
    with_typed_error_context(op, globals, || {
        let spec = build_search_spec(args, globals)?;
        dispatch_typed_command(spec, globals, pretty)
    })
}

fn build_search_spec(
    args: &SearchArgs,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let op = registry::lookup_by_segments(&["search"]).expect("search is in the registry");
    let flag_values = [
        ("query", Some(args.query.clone())),
        ("num-results", args.num_results.map(|n| n.to_string())),
        ("type", args.r#type.map(|t| t.as_str().to_string())),
        ("category", args.category.map(|c| c.as_str().to_string())),
    ];
    build_typed_spec(op, &flag_values, globals)
}

fn dispatch_contents(
    args: &ContentsArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["contents"]).expect("contents is in the registry");
    with_typed_error_context(op, globals, || {
        let spec = build_contents_spec(args, globals)?;
        let specs = chunk_contents_specs(spec, args.chunk_size)?;
        if specs.len() == 1 && args.chunk_size.is_none() {
            let spec = specs.into_iter().next().expect("one contents spec");
            dispatch_typed_command(spec, globals, pretty)
        } else {
            dispatch_typed_chunks(specs, globals, pretty)
        }
    })
}

fn build_contents_spec(
    args: &ContentsArgs,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let op = registry::lookup_by_segments(&["contents"]).expect("contents is in the registry");
    let flag_values = [
        (
            "urls",
            (!args.urls.is_empty()).then(|| request::encode_str_array(&args.urls)),
        ),
        (
            "ids",
            (!args.ids.is_empty()).then(|| request::encode_str_array(&args.ids)),
        ),
    ];
    build_typed_spec(op, &flag_values, globals)
}

fn build_typed_spec(
    op: &'static registry::OperationDef,
    flag_values: &[(&str, Option<String>)],
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    request::build_request(
        op,
        flag_values,
        RequestOverrides {
            body: globals
                .body
                .as_deref()
                .map(request::parse_body_source)
                .transpose()?,
            sets: &globals.set,
        },
    )
}

fn with_typed_error_context<F>(
    op: &'static registry::OperationDef,
    globals: &GlobalArgs,
    f: F,
) -> Result<i32, CliError>
where
    F: FnOnce() -> Result<i32, CliError>,
{
    match f() {
        Ok(code) => Ok(code),
        Err(err) => {
            let code = err.category() as i32;
            let request_id = if globals.print_request || globals.dry_run {
                "req_dry_run".to_string()
            } else {
                transport::new_request_id()
            };
            let env = ErrorEnvelope::from_error(&err).with_context(
                op.method.as_str(),
                op.api_path,
                request_id,
                globals.correlation_id.clone(),
            );
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&env.to_json()).unwrap_or_default()
            );
            Ok(code)
        }
    }
}

fn chunk_contents_specs(
    spec: request::RequestSpec,
    chunk_size: Option<u32>,
) -> Result<Vec<request::RequestSpec>, CliError> {
    let (field, values) = contents_inputs_from_body(&spec.body)?;
    if values.len() > MAX_CONTENTS_BATCH_SIZE && chunk_size.is_none() {
        return Err(CliError::Usage(
            Diag::new(
                "invalid_flag_combination",
                format!(
                    "contents accepts at most {MAX_CONTENTS_BATCH_SIZE} urls/ids per request; pass --chunk-size {MAX_CONTENTS_BATCH_SIZE} to split larger batches"
                ),
            )
            .with_suggestion(format!(
                "exa-agent contents <inputs> --chunk-size {MAX_CONTENTS_BATCH_SIZE}"
            )),
        ));
    }

    let size = chunk_size.map(|n| n as usize).unwrap_or(values.len());
    if size == 0 || size > MAX_CONTENTS_BATCH_SIZE {
        return Err(CliError::Usage(
            Diag::new(
                "invalid_value",
                format!("--chunk-size must be between 1 and {MAX_CONTENTS_BATCH_SIZE}"),
            )
            .with_suggestion(format!("--chunk-size {MAX_CONTENTS_BATCH_SIZE}")),
        ));
    }

    let mut specs = Vec::new();
    for chunk in values.chunks(size) {
        let mut body = spec.body.clone();
        body[field] = serde_json::Value::Array(
            chunk
                .iter()
                .map(|value| serde_json::Value::String(value.clone()))
                .collect(),
        );
        specs.push(request::RequestSpec { op: spec.op, body });
    }
    Ok(specs)
}

fn contents_inputs_from_body(
    body: &serde_json::Value,
) -> Result<(&'static str, Vec<String>), CliError> {
    let urls = string_array_field(body, "urls")?;
    let ids = string_array_field(body, "ids")?;
    match (urls, ids) {
        (Some(urls), None) if !urls.is_empty() => Ok(("urls", urls)),
        (None, Some(ids)) if !ids.is_empty() => Ok(("ids", ids)),
        (Some(urls), Some(ids)) if urls.is_empty() && !ids.is_empty() => Ok(("ids", ids)),
        (Some(urls), Some(ids)) if !urls.is_empty() && ids.is_empty() => Ok(("urls", urls)),
        (Some(_), Some(_)) => Err(CliError::Usage(Diag::new(
            "invalid_flag_combination",
            "contents request body must contain urls or ids, not both",
        ))),
        _ => Err(CliError::NoInput(Diag::new(
            "missing_required_argument",
            "contents requires at least one URL or --ids value",
        ))),
    }
}

fn string_array_field(
    body: &serde_json::Value,
    key: &'static str,
) -> Result<Option<Vec<String>>, CliError> {
    let Some(value) = body.get(key) else {
        return Ok(None);
    };
    let Some(items) = value.as_array() else {
        return Err(CliError::Usage(Diag::new(
            "invalid_value",
            format!("contents request body field `{key}` must be an array of strings"),
        )));
    };
    let mut strings = Vec::with_capacity(items.len());
    for item in items {
        let Some(s) = item.as_str() else {
            return Err(CliError::Usage(Diag::new(
                "invalid_value",
                format!("contents request body field `{key}` must be an array of strings"),
            )));
        };
        strings.push(s.to_string());
    }
    Ok(Some(strings))
}

/// Registry-backed typed command path: dry-run preview or live transport + response envelope.
fn dispatch_typed_command(
    spec: request::RequestSpec,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = spec.op;
    let method = op.method.as_str();
    let path = op.api_path;
    let request_id = if globals.print_request || globals.dry_run {
        "req_dry_run".to_string()
    } else {
        transport::new_request_id()
    };
    match dispatch_typed_inner(&spec, globals, pretty, &request_id) {
        Ok(code) => Ok(code),
        Err(err) => {
            let code = err.category() as i32;
            let env = ErrorEnvelope::from_error(&err).with_context(
                method,
                path,
                request_id,
                globals.correlation_id.clone(),
            );
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&env.to_json()).unwrap_or_default()
            );
            Ok(code)
        }
    }
}

fn dispatch_typed_inner(
    spec: &request::RequestSpec,
    globals: &GlobalArgs,
    pretty: bool,
    request_id: &str,
) -> Result<i32, CliError> {
    parse_user_headers(&globals.headers)?;
    if globals.print_request || globals.dry_run {
        emit_stdout(&redacted_preview(spec), pretty);
        return Ok(0);
    }

    let api_input = credential_input(auth::CredentialNamespace::Api, globals)?;
    let credential = auth::resolve_api_credential(&api_input, &auth::NoopKeyring)
        .map_err(|missing| auth::not_authenticated_error(&missing))?;
    let cfg = config::Config::load()?;
    let timeout = transport::resolve_timeout(globals, &cfg);
    let transport = UreqTransport::new(timeout);
    execute_typed_live(&transport, spec, globals, &credential, request_id, pretty)
}

fn dispatch_typed_chunks(
    specs: Vec<request::RequestSpec>,
    globals: &GlobalArgs,
    _pretty: bool,
) -> Result<i32, CliError> {
    let op = specs
        .first()
        .map(|spec| spec.op)
        .expect("contents chunking creates at least one spec");
    let batch_request_id = if globals.print_request || globals.dry_run {
        "req_dry_run".to_string()
    } else {
        transport::new_request_id()
    };
    match dispatch_typed_chunks_inner(specs, globals) {
        Ok(code) => Ok(code),
        Err(err) => {
            let code = err.category() as i32;
            let env = ErrorEnvelope::from_error(&err).with_context(
                op.method.as_str(),
                op.api_path,
                batch_request_id,
                globals.correlation_id.clone(),
            );
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&env.to_json()).unwrap_or_default()
            );
            Ok(code)
        }
    }
}

fn dispatch_typed_chunks_inner(
    specs: Vec<request::RequestSpec>,
    globals: &GlobalArgs,
) -> Result<i32, CliError> {
    parse_user_headers(&globals.headers)?;
    if globals.raw && specs.len() > 1 {
        return Err(CliError::Usage(Diag::new(
            "invalid_flag_combination",
            "contents --chunk-size cannot be combined with --raw when it creates multiple upstream requests",
        )));
    }
    if globals.print_request || globals.dry_run {
        for spec in &specs {
            emit_ndjson(&redacted_preview(spec));
        }
        return Ok(0);
    }

    let api_input = credential_input(auth::CredentialNamespace::Api, globals)?;
    let credential = auth::resolve_api_credential(&api_input, &auth::NoopKeyring)
        .map_err(|missing| auth::not_authenticated_error(&missing))?;
    let cfg = config::Config::load()?;
    let timeout = transport::resolve_timeout(globals, &cfg);
    let transport = UreqTransport::new(timeout);
    let mut exit_code = 0;
    for spec in &specs {
        let request_id = transport::new_request_id();
        match execute_typed_live(&transport, spec, globals, &credential, &request_id, false) {
            Ok(10) => exit_code = 10,
            Ok(_) => {}
            Err(err) => {
                let code = err.category() as i32;
                let env = ErrorEnvelope::from_error(&err).with_context(
                    spec.op.method.as_str(),
                    spec.op.api_path,
                    request_id,
                    globals.correlation_id.clone(),
                );
                emit_ndjson(&env.to_json());
                return Ok(code);
            }
        }
    }
    Ok(exit_code)
}

fn execute_typed_live(
    transport: &UreqTransport,
    spec: &request::RequestSpec,
    globals: &GlobalArgs,
    credential: &auth::ResolvedCredential,
    request_id: &str,
    pretty: bool,
) -> Result<i32, CliError> {
    let result = execute_raw_with_request_id(
        transport,
        RawExecuteParams {
            method: spec.op.method.as_str(),
            path: spec.op.api_path,
            query_raw: &[],
            body: spec.body.clone(),
            globals,
            credential,
            request_id: request_id.to_string(),
        },
    )?;

    if globals.raw {
        emit_raw(&result.response.body).map_err(|err| {
            CliError::Interrupted(Diag::new(
                "interrupted",
                format!("failed to write raw stdout: {err}"),
            ))
        })?;
        return Ok(0);
    }

    let data = transport::parse_response_data(&result.response.body);
    let count = transport::primary_count(&data);
    let hash = transport::data_hash(&data);
    let command = spec.op.command();
    let exit_code = if command == "contents" {
        transport::contents_mixed_status_exit_code(&data)
    } else {
        0
    };
    let envelope = response_envelope(ResponseEnvelopeArgs {
        command: &command,
        method: &result.method,
        path: &result.path,
        request_id: &result.request_id,
        profile: &result.profile,
        correlation_id: result.correlation_id.as_deref(),
        data,
        count,
        data_hash: hash,
        retries: result.retries,
    });
    if globals.ndjson {
        emit_ndjson(&envelope);
    } else {
        emit_stdout(&envelope, pretty);
    }
    Ok(exit_code)
}

fn dispatch_auth(sub: &AuthCmd, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    match sub {
        AuthCmd::Status => {
            let api_input = credential_input(auth::CredentialNamespace::Api, globals)?;
            let service_input = credential_input(auth::CredentialNamespace::Service, globals)?;
            let api = auth::resolve_api_credential(&api_input, &auth::NoopKeyring);
            let service = auth::resolve_service_credential(&service_input, &auth::NoopKeyring);
            let (authenticated, source, key_fingerprint, last4, checked) = match api {
                Ok(resolved) => {
                    let status = resolved.status();
                    (
                        true,
                        Some(status.source),
                        Some(status.fingerprint),
                        Some(status.last4),
                        Vec::new(),
                    )
                }
                Err(missing) => (false, None, None, None, missing.checked),
            };
            let mut warnings = Vec::new();
            let (can_admin, service_source) = match service {
                Ok(resolved) if auth::looks_like_api_key(resolved.secret.expose()) => {
                    warnings.push(
                        "EXA_SERVICE_KEY looks like an API key; admin commands require a service key"
                            .to_string(),
                    );
                    (false, Some(resolved.source.label()))
                }
                Ok(resolved) => (true, Some(resolved.source.label())),
                Err(_) => (false, None),
            };
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.auth_status.v1",
                    "ok": true,
                    "authenticated": authenticated,
                    "source": source,
                    "profile": auth::resolve_profile(globals.profile.as_deref(), std::env::var("EXA_PROFILE").ok().as_deref()),
                    "keyFingerprint": key_fingerprint,
                    "last4": last4,
                    "canAdmin": can_admin,
                    "serviceSource": service_source,
                    "checked": checked,
                    "warnings": warnings,
                }),
                pretty,
            );
            Ok(0)
        }
        AuthCmd::Login => {
            let secret = read_secret_stdin("auth login", "EXA_API_KEY")?;
            let path = auth::write_credential_file(auth::CredentialNamespace::Api, &secret)?;
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.auth_login.v1",
                    "ok": true,
                    "stored": true,
                    "source": "credentials_file",
                    "path": path.display().to_string(),
                    "redacted": true,
                    "keyFingerprint": secret.fingerprint(),
                    "last4": secret.last4(),
                }),
                pretty,
            );
            Ok(0)
        }
        AuthCmd::Logout => {
            let path = auth::clear_credential_file(auth::CredentialNamespace::Api)?;
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.auth_logout.v1",
                    "ok": true,
                    "cleared": true,
                    "source": "credentials_file",
                    "path": path.display().to_string(),
                }),
                pretty,
            );
            Ok(0)
        }
        AuthCmd::Test => Err(not_implemented(
            "auth test",
            "network auth probe lands with transport",
        )),
    }
}

fn dispatch_schema(sub: &SchemaCmd, pretty: bool) -> Result<i32, CliError> {
    match sub {
        SchemaCmd::List => {
            let list = schema_operations();
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
        SchemaCmd::Show { name } => {
            let op = registry::lookup_by_command(name)
                .or_else(|| registry::REGISTRY.iter().find(|op| op.operation_id == name))
                .ok_or_else(|| {
                    CliError::Usage(
                        Diag::new(
                            "invalid_value",
                            format!("unknown schema or command `{name}`"),
                        )
                        .with_suggestion("exa-agent schema list"),
                    )
                })?;
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.schema_show.v1",
                    "ok": true,
                    "operation": operation_schema(op),
                }),
                pretty,
            );
            Ok(0)
        }
        SchemaCmd::Export(args) => {
            let target = args.api.as_deref().or(args.cli.as_deref()).unwrap_or("cli");
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.schema_export.v1",
                    "ok": true,
                    "target": target,
                    "spec": {
                        "title": registry::SPEC_TITLE,
                        "version": registry::SPEC_VERSION,
                        "url": registry::SPEC_URL,
                        "embeddedSpecSha256": registry::EMBEDDED_SPEC_SHA256,
                    },
                    "operations": schema_operations(),
                }),
                pretty,
            );
            Ok(0)
        }
        SchemaCmd::ValidateInput(args) => {
            let op = registry::lookup_by_command(&args.command).ok_or_else(|| {
                CliError::Usage(
                    Diag::new(
                        "invalid_value",
                        format!("unknown command `{}`", args.command),
                    )
                    .with_suggestion("exa-agent schema list"),
                )
            })?;
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.schema_validate_input.v1",
                    "ok": true,
                    "valid": true,
                    "command": op.command(),
                    "note": "offline structural validation is limited to known command discovery in this phase",
                }),
                pretty,
            );
            Ok(0)
        }
        SchemaCmd::Refresh(args) => {
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.schema_refresh.v1",
                    "ok": true,
                    "check": args.check,
                    "status": "current",
                    "embeddedSpecSha256": registry::EMBEDDED_SPEC_SHA256,
                }),
                pretty,
            );
            Ok(0)
        }
    }
}

fn dispatch_robot_docs(sub: &RobotDocsCmd, pretty: bool) -> Result<i32, CliError> {
    match sub {
        RobotDocsCmd::Guide => emit_robot_docs(
            serde_json::json!({
                "schema": "exa.cli.robot_docs.v1",
                "ok": true,
                "section": "guide",
                "guidance": [
                    "Use capabilities first to discover command metadata.",
                    "Use --dry-run --print-request before live mutations.",
                    "Do not pass managed auth headers; use EXA_API_KEY or auth login.",
                    "Errors are JSON on stderr with stable error.code values."
                ],
            }),
            pretty,
        ),
        RobotDocsCmd::Commands => emit_robot_docs(
            serde_json::json!({
                "schema": "exa.cli.robot_docs.v1",
                "ok": true,
                "section": "commands",
                "commands": schema_operations(),
            }),
            pretty,
        ),
        RobotDocsCmd::Errors => emit_robot_docs(
            serde_json::json!({
                "schema": "exa.cli.robot_docs.v1",
                "ok": true,
                "section": "errors",
                "exitCodes": error::EXIT_CODES.iter().map(|(code, name, description)| {
                    serde_json::json!({ "exit": code, "category": name, "description": description })
                }).collect::<Vec<_>>(),
                "errorCodes": error_codes_json(),
            }),
            pretty,
        ),
        RobotDocsCmd::Examples(args) => emit_robot_docs(
            serde_json::json!({
                "schema": "exa.cli.robot_docs.v1",
                "ok": true,
                "section": "examples",
                "task": args.task,
                "examples": [
                    "exa-agent capabilities --compact",
                    "exa-agent search \"AI infrastructure news\" --dry-run --print-request --compact",
                    "exa-agent raw GET /websets/v0/teams/me --compact"
                ],
            }),
            pretty,
        ),
        RobotDocsCmd::Prompts => emit_robot_docs(
            serde_json::json!({
                "schema": "exa.cli.robot_docs.v1",
                "ok": true,
                "section": "prompts",
                "prompts": [
                    "First run `exa-agent capabilities --compact`, then choose the narrowest command.",
                    "Before live writes, run the same command with `--dry-run --print-request` and inspect the JSON envelope."
                ],
            }),
            pretty,
        ),
    }
}

fn emit_robot_docs(value: serde_json::Value, pretty: bool) -> Result<i32, CliError> {
    emit_stdout(&value, pretty);
    Ok(0)
}

fn schema_operations() -> Vec<serde_json::Value> {
    registry::REGISTRY.iter().map(operation_schema).collect()
}

fn operation_schema(op: &registry::OperationDef) -> serde_json::Value {
    serde_json::json!({
        "command": op.command(),
        "method": op.method.as_str(),
        "apiPath": op.api_path,
        "operationId": op.operation_id,
        "readOnly": op.read_only,
        "streaming": op.streaming,
        "destructive": op.destructive(),
        "idempotencySensitive": op.idempotency_sensitive,
        "deprecated": op.deprecated,
        "fields": op.fields.iter().map(|field| serde_json::json!({
            "flag": field.flag,
            "bodyPath": field.body_path,
            "required": field.required,
        })).collect::<Vec<_>>(),
    })
}

fn dispatch_config(sub: &ConfigCmd, pretty: bool) -> Result<i32, CliError> {
    match sub {
        ConfigCmd::Path => {
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.config_path.v1",
                    "ok": true,
                    "path": config::config_path().display().to_string(),
                }),
                pretty,
            );
            Ok(0)
        }
        ConfigCmd::List { effective } => {
            let cfg = config::Config::load()?;
            let mut data = cfg.list_json();
            data["effective"] = serde_json::json!(effective);
            data["effectiveBaseUrl"] = serde_json::json!(cfg.effective_base_url());
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.config_list.v1",
                    "ok": true,
                    "config": data,
                }),
                pretty,
            );
            Ok(0)
        }
        ConfigCmd::Get { path } => {
            let cfg = config::Config::load()?;
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.config_get.v1",
                    "ok": true,
                    "path": path,
                    "value": cfg.get_path(path)?,
                }),
                pretty,
            );
            Ok(0)
        }
        ConfigCmd::Set { path, value } => {
            let mut cfg = config::Config::load()?;
            cfg.set_path(path, value)?;
            cfg.save()?;
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.config_set.v1",
                    "ok": true,
                    "path": path,
                    "configPath": config::config_path().display().to_string(),
                }),
                pretty,
            );
            Ok(0)
        }
        ConfigCmd::Unset { path } => {
            let mut cfg = config::Config::load()?;
            cfg.unset_path(path)?;
            cfg.save()?;
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.config_unset.v1",
                    "ok": true,
                    "path": path,
                    "configPath": config::config_path().display().to_string(),
                }),
                pretty,
            );
            Ok(0)
        }
        ConfigCmd::Profiles { sub } => dispatch_config_profiles(sub, pretty),
    }
}

fn dispatch_config_profiles(sub: &ConfigProfilesCmd, pretty: bool) -> Result<i32, CliError> {
    match sub {
        ConfigProfilesCmd::List => {
            let cfg = config::Config::load()?;
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.config_profiles.v1",
                    "ok": true,
                    "data": cfg.profiles_json(),
                }),
                pretty,
            );
            Ok(0)
        }
        ConfigProfilesCmd::Show { name } => {
            let cfg = config::Config::load()?;
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.config_profile.v1",
                    "ok": true,
                    "name": name,
                    "profile": cfg.show_profile(name)?,
                }),
                pretty,
            );
            Ok(0)
        }
        ConfigProfilesCmd::Use { name } => {
            let mut cfg = config::Config::load()?;
            cfg.use_profile(name)?;
            cfg.save()?;
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.config_profile_use.v1",
                    "ok": true,
                    "activeProfile": name,
                }),
                pretty,
            );
            Ok(0)
        }
        ConfigProfilesCmd::Create { name } => {
            let mut cfg = config::Config::load()?;
            cfg.create_profile(name)?;
            cfg.save()?;
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.config_profile_create.v1",
                    "ok": true,
                    "name": name,
                }),
                pretty,
            );
            Ok(0)
        }
        ConfigProfilesCmd::Delete { name } => {
            let mut cfg = config::Config::load()?;
            cfg.delete_profile(name)?;
            cfg.save()?;
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.config_profile_delete.v1",
                    "ok": true,
                    "name": name,
                }),
                pretty,
            );
            Ok(0)
        }
    }
}

fn credential_input(
    ns: auth::CredentialNamespace,
    globals: &GlobalArgs,
) -> Result<auth::CredentialInput, CliError> {
    let stdin = match ns {
        auth::CredentialNamespace::Api if globals.api_key_stdin => Some(
            read_secret_stdin("--api-key-stdin", "EXA_API_KEY")?
                .expose()
                .to_string(),
        ),
        auth::CredentialNamespace::Service if globals.service_key_stdin => Some(
            read_secret_stdin("--service-key-stdin", "EXA_SERVICE_KEY")?
                .expose()
                .to_string(),
        ),
        _ => None,
    };
    let explicit = match ns {
        auth::CredentialNamespace::Api => globals.api_key.clone(),
        auth::CredentialNamespace::Service => globals.service_key.clone(),
    };
    Ok(auth::CredentialInput::from_env(
        globals.profile.clone(),
        explicit,
        stdin,
        ns,
    ))
}

fn read_secret_stdin(context: &str, env_var: &str) -> Result<auth::Secret, CliError> {
    if io::stdin().is_terminal() {
        return Err(CliError::NoInput(
            Diag::new(
                "no_input",
                format!("{context} requires piped stdin (refusing to read an interactive TTY)"),
            )
            .with_suggestion(format!("printf '%s' \"${env_var}\" | exa-agent {context}")),
        ));
    }
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf).map_err(|e| {
        CliError::NoInput(Diag::new("no_input", format!("failed to read stdin: {e}")))
    })?;
    auth::Secret::new(buf).ok_or_else(|| CliError::NoInput(Diag::new("no_input", "stdin is empty")))
}

fn parse_checks(raw: &[String]) -> Vec<String> {
    raw.iter()
        .flat_map(|item| item.split(','))
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn redacted_preview(spec: &request::RequestSpec) -> serde_json::Value {
    let mut body = spec.body.clone();
    redaction::redact_json_value(&mut body);
    let command = spec.op.command();
    let data = serde_json::json!({
        "request": {
            "method": spec.op.method.as_str(),
            "path": spec.op.api_path,
            "body": body,
        },
        "dryRun": true,
    });
    let count = transport::primary_count(data.get("request").unwrap_or(&data));
    let hash = transport::data_hash(&data);
    response_envelope(ResponseEnvelopeArgs {
        command: &command,
        method: spec.op.method.as_str(),
        path: spec.op.api_path,
        request_id: "req_dry_run",
        profile: "default",
        correlation_id: None,
        data,
        count,
        data_hash: hash,
        retries: 0,
    })
}

fn dispatch_raw(args: &cli::RawArgs, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let method = args.method.to_uppercase();
    let request_id = if globals.print_request || globals.dry_run {
        "req_dry_run".to_string()
    } else {
        transport::new_request_id()
    };
    match dispatch_raw_inner(args, globals, pretty, &method, &request_id) {
        Ok(code) => Ok(code),
        Err(err) => {
            let code = err.category() as i32;
            let env = ErrorEnvelope::from_error(&err).with_context(
                method,
                args.path.clone(),
                request_id,
                globals.correlation_id.clone(),
            );
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&env.to_json()).unwrap_or_default()
            );
            Ok(code)
        }
    }
}

fn dispatch_raw_inner(
    args: &cli::RawArgs,
    globals: &GlobalArgs,
    pretty: bool,
    method: &str,
    request_id: &str,
) -> Result<i32, CliError> {
    parse_user_headers(&globals.headers)?;
    if globals.print_request || globals.dry_run {
        let mut body = raw_body(globals)?;
        let query = raw_query_preview(&args.query)?;
        redaction::redact_json_value(&mut body);
        let data = serde_json::json!({
            "request": {
                "method": method,
                "path": args.path,
                "query": query,
                "body": body,
            },
            "dryRun": true,
        });
        let hash = transport::data_hash(&data);
        emit_stdout(
            &response_envelope(ResponseEnvelopeArgs {
                command: "raw",
                method,
                path: &args.path,
                request_id,
                profile: "default",
                correlation_id: globals.correlation_id.as_deref(),
                data,
                count: None,
                data_hash: hash,
                retries: 0,
            }),
            pretty,
        );
        return Ok(0);
    }

    let api_input = credential_input(auth::CredentialNamespace::Api, globals)?;
    let credential = auth::resolve_api_credential(&api_input, &auth::NoopKeyring)
        .map_err(|missing| auth::not_authenticated_error(&missing))?;
    let body = raw_body(globals)?;
    let cfg = config::Config::load()?;
    let timeout = transport::resolve_timeout(globals, &cfg);
    let transport = UreqTransport::new(timeout);
    let result = execute_raw_with_request_id(
        &transport,
        RawExecuteParams {
            method,
            path: &args.path,
            query_raw: &args.query,
            body,
            globals,
            credential: &credential,
            request_id: request_id.to_string(),
        },
    )?;

    if globals.raw {
        emit_raw(&result.response.body).map_err(|err| {
            CliError::Interrupted(Diag::new(
                "interrupted",
                format!("failed to write raw stdout: {err}"),
            ))
        })?;
        return Ok(0);
    }

    let data = transport::parse_response_data(&result.response.body);
    let count = transport::primary_count(&data);
    let hash = transport::data_hash(&data);
    let envelope = response_envelope(ResponseEnvelopeArgs {
        command: "raw",
        method: &result.method,
        path: &result.path,
        request_id: &result.request_id,
        profile: &result.profile,
        correlation_id: result.correlation_id.as_deref(),
        data,
        count,
        data_hash: hash,
        retries: result.retries,
    });
    emit_stdout(&envelope, pretty);
    Ok(0)
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
                    Diag::new("invalid_value", "raw --query expects `key=value`")
                        .with_suggestion("exa-agent raw METHOD PATH --query key=value --dry-run"),
                )
            })?;
            if name.is_empty() {
                return Err(CliError::Usage(
                    Diag::new("invalid_value", "raw --query expects a non-empty key")
                        .with_suggestion("exa-agent raw METHOD PATH --query key=value --dry-run"),
                ));
            }
            let value = if redaction::is_secret_name(name) {
                redaction::REDACTED.to_string()
            } else {
                redaction::scrub_text(value)
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
