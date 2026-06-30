//! `exa-agent` library entrypoint. `run()` parses, dispatches, and is the single funnel that
//! maps a `CliError` to an exit code and an error envelope (arch §10).

#![forbid(unsafe_code)]

pub mod auth;
pub mod cli;
pub mod config;
pub mod doctor;
pub mod error;
pub mod output;
pub mod pending;
pub mod redaction;
pub mod registry;
pub mod request;
pub mod stream;
pub mod transport;

use clap::Parser;
use std::io::{self, IsTerminal, Read};
use std::time::Duration;

use cli::{
    command_path, AgentCmd, AgentRunArgs, AgentRunsCmd, AgentRunsEventsArgs, AnswerArgs, AuthCmd,
    Cli, Command, ConfigCmd, ConfigProfilesCmd, ContentsArgs, ContextArgs, FetchArgs, GlobalArgs,
    PaginationArgs, ResearchCmd, ResearchCreateArgs, RobotDocsCmd, SchemaCmd, SearchArgs,
    SimilarArgs, TeamCmd,
};
use error::{CliError, Diag};
use output::envelope::{
    capabilities, error_codes_json, event_envelope, response_envelope, ErrorEnvelope,
    EventEnvelopeArgs, ResponseEnvelopeArgs,
};
use output::{
    emit_ndjson, emit_raw, emit_stdout, resolve_mode, stdout_is_tty, write_ndjson,
    write_stdout_value, OutputMode,
};
use request::RequestOverrides;
use transport::{
    body_wants_stream, execute_raw_stream_with_request_id, execute_raw_with_request_id,
    infer_stream_event_type, parse_user_headers, terminal_stream_data, RawExecuteParams,
    StreamItem, Transport, UreqTransport,
};

const MAX_CONTENTS_BATCH_SIZE: usize = 100;
const MAX_CONTEXT_QUERY_CHARS: usize = 2_000;

#[derive(Clone, Copy)]
struct TypedRoute<'a> {
    path: &'a str,
    query: &'a [(String, String)],
    /// When true, send `Accept: text/event-stream` (for GET SSE replay paths).
    sse_accept: bool,
}

#[derive(Clone, Copy, Default)]
struct TypedDispatchOptions<'a> {
    path_override: Option<&'a str>,
    query: &'a [(String, String)],
    expands_to: Option<&'a str>,
    sse_accept: bool,
    extra_headers: Option<&'a [(String, String)]>,
    command_override: Option<&'a str>,
}

#[derive(Clone, Copy)]
struct TypedExecution<'a> {
    request_id: &'a str,
    pretty: bool,
    route: TypedRoute<'a>,
    command_override: Option<&'a str>,
}

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
        Command::Similar(args) => dispatch_similar(args, &cli.globals, pretty),
        Command::Answer(args) => dispatch_answer(args, &cli.globals, pretty),
        Command::Context(args) => dispatch_context(args, &cli.globals, pretty),
        Command::Team { sub } => dispatch_team(sub, &cli.globals, pretty),
        Command::Agent { sub } => dispatch_agent(sub, &cli.globals, pretty),
        Command::Research { sub } => dispatch_research(sub, &cli.globals, pretty),
        Command::Ask(args) => dispatch_ask(args, &cli.globals, pretty),
        Command::Fetch(args) => dispatch_fetch(args, &cli.globals, pretty),
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
        ("text", args.text.then_some("true".to_string())),
        ("summary-query", args.summary_query.clone()),
    ];
    build_typed_spec(op, &flag_values, globals)
}

fn dispatch_similar(
    args: &SimilarArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["similar"]).expect("similar is in the registry");
    with_typed_error_context(op, globals, || {
        let spec = build_similar_spec(args, globals)?;
        dispatch_typed_command(spec, globals, pretty)
    })
}

fn build_similar_spec(
    args: &SimilarArgs,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let op = registry::lookup_by_segments(&["similar"]).expect("similar is in the registry");
    let flag_values = [
        ("url", Some(args.url.clone())),
        ("num-results", args.num_results.map(|n| n.to_string())),
        (
            "exclude-source-domain",
            args.exclude_source_domain.then_some("true".to_string()),
        ),
        ("category", args.category.map(|c| c.as_str().to_string())),
    ];
    build_typed_spec(op, &flag_values, globals)
}

fn dispatch_answer(args: &AnswerArgs, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["answer"]).expect("answer is in the registry");
    with_typed_error_context(op, globals, || {
        let spec = build_answer_spec(args, globals)?;
        dispatch_typed_command(spec, globals, pretty)
    })
}

fn build_answer_spec(
    args: &AnswerArgs,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let op = registry::lookup_by_segments(&["answer"]).expect("answer is in the registry");
    let output_schema = args
        .output_schema
        .as_deref()
        .map(|raw| request::read_json_value_arg(raw, "output-schema"))
        .transpose()?
        .map(|value| value.to_string());
    let flag_values = [
        ("question", Some(args.question.clone())),
        ("text", args.text.then_some("true".to_string())),
        ("stream", args.stream.then_some("true".to_string())),
        ("output-schema", output_schema),
    ];
    build_typed_spec(op, &flag_values, globals)
}

fn dispatch_ask(args: &cli::AskArgs, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["answer"]).expect("answer is in the registry");
    with_typed_error_context(op, globals, || {
        let answer_args = AnswerArgs {
            question: args.question.clone(),
            text: true,
            stream: false,
            output_schema: None,
        };
        let spec = build_answer_spec(&answer_args, globals)?;
        let expands_to = format!("answer {} --text", shell_quote(&args.question));
        dispatch_typed_command_expanded(spec, globals, pretty, Some(expands_to.as_str()), None)
    })
}

fn dispatch_fetch(args: &FetchArgs, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["contents"]).expect("contents is in the registry");
    with_typed_error_context(op, globals, || {
        let contents_args = ContentsArgs {
            urls: args.urls.clone(),
            ids: Vec::new(),
            text: true,
            summary_query: Some("Summarize the page".to_string()),
            chunk_size: None,
        };
        let spec = build_contents_spec(&contents_args, globals)?;
        let specs = chunk_contents_specs(spec, None)?;
        let expands_to = format!(
            "contents {} --text --summary-query {}",
            shell_join(&args.urls),
            shell_quote("Summarize the page")
        );
        let spec = specs.into_iter().next().expect("one fetch contents spec");
        dispatch_typed_command_expanded(spec, globals, pretty, Some(expands_to.as_str()), None)
    })
}

fn dispatch_context(
    args: &ContextArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["context"]).expect("context is in the registry");
    with_typed_error_context(op, globals, || {
        let spec = build_context_spec(args, globals)?;
        dispatch_typed_command(spec, globals, pretty)
    })
}

fn build_context_spec(
    args: &ContextArgs,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let op = registry::lookup_by_segments(&["context"]).expect("context is in the registry");
    let tokens = args
        .tokens
        .as_deref()
        .map(context_tokens_value)
        .transpose()?
        .flatten()
        .map(|n| n.to_string());
    let flag_values = [("query", Some(args.query.clone())), ("tokens", tokens)];
    let spec = build_typed_spec(op, &flag_values, globals)?;
    validate_context_query_length(&spec.body)?;
    Ok(spec)
}

fn validate_context_query_length(body: &serde_json::Value) -> Result<(), CliError> {
    let Some(query) = body.get("query").and_then(serde_json::Value::as_str) else {
        return Ok(());
    };
    if query.chars().count() <= MAX_CONTEXT_QUERY_CHARS {
        return Ok(());
    }
    Err(CliError::Usage(
        Diag::new(
            "invalid_value",
            format!("context query must be at most {MAX_CONTEXT_QUERY_CHARS} characters"),
        )
        .with_suggestion("shorten the query or use exa-agent search/contents for broader input"),
    ))
}

fn context_tokens_value(raw: &str) -> Result<Option<u32>, CliError> {
    if raw.eq_ignore_ascii_case("dynamic") {
        return Ok(None);
    }
    let tokens = raw.parse::<u32>().map_err(|_| {
        CliError::Usage(
            Diag::new(
                "invalid_value",
                "`--tokens` must be `dynamic` or an integer between 50 and 100000",
            )
            .with_suggestion("exa-agent context QUERY --tokens dynamic"),
        )
    })?;
    if !(50..=100_000).contains(&tokens) {
        return Err(CliError::Usage(
            Diag::new("invalid_value", "`--tokens` must be between 50 and 100000")
                .with_suggestion("exa-agent context QUERY --tokens dynamic"),
        ));
    }
    Ok(Some(tokens))
}

fn dispatch_team(sub: &TeamCmd, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    match sub {
        TeamCmd::Info => dispatch_team_info(globals, pretty),
    }
}

fn dispatch_team_info(globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["team", "info"]).expect("team info is in the registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        dispatch_typed_command(spec, globals, pretty)
    })
}

fn dispatch_agent(sub: &AgentCmd, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    match sub {
        AgentCmd::Run(args) => {
            let expands_to = format!("agent runs create {}", shell_quote(&args.query));
            dispatch_agent_runs_create(
                args,
                globals,
                pretty,
                Some(expands_to.as_str()),
                Some("agent run"),
            )
        }
        AgentCmd::Runs { sub } => match sub {
            AgentRunsCmd::Create(args) => {
                dispatch_agent_runs_create(args, globals, pretty, None, None)
            }
            AgentRunsCmd::List(pagination) => dispatch_agent_runs_list(pagination, globals, pretty),
            AgentRunsCmd::Get { id } => dispatch_agent_runs_get(id, globals, pretty),
            AgentRunsCmd::Events(args) => dispatch_agent_runs_events(args, globals, pretty),
            AgentRunsCmd::Cancel { id } => dispatch_agent_runs_cancel(id, globals, pretty),
            AgentRunsCmd::Delete { id } => dispatch_agent_runs_delete(id, globals, pretty),
        },
    }
}

fn dispatch_agent_runs_create(
    args: &AgentRunArgs,
    globals: &GlobalArgs,
    pretty: bool,
    expands_to: Option<&str>,
    command_override: Option<&str>,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["agent", "runs", "create"]).expect("agent runs create");
    with_typed_error_context(op, globals, || {
        let spec = build_agent_run_spec(args, globals)?;
        if expands_to.is_some() || command_override.is_some() {
            dispatch_typed_command_expanded(spec, globals, pretty, expands_to, command_override)
        } else {
            dispatch_typed_command(spec, globals, pretty)
        }
    })
}

fn build_agent_run_spec(
    args: &AgentRunArgs,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let op = registry::lookup_by_segments(&["agent", "runs", "create"]).expect("agent runs create");
    let output_schema = args
        .output_schema
        .as_deref()
        .map(|raw| request::read_json_value_arg(raw, "output-schema"))
        .transpose()?
        .map(|value| value.to_string());
    let input = args
        .input
        .as_deref()
        .map(|raw| request::read_json_value_arg(raw, "input"))
        .transpose()?
        .map(|value| value.to_string());
    let input_rows = agent_input_rows_json(&args.input_row)?;
    let exclusion = args
        .exclusion
        .as_deref()
        .map(|raw| request::read_json_value_arg(raw, "exclusion"))
        .transpose()?
        .map(|value| value.to_string());
    let data_sources = agent_data_sources_json(&args.data_source)?;
    let metadata = args
        .metadata
        .as_deref()
        .map(|raw| request::read_json_value_arg(raw, "metadata"))
        .transpose()?
        .map(|value| value.to_string());
    let flag_values = [
        ("query", Some(args.query.clone())),
        ("output-schema", output_schema),
        ("input", input),
        ("input-row", input_rows),
        ("exclusion", exclusion),
        ("previous-run-id", args.previous_run_id.clone()),
        (
            "effort",
            args.effort.map(|effort| effort.as_str().to_string()),
        ),
        ("data-source", data_sources),
        ("metadata", metadata),
        ("stream", args.stream.then_some("true".to_string())),
    ];
    build_typed_spec(op, &flag_values, globals)
}

fn agent_input_rows_json(raw_rows: &[String]) -> Result<Option<String>, CliError> {
    if raw_rows.is_empty() {
        return Ok(None);
    }
    let mut rows = Vec::with_capacity(raw_rows.len());
    for raw in raw_rows {
        let row = request::read_json_value_arg(raw, "input-row")?;
        if !row.is_object() {
            return Err(CliError::Usage(Diag::new(
                "invalid_value",
                "`--input-row` must be a JSON object",
            )));
        }
        rows.push(row);
    }
    Ok(Some(serde_json::Value::Array(rows).to_string()))
}

fn agent_data_sources_json(providers: &[String]) -> Result<Option<String>, CliError> {
    if providers.is_empty() {
        return Ok(None);
    }
    if providers.len() > 5 {
        return Err(CliError::Usage(
            Diag::new(
                "invalid_value",
                "`--data-source` accepts at most 5 providers",
            )
            .with_suggestion("exa-agent agent runs create <query> --data-source similarweb"),
        ));
    }
    let mut sources = Vec::with_capacity(providers.len());
    for provider in providers {
        let provider = provider.trim();
        if provider.is_empty() {
            return Err(CliError::Usage(Diag::new(
                "invalid_value",
                "`--data-source` provider must not be empty",
            )));
        }
        sources.push(serde_json::json!({ "provider": provider }));
    }
    Ok(Some(serde_json::Value::Array(sources).to_string()))
}

fn dispatch_agent_runs_list(
    pagination: &PaginationArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["agent", "runs", "list"]).expect("agent runs list");
    with_typed_error_context(op, globals, || {
        validate_cursor_pagination(pagination)?;
        let spec = build_typed_spec(op, &[], globals)?;
        let query = pagination_query(pagination);
        if pagination.all && !(globals.print_request || globals.dry_run) {
            dispatch_paginated_typed_command(spec, globals, pretty, pagination, None)
        } else {
            dispatch_typed_command_routed(spec, globals, pretty, None, &query, false, None)
        }
    })
}

fn dispatch_agent_runs_get(id: &str, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["agent", "runs", "get"]).expect("agent runs get");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = substitute_path(op.api_path, &[("id", id)]);
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_agent_runs_events(
    args: &AgentRunsEventsArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["agent", "runs", "events"]).expect("agent runs events");
    with_typed_error_context(op, globals, || {
        validate_agent_runs_events_mode(args)?;
        validate_cursor_pagination(&args.pagination)?;
        let spec = build_typed_spec(op, &[], globals)?;
        let path = substitute_path(op.api_path, &[("id", &args.id)]);
        let query = pagination_query(&args.pagination);
        let extra_headers = agent_runs_events_headers(args);
        if args.pagination.all && !args.stream && !(globals.print_request || globals.dry_run) {
            dispatch_paginated_typed_command(
                spec,
                globals,
                pretty,
                &args.pagination,
                Some(path.as_str()),
            )
        } else {
            dispatch_typed_command_routed(
                spec,
                globals,
                pretty,
                Some(path.as_str()),
                &query,
                args.stream,
                Some(&extra_headers),
            )
        }
    })
}

fn validate_agent_runs_events_mode(args: &AgentRunsEventsArgs) -> Result<(), CliError> {
    if args.last_event_id.is_some() && !args.stream {
        return Err(CliError::Usage(
            Diag::new(
                "invalid_flag_combination",
                "`--last-event-id` requires `--stream` for SSE replay",
            )
            .with_suggestion(format!(
                "exa-agent agent runs events {} --stream --last-event-id <event-id>",
                args.id
            )),
        ));
    }
    if args.stream
        && (args.pagination.limit.is_some()
            || args.pagination.cursor.is_some()
            || args.pagination.all
            || args.pagination.max_pages.is_some()
            || args.pagination.page_delay.is_some())
    {
        return Err(CliError::Usage(
            Diag::new(
                "invalid_flag_combination",
                "`--stream` uses SSE replay; cursor pagination flags are only valid without `--stream`",
            )
            .with_suggestion(format!(
                "exa-agent agent runs events {} --stream --last-event-id <event-id>",
                args.id
            )),
        ));
    }
    Ok(())
}

fn agent_runs_events_headers(args: &AgentRunsEventsArgs) -> Vec<(String, String)> {
    let mut headers = Vec::new();
    if args.stream {
        headers.push(("Accept".to_string(), "text/event-stream".to_string()));
    }
    if let Some(last_event_id) = &args.last_event_id {
        headers.push(("Last-Event-ID".to_string(), last_event_id.clone()));
    }
    headers
}

fn dispatch_agent_runs_cancel(
    id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["agent", "runs", "cancel"]).expect("agent runs cancel");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = substitute_path(op.api_path, &[("id", id)]);
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_agent_runs_delete(
    id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["agent", "runs", "delete"]).expect("agent runs delete");
    with_typed_error_context(op, globals, || {
        ensure_destructive_confirmed(op, globals, id)?;
        let spec = build_typed_spec(op, &[], globals)?;
        let path = substitute_path(op.api_path, &[("id", id)]);
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn ensure_destructive_confirmed(
    op: &'static registry::OperationDef,
    globals: &GlobalArgs,
    id: &str,
) -> Result<(), CliError> {
    if !op.destructive() || globals.yes || globals.dry_run || globals.print_request {
        return Ok(());
    }
    Err(CliError::Safety(
        Diag::new(
            "confirmation_required",
            format!(
                "Refusing to delete agent run `{id}` without `--yes`; preview first with `agent runs get`"
            ),
        )
        .with_suggestion(format!("exa-agent agent runs delete {id} --yes")),
    ))
}

fn globals_with_extra_headers(globals: &GlobalArgs, extra: &[(String, String)]) -> GlobalArgs {
    let mut headers = globals.headers.clone();
    for (name, value) in extra {
        headers.push(format!("{name}:{value}"));
    }
    let mut merged = globals.clone();
    merged.headers = headers;
    merged
}

fn dispatch_research(
    sub: &ResearchCmd,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    match sub {
        ResearchCmd::Create(args) => dispatch_research_create(args, globals, pretty),
        ResearchCmd::List(pagination) => dispatch_research_list(pagination, globals, pretty),
        ResearchCmd::Get { research_id } => dispatch_research_get(research_id, globals, pretty),
    }
}

fn dispatch_research_create(
    args: &ResearchCreateArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["research", "create"])
        .expect("research create is in the registry");
    with_typed_error_context(op, globals, || {
        if args.stream {
            return Err(CliError::Usage(
                Diag::new(
                    "invalid_flag_combination",
                    "`research create --stream` is not supported by the upstream create endpoint",
                )
                .with_suggestion("exa-agent research create QUERY && exa-agent research get ID"),
            ));
        }
        let spec = build_research_create_spec(args, globals)?;
        dispatch_typed_command(spec, globals, pretty)
    })
}

fn build_research_create_spec(
    args: &ResearchCreateArgs,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let op = registry::lookup_by_segments(&["research", "create"])
        .expect("research create is in the registry");
    let flag_values = [("query", Some(args.query.clone()))];
    build_typed_spec(op, &flag_values, globals)
}

fn dispatch_research_list(
    pagination: &PaginationArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op =
        registry::lookup_by_segments(&["research", "list"]).expect("research list is in registry");
    with_typed_error_context(op, globals, || {
        validate_cursor_pagination(pagination)?;
        let spec = build_typed_spec(op, &[], globals)?;
        let query = pagination_query(pagination);
        if pagination.all && !(globals.print_request || globals.dry_run) {
            dispatch_paginated_typed_command(spec, globals, pretty, pagination, None)
        } else {
            dispatch_typed_command_routed(spec, globals, pretty, None, &query, false, None)
        }
    })
}

fn validate_cursor_pagination(pagination: &PaginationArgs) -> Result<(), CliError> {
    if !pagination.all && (pagination.max_pages.is_some() || pagination.page_delay.is_some()) {
        return Err(CliError::Usage(
            Diag::new(
                "invalid_flag_combination",
                "`--max-pages` and `--page-delay` require `--all` on cursor-paginated list commands",
            )
            .with_suggestion("exa-agent research list --all --max-pages 3"),
        ));
    }
    if pagination.max_pages == Some(0) {
        return Err(CliError::Usage(
            Diag::new("invalid_value", "--max-pages must be at least 1")
                .with_suggestion("exa-agent research list --all --max-pages 1"),
        ));
    }
    if let Some(raw) = &pagination.page_delay {
        parse_page_delay(raw)?;
    }
    Ok(())
}

fn dispatch_research_get(
    research_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op =
        registry::lookup_by_segments(&["research", "get"]).expect("research get is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = substitute_path(op.api_path, &[("researchId", research_id)]);
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn substitute_path(template: &str, params: &[(&str, &str)]) -> String {
    let mut path = template.to_string();
    for (key, value) in params {
        let value = transport::encode_path_segment(value);
        path = path.replace(&format!("{{{key}}}"), &value);
    }
    path
}

fn pagination_query(pagination: &PaginationArgs) -> Vec<(String, String)> {
    pagination_query_with_cursor(pagination, pagination.cursor.as_deref())
}

fn pagination_query_with_cursor(
    pagination: &PaginationArgs,
    cursor: Option<&str>,
) -> Vec<(String, String)> {
    let mut query = Vec::new();
    if let Some(limit) = pagination.limit {
        query.push(("limit".to_string(), limit.to_string()));
    }
    if let Some(cursor) = cursor {
        query.push(("cursor".to_string(), cursor.to_string()));
    }
    query
}

fn parse_page_delay(raw: &str) -> Result<Duration, CliError> {
    let raw = raw.trim();
    let parsed = if let Some(ms) = raw.strip_suffix("ms") {
        ms.parse::<u64>().ok().map(Duration::from_millis)
    } else if let Some(secs) = raw.strip_suffix('s') {
        secs.parse::<u64>().ok().map(Duration::from_secs)
    } else {
        raw.parse::<u64>().ok().map(Duration::from_secs)
    };
    parsed.ok_or_else(|| {
        CliError::Usage(
            Diag::new(
                "invalid_value",
                "--page-delay expects a duration like 250ms or 1s",
            )
            .with_suggestion("exa-agent research list --all --page-delay 250ms"),
        )
    })
}

fn next_cursor(data: &serde_json::Value) -> Option<String> {
    data.get("nextCursor")
        .or_else(|| data.get("next_cursor"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn has_more(data: &serde_json::Value, next: Option<&str>) -> bool {
    data.get("hasMore")
        .or_else(|| data.get("has_more"))
        .and_then(|value| value.as_bool())
        .unwrap_or_else(|| next.is_some())
}

fn primary_items(data: &serde_json::Value) -> Vec<serde_json::Value> {
    data.get("data")
        .or_else(|| data.get("items"))
        .or_else(|| data.get("results"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default()
}

fn shell_join(args: &[String]) -> String {
    args.iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", arg.replace('\'', "'\\''"))
}

fn query_preview(query: &[(String, String)]) -> Vec<serde_json::Value> {
    query
        .iter()
        .map(|(name, value)| {
            let value = if redaction::is_secret_name(name) {
                redaction::REDACTED.to_string()
            } else {
                redaction::scrub_text(value)
            };
            serde_json::json!({ "name": name, "value": value })
        })
        .collect()
}

fn header_preview(headers: &[(String, String)]) -> Vec<serde_json::Value> {
    headers
        .iter()
        .map(|(name, value)| {
            let value = if redaction::is_secret_name(name) {
                redaction::REDACTED.to_string()
            } else {
                redaction::scrub_text(value)
            };
            serde_json::json!({ "name": name, "value": value })
        })
        .collect()
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
    dispatch_typed_command_with_options(spec, globals, pretty, TypedDispatchOptions::default())
}

fn dispatch_typed_command_expanded(
    spec: request::RequestSpec,
    globals: &GlobalArgs,
    pretty: bool,
    expands_to: Option<&str>,
    command_override: Option<&str>,
) -> Result<i32, CliError> {
    dispatch_typed_command_with_options(
        spec,
        globals,
        pretty,
        TypedDispatchOptions {
            expands_to,
            command_override,
            ..TypedDispatchOptions::default()
        },
    )
}

fn dispatch_typed_command_routed(
    spec: request::RequestSpec,
    globals: &GlobalArgs,
    pretty: bool,
    path_override: Option<&str>,
    query: &[(String, String)],
    sse_accept: bool,
    extra_headers: Option<&[(String, String)]>,
) -> Result<i32, CliError> {
    dispatch_typed_command_with_options(
        spec,
        globals,
        pretty,
        TypedDispatchOptions {
            path_override,
            query,
            sse_accept,
            extra_headers,
            ..TypedDispatchOptions::default()
        },
    )
}

fn dispatch_typed_command_with_options(
    spec: request::RequestSpec,
    globals: &GlobalArgs,
    pretty: bool,
    options: TypedDispatchOptions<'_>,
) -> Result<i32, CliError> {
    let op = spec.op;
    let method = op.method.as_str();
    let path = options.path_override.unwrap_or(op.api_path);
    let request_id = if globals.print_request || globals.dry_run {
        "req_dry_run".to_string()
    } else {
        transport::new_request_id()
    };
    match dispatch_typed_inner(&spec, globals, pretty, &request_id, path, options) {
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
    path: &str,
    options: TypedDispatchOptions<'_>,
) -> Result<i32, CliError> {
    parse_user_headers(&globals.headers)?;
    if globals.print_request || globals.dry_run {
        emit_stdout(
            &redacted_preview_expanded(
                spec,
                path,
                options.query,
                options.expands_to,
                options.extra_headers,
                options.command_override,
            ),
            pretty,
        );
        return Ok(0);
    }

    let effective_globals = options
        .extra_headers
        .filter(|headers| !headers.is_empty())
        .map(|headers| globals_with_extra_headers(globals, headers))
        .unwrap_or_else(|| globals.clone());
    let api_input = credential_input(auth::CredentialNamespace::Api, &effective_globals)?;
    let credential = auth::resolve_api_credential(&api_input, &auth::NoopKeyring)
        .map_err(|missing| auth::not_authenticated_error(&missing))?;
    let cfg = config::Config::load()?;
    let timeout = transport::resolve_timeout(&effective_globals, &cfg);
    let transport = UreqTransport::new(timeout);
    execute_typed_live(
        &transport,
        spec,
        &effective_globals,
        &credential,
        TypedExecution {
            request_id,
            pretty,
            route: TypedRoute {
                path,
                query: options.query,
                sse_accept: options.sse_accept,
            },
            command_override: options.command_override,
        },
    )
}

fn dispatch_paginated_typed_command(
    spec: request::RequestSpec,
    globals: &GlobalArgs,
    pretty: bool,
    pagination: &PaginationArgs,
    path_override: Option<&str>,
) -> Result<i32, CliError> {
    if globals.raw {
        return Err(CliError::Usage(
            Diag::new(
                "invalid_flag_combination",
                "`--raw` cannot be combined with `--all`; request JSON or NDJSON pages instead",
            )
            .with_suggestion("exa-agent research list --all --ndjson"),
        ));
    }
    parse_user_headers(&globals.headers)?;
    let api_input = credential_input(auth::CredentialNamespace::Api, globals)?;
    let credential = auth::resolve_api_credential(&api_input, &auth::NoopKeyring)
        .map_err(|missing| auth::not_authenticated_error(&missing))?;
    let cfg = config::Config::load()?;
    let timeout = transport::resolve_timeout(globals, &cfg);
    let transport = UreqTransport::new(timeout);
    execute_paginated_live(
        &transport,
        &spec,
        globals,
        &credential,
        pretty,
        pagination,
        path_override,
    )
}

fn execute_paginated_live<T: Transport>(
    transport: &T,
    spec: &request::RequestSpec,
    globals: &GlobalArgs,
    credential: &auth::ResolvedCredential,
    pretty: bool,
    pagination: &PaginationArgs,
    path_override: Option<&str>,
) -> Result<i32, CliError> {
    let delay = pagination
        .page_delay
        .as_deref()
        .map(parse_page_delay)
        .transpose()?
        .unwrap_or_default();
    let max_pages = pagination.max_pages.unwrap_or(u32::MAX);
    let command = spec.op.command();
    let mut warnings = typed_command_warnings(spec.op);
    let env_output = std::env::var("EXA_OUTPUT").ok();
    let ndjson = matches!(
        resolve_mode(
            explicit_mode(globals),
            env_output.as_deref(),
            stdout_is_tty()
        ),
        OutputMode::Ndjson
    );
    let mut cursor = pagination.cursor.clone();
    let mut all_items = Vec::new();
    let mut first_request_id = String::new();
    let mut total_retries = 0u32;
    let mut page = 0u32;

    let (mut last_data, last_next, last_has_more) = loop {
        page += 1;
        let request_id = transport::new_request_id();
        if first_request_id.is_empty() {
            first_request_id = request_id.clone();
        }
        let query = pagination_query_with_cursor(pagination, cursor.as_deref());
        let query_raw: Vec<String> = query.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let path = path_override.unwrap_or(spec.op.api_path);
        let result = execute_raw_with_request_id(
            transport,
            RawExecuteParams {
                method: spec.op.method.as_str(),
                path,
                query_raw: &query_raw,
                body: typed_wire_body(spec),
                globals,
                credential,
                request_id,
            },
        )?;
        total_retries = total_retries.saturating_add(result.retries);
        let data = transport::parse_response_data(&result.response.body);
        let next = next_cursor(&data);
        let mut more = has_more(&data, next.as_deref());
        let reached_cap = more && page >= max_pages;
        if reached_cap {
            warnings.push(serde_json::json!({
                "code": "pagination_max_pages_reached",
                "message": "--max-pages stopped auto-pagination before the upstream cursor was exhausted.",
                "nextCursor": next.clone()
            }));
            more = true;
        } else if more && next.is_none() {
            warnings.push(serde_json::json!({
                "code": "pagination_missing_cursor",
                "message": "Upstream reported more pages but did not return nextCursor."
            }));
            more = false;
        } else if more && next == cursor {
            warnings.push(serde_json::json!({
                "code": "pagination_repeated_cursor",
                "message": "Upstream returned the same cursor twice; stopped to avoid an infinite loop."
            }));
            more = false;
        }

        all_items.extend(primary_items(&data));
        if ndjson {
            emit_ndjson(&page_envelope(
                &command,
                &result,
                data.clone(),
                result.retries,
                &warnings,
                PageInfo {
                    cursor: cursor.as_deref(),
                    next_cursor: next.as_deref(),
                    has_more: more,
                    page,
                    page_count: page,
                },
            ));
        }
        if !more || reached_cap {
            break (data, next, more);
        }
        cursor = next;
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
    };

    if !ndjson {
        if let Some(obj) = last_data.as_object_mut() {
            obj.insert("data".to_string(), serde_json::Value::Array(all_items));
        }
        let count = last_data
            .get("data")
            .and_then(|value| value.as_array())
            .map(|items| items.len() as u64);
        let hash = transport::data_hash(&last_data);
        let mut envelope = response_envelope(ResponseEnvelopeArgs {
            command: &command,
            method: spec.op.method.as_str(),
            path: path_override.unwrap_or(spec.op.api_path),
            request_id: &first_request_id,
            profile: &credential.profile,
            correlation_id: globals.correlation_id.as_deref(),
            data: last_data,
            count,
            data_hash: hash,
            retries: total_retries,
            warnings: &warnings,
        });
        set_pagination(
            &mut envelope,
            PageInfo {
                cursor: pagination.cursor.as_deref(),
                next_cursor: last_next.as_deref(),
                has_more: last_has_more,
                page,
                page_count: page,
            },
        );
        emit_stdout(&envelope, pretty);
    }
    Ok(0)
}

struct PageInfo<'a> {
    cursor: Option<&'a str>,
    next_cursor: Option<&'a str>,
    has_more: bool,
    page: u32,
    page_count: u32,
}

fn page_envelope(
    command: &str,
    result: &transport::RawExecuteResult,
    data: serde_json::Value,
    retries: u32,
    warnings: &[serde_json::Value],
    page: PageInfo<'_>,
) -> serde_json::Value {
    let count = transport::primary_count(&data);
    let hash = transport::data_hash(&data);
    let mut envelope = response_envelope(ResponseEnvelopeArgs {
        command,
        method: &result.method,
        path: &result.path,
        request_id: &result.request_id,
        profile: &result.profile,
        correlation_id: result.correlation_id.as_deref(),
        data,
        count,
        data_hash: hash,
        retries,
        warnings,
    });
    set_pagination(&mut envelope, page);
    envelope
}

fn set_pagination(envelope: &mut serde_json::Value, page: PageInfo<'_>) {
    envelope["pagination"] = serde_json::json!({
        "cursor": page.cursor,
        "nextCursor": page.next_cursor,
        "hasMore": page.has_more,
        "autoPaginated": true,
        "page": page.page,
        "pageCount": page.page_count,
        "total": null
    });
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
        match execute_typed_live(
            &transport,
            spec,
            globals,
            &credential,
            TypedExecution {
                request_id: &request_id,
                pretty: false,
                route: TypedRoute {
                    path: spec.op.api_path,
                    query: &[],
                    sse_accept: false,
                },
                command_override: None,
            },
        ) {
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

fn execute_typed_live<T: Transport>(
    transport: &T,
    spec: &request::RequestSpec,
    globals: &GlobalArgs,
    credential: &auth::ResolvedCredential,
    execution: TypedExecution<'_>,
) -> Result<i32, CliError> {
    let body = typed_wire_body(spec);
    let stream_requested = body_wants_stream(&body) || execution.route.sse_accept;
    let query_raw: Vec<String> = execution
        .route
        .query
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    let command = execution
        .command_override
        .map(str::to_string)
        .unwrap_or_else(|| spec.op.command());
    let warnings = typed_command_warnings(spec.op);
    if stream_requested {
        let params = RawExecuteParams {
            method: spec.op.method.as_str(),
            path: execution.route.path,
            query_raw: &query_raw,
            body,
            globals,
            credential,
            request_id: execution.request_id.to_string(),
        };
        return match execute_streaming_live(
            transport,
            params,
            &command,
            globals,
            execution.pretty,
            &warnings,
        ) {
            Ok(code) => Ok(code),
            Err(err) => Err(maybe_record_pending_run_on_create_failure(
                err,
                spec,
                globals,
                execution.request_id,
                execution.route.path,
            )),
        };
    }

    let result = match execute_raw_with_request_id(
        transport,
        RawExecuteParams {
            method: spec.op.method.as_str(),
            path: execution.route.path,
            query_raw: &query_raw,
            body: body.clone(),
            globals,
            credential,
            request_id: execution.request_id.to_string(),
        },
    ) {
        Ok(result) => result,
        Err(err) => {
            return Err(maybe_record_pending_run_on_create_failure(
                err,
                spec,
                globals,
                execution.request_id,
                execution.route.path,
            ));
        }
    };

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
        warnings: &warnings,
    });
    if globals.ndjson {
        emit_ndjson(&envelope);
    } else {
        emit_stdout(&envelope, execution.pretty);
    }
    Ok(exit_code)
}

fn execute_streaming_live<T: Transport>(
    transport: &T,
    params: RawExecuteParams<'_>,
    command: &str,
    globals: &GlobalArgs,
    pretty: bool,
    warnings: &[serde_json::Value],
) -> Result<i32, CliError> {
    let stream_mode = stream_output_mode(globals, stdout_is_tty());
    let ndjson = !globals.raw && stream_mode == OutputMode::Ndjson;
    let human = !globals.raw && stream_mode == OutputMode::Human;
    let mut seq = 0u64;
    let mut out = std::io::stdout().lock();
    let mut on_item = |item: StreamItem<'_>| -> Result<(), CliError> {
        match item {
            StreamItem::Bytes(bytes) if globals.raw => {
                use std::io::Write;
                out.write_all(bytes).map_err(|err| {
                    CliError::Interrupted(Diag::new(
                        "interrupted",
                        format!("failed to write raw stdout: {err}"),
                    ))
                })?;
            }
            StreamItem::Frame(frame) if ndjson => {
                write_stream_event_ndjson(
                    &mut out,
                    &frame,
                    command,
                    &mut seq,
                    globals.correlation_id.as_deref(),
                )?;
            }
            StreamItem::Frame(frame) if human => {
                write_stream_event_human(&mut out, &frame)?;
            }
            _ => {}
        }
        Ok(())
    };
    let (result, frames) = execute_raw_stream_with_request_id(transport, params, &mut on_item)?;
    if globals.raw {
        use std::io::Write;
        out.flush()
            .map_err(|err| stream_write_error(err, last_frame_event_id(&frames)))?;
        return Ok(0);
    }

    if frames.is_empty() {
        let data = transport::parse_response_data(&result.response.body);
        let envelope = response_envelope(ResponseEnvelopeArgs {
            command,
            method: &result.method,
            path: &result.path,
            request_id: &result.request_id,
            profile: &result.profile,
            correlation_id: result.correlation_id.as_deref(),
            count: transport::primary_count(&data),
            data_hash: transport::data_hash(&data),
            data,
            retries: result.retries,
            warnings,
        });
        if ndjson {
            write_ndjson(&mut out, &envelope).map_err(|err| stream_write_error(err, None))?;
        } else {
            write_stdout_value(&mut out, &envelope, pretty)
                .map_err(|err| stream_write_error(err, None))?;
        }
        return Ok(0);
    }

    let terminal_data = terminal_stream_data(&frames);
    let count = transport::primary_count(&terminal_data);
    let hash = transport::data_hash(&terminal_data);
    let terminal = response_envelope(ResponseEnvelopeArgs {
        command,
        method: &result.method,
        path: &result.path,
        request_id: &result.request_id,
        profile: &result.profile,
        correlation_id: result.correlation_id.as_deref(),
        data: terminal_data,
        count,
        data_hash: hash,
        retries: result.retries,
        warnings,
    });
    if ndjson {
        write_ndjson(&mut out, &terminal)
            .map_err(|err| stream_write_error(err, last_frame_event_id(&frames)))?;
    } else if human {
        use std::io::Write;
        out.flush()
            .map_err(|err| stream_write_error(err, last_frame_event_id(&frames)))?;
    } else {
        write_stdout_value(&mut out, &terminal, pretty)
            .map_err(|err| stream_write_error(err, last_frame_event_id(&frames)))?;
    }
    Ok(0)
}

fn write_stream_event_ndjson(
    out: &mut impl std::io::Write,
    frame: &transport::SseFrame,
    command: &str,
    seq: &mut u64,
    correlation_id: Option<&str>,
) -> Result<(), CliError> {
    for chunk in &frame.data {
        if chunk == "[DONE]" {
            continue;
        }
        let next_seq = *seq + 1;
        let event = serde_json::from_str::<serde_json::Value>(chunk)
            .unwrap_or_else(|_| serde_json::Value::String(chunk.clone()));
        let envelope = event_envelope(EventEnvelopeArgs {
            event_type: infer_stream_event_type(&event),
            command,
            seq: next_seq,
            event_id: frame.id.as_deref(),
            correlation_id,
            event,
        });
        write_ndjson(out, &envelope).map_err(|err| stream_write_error(err, None))?;
        *seq = next_seq;
    }
    Ok(())
}

fn write_stream_event_human(
    out: &mut impl std::io::Write,
    frame: &transport::SseFrame,
) -> Result<(), CliError> {
    for chunk in &frame.data {
        if chunk == "[DONE]" {
            continue;
        }
        out.write_all(chunk.as_bytes())
            .and_then(|_| out.write_all(b"\n"))
            .and_then(|_| out.flush())
            .map_err(|err| stream_write_error(err, None))?;
    }
    Ok(())
}

fn stream_write_error(err: std::io::Error, last_event_id: Option<&str>) -> CliError {
    let mut diag = Diag::new(
        "interrupted",
        format!("failed to write stream stdout: {err}"),
    );
    if let Some(last_event_id) = last_event_id {
        diag = diag.with_details(serde_json::json!({ "lastEventId": last_event_id }));
    }
    CliError::Interrupted(diag)
}

fn last_frame_event_id(frames: &[transport::SseFrame]) -> Option<&str> {
    frames.iter().rev().find_map(|frame| frame.id.as_deref())
}

fn maybe_record_pending_run_on_create_failure(
    err: CliError,
    spec: &request::RequestSpec,
    globals: &GlobalArgs,
    request_id: &str,
    path: &str,
) -> CliError {
    if !should_write_pending_run(&err, spec, globals) {
        return err;
    }

    let suggested = pending_recovery_command(spec.op);
    let pending_path = pending::pending_runs_path();
    let record = pending::PendingRunRecord::for_operation(
        spec.op,
        request_id,
        globals.idempotency_key.as_deref(),
        &suggested,
    );
    let write_result = pending::append_pending_run(&pending_path, &record);

    attach_pending_run_details(err, suggested, path, &pending_path, write_result)
}

fn should_write_pending_run(
    err: &CliError,
    spec: &request::RequestSpec,
    globals: &GlobalArgs,
) -> bool {
    spec.op.idempotency_sensitive
        && spec.op.method == registry::Method::Post
        && globals.idempotency_key.is_none()
        && matches!(
            err,
            CliError::Network(_) | CliError::Upstream(_) | CliError::RateLimit(_)
        )
}

fn pending_recovery_command(op: &'static registry::OperationDef) -> String {
    match op.command().as_str() {
        "agent runs create" => "exa-agent agent runs list --limit 10".to_string(),
        "research create" => "exa-agent research list --limit 10".to_string(),
        other => format!("exa-agent {other} --idempotency-key <stable-key>"),
    }
}

fn attach_pending_run_details(
    err: CliError,
    suggested: String,
    route_path: &str,
    pending_path: &std::path::Path,
    write_result: std::io::Result<()>,
) -> CliError {
    fn update(
        mut diag: Diag,
        suggested: String,
        route_path: &str,
        pending_path: &std::path::Path,
        write_result: std::io::Result<()>,
    ) -> Diag {
        diag.retryable = false;
        diag.suggested_command = Some(suggested);
        let mut details = diag
            .details
            .take()
            .map(|value| *value)
            .filter(|value| value.is_object())
            .unwrap_or_else(|| serde_json::json!({}));
        if let Some(obj) = details.as_object_mut() {
            obj.insert(
                "pendingRunPath".to_string(),
                serde_json::Value::String(pending_path.display().to_string()),
            );
            obj.insert(
                "pendingRunApiPath".to_string(),
                serde_json::Value::String(redaction::scrub_text(route_path)),
            );
            match write_result {
                Ok(()) => {
                    obj.insert(
                        "pendingRunWritten".to_string(),
                        serde_json::Value::Bool(true),
                    );
                }
                Err(err) => {
                    obj.insert(
                        "pendingRunWritten".to_string(),
                        serde_json::Value::Bool(false),
                    );
                    obj.insert(
                        "pendingRunWriteError".to_string(),
                        serde_json::Value::String(redaction::scrub_text(&err.to_string())),
                    );
                }
            }
        }
        diag.details = Some(Box::new(details));
        diag
    }

    match err {
        CliError::Network(diag) => CliError::Network(update(
            diag,
            suggested,
            route_path,
            pending_path,
            write_result,
        )),
        CliError::Upstream(diag) => CliError::Upstream(update(
            diag,
            suggested,
            route_path,
            pending_path,
            write_result,
        )),
        CliError::RateLimit(diag) => CliError::RateLimit(update(
            diag,
            suggested,
            route_path,
            pending_path,
            write_result,
        )),
        other => other,
    }
}

fn typed_wire_body(spec: &request::RequestSpec) -> serde_json::Value {
    let empty_object = spec
        .body
        .as_object()
        .map(|obj| obj.is_empty())
        .unwrap_or(false);
    if empty_object
        && matches!(
            spec.op.method,
            registry::Method::Get | registry::Method::Delete
        )
    {
        serde_json::Value::Null
    } else {
        spec.body.clone()
    }
}

#[cfg(test)]
fn stream_ndjson_values(
    result: &transport::RawExecuteResult,
    command: &str,
    warnings: &[serde_json::Value],
    terminal_data: serde_json::Value,
    count: Option<u64>,
    hash: Option<String>,
) -> Vec<serde_json::Value> {
    let frames = transport::parse_sse(&result.response.body);
    let mut values = Vec::new();
    let mut seq = 0u64;
    for frame in &frames {
        for chunk in &frame.data {
            if chunk == "[DONE]" {
                continue;
            }
            seq += 1;
            let event = serde_json::from_str::<serde_json::Value>(chunk)
                .unwrap_or_else(|_| serde_json::Value::String(chunk.clone()));
            values.push(event_envelope(EventEnvelopeArgs {
                event_type: infer_stream_event_type(&event),
                command,
                seq,
                event_id: frame.id.as_deref(),
                correlation_id: result.correlation_id.as_deref(),
                event,
            }));
        }
    }
    values.push(response_envelope(ResponseEnvelopeArgs {
        command,
        method: &result.method,
        path: &result.path,
        request_id: &result.request_id,
        profile: &result.profile,
        correlation_id: result.correlation_id.as_deref(),
        data: terminal_data,
        count,
        data_hash: hash,
        retries: result.retries,
        warnings,
    }));
    values
}

fn stream_output_mode(g: &GlobalArgs, stdout_is_tty: bool) -> OutputMode {
    let env_output = std::env::var("EXA_OUTPUT").ok();
    stream_output_mode_from_env(g, env_output.as_deref(), stdout_is_tty)
}

fn stream_output_mode_from_env(
    g: &GlobalArgs,
    env_output: Option<&str>,
    stdout_is_tty: bool,
) -> OutputMode {
    if explicit_mode(g).is_none() && env_output.is_none() && !stdout_is_tty {
        return OutputMode::Ndjson;
    }
    resolve_mode(explicit_mode(g), env_output, stdout_is_tty)
}

fn typed_command_warnings(op: &'static registry::OperationDef) -> Vec<serde_json::Value> {
    if op.cli_path.first() == Some(&"research") {
        return vec![serde_json::json!({
            "code": "legacy_api",
            "message": "The /research/v1 API is legacy; prefer `exa-agent agent run` for new work.",
            "replacement": "exa-agent agent run <query>"
        })];
    }
    if !op.deprecated {
        return Vec::new();
    }
    let warning = if op.operation_id == "findSimilar" || op.command() == "similar" {
        serde_json::json!({
            "code": "deprecated_upstream",
            "message": "POST /findSimilar is deprecated upstream; prefer exa-agent search with a query describing the source URL.",
            "replacement": "exa-agent search \"pages similar to <url>\" --num-results N"
        })
    } else {
        serde_json::json!({
            "code": "deprecated_upstream",
            "message": format!("{} is deprecated upstream.", op.command()),
        })
    };
    vec![warning]
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
    redacted_preview_expanded(spec, spec.op.api_path, &[], None, None, None)
}

fn redacted_preview_expanded(
    spec: &request::RequestSpec,
    path: &str,
    query: &[(String, String)],
    expands_to: Option<&str>,
    extra_headers: Option<&[(String, String)]>,
    command_override: Option<&str>,
) -> serde_json::Value {
    let mut body = typed_wire_body(spec);
    redaction::redact_json_value(&mut body);
    let command = command_override
        .map(str::to_string)
        .unwrap_or_else(|| spec.op.command());
    let warnings = typed_command_warnings(spec.op);
    let mut request = serde_json::json!({
        "method": spec.op.method.as_str(),
        "path": path,
        "query": query_preview(query),
        "body": body,
    });
    if let Some(headers) = extra_headers.filter(|headers| !headers.is_empty()) {
        request["headers"] = serde_json::Value::Array(header_preview(headers));
    } else if body_wants_stream(&body) {
        request["headers"] = serde_json::json!([{
            "name": "Accept",
            "value": "text/event-stream"
        }]);
    }
    let data = data_with_expands_to(
        serde_json::json!({
            "request": request,
            "dryRun": true,
        }),
        expands_to,
    );
    let count = transport::primary_count(data.get("request").unwrap_or(&data));
    let hash = transport::data_hash(&data);
    response_envelope(ResponseEnvelopeArgs {
        command: &command,
        method: spec.op.method.as_str(),
        path,
        request_id: "req_dry_run",
        profile: "default",
        correlation_id: None,
        data,
        count,
        data_hash: hash,
        retries: 0,
        warnings: &warnings,
    })
}

fn data_with_expands_to(
    mut data: serde_json::Value,
    expands_to: Option<&str>,
) -> serde_json::Value {
    let Some(expands_to) = expands_to else {
        return data;
    };
    let expands_to = redaction::scrub_text(expands_to);
    if let serde_json::Value::Object(obj) = &mut data {
        obj.insert(
            "expandsTo".to_string(),
            serde_json::Value::String(expands_to.clone()),
        );
        obj.insert(
            "expands_to".to_string(),
            serde_json::Value::String(expands_to),
        );
    }
    data
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
                warnings: &[],
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
    if body_wants_stream(&body) {
        return execute_streaming_live(
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
            "raw",
            globals,
            pretty,
            &[],
        );
    }
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
        warnings: &[],
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{CredentialInput, NoopKeyring};
    use crate::registry::{FieldDef, Method, Namespace, OperationDef, Pagination};
    use crate::transport::{FakeTransport, HttpResponse, RawExecuteResult};

    static GENERIC_DEPRECATED_OP: OperationDef = OperationDef {
        cli_path: &["old"],
        operation_id: "oldThing",
        method: Method::Get,
        api_path: "/old",
        read_only: true,
        streaming: false,
        pagination: Pagination::None,
        dangerous: false,
        namespace: Namespace::Api,
        idempotency_sensitive: false,
        deprecated: true,
        source: "test",
        source_version: "0",
        fields: &[] as &[FieldDef],
    };

    struct PendingPathGuard;

    impl PendingPathGuard {
        fn set(path: std::path::PathBuf) -> Self {
            pending::set_test_pending_runs_path(Some(path));
            Self
        }
    }

    impl Drop for PendingPathGuard {
        fn drop(&mut self) {
            pending::set_test_pending_runs_path(None);
        }
    }

    fn parse_globals(args: &[&str]) -> GlobalArgs {
        let argv: Vec<_> = std::iter::once("exa-agent")
            .chain(args.iter().copied())
            .chain(std::iter::once("capabilities"))
            .collect();
        Cli::try_parse_from(argv).unwrap().globals
    }

    #[test]
    fn stream_mode_honors_explicit_env_and_piped_default() {
        let defaults = parse_globals(&[]);
        assert_eq!(
            stream_output_mode_from_env(&defaults, None, false),
            OutputMode::Ndjson
        );
        assert_eq!(
            stream_output_mode_from_env(&defaults, Some("ndjson"), true),
            OutputMode::Ndjson
        );

        let explicit_json = parse_globals(&["--format", "json"]);
        assert_eq!(
            stream_output_mode_from_env(&explicit_json, Some("ndjson"), false),
            OutputMode::Json
        );

        let explicit_ndjson = parse_globals(&["--format", "ndjson"]);
        assert_eq!(
            stream_output_mode_from_env(&explicit_ndjson, Some("json"), true),
            OutputMode::Ndjson
        );
    }

    #[test]
    fn stream_ndjson_terminal_line_prefers_final_answer_object() {
        let result = RawExecuteResult {
            request_id: "req_test".into(),
            method: "POST".into(),
            path: "/answer".into(),
            profile: "default".into(),
            correlation_id: Some("corr-test".into()),
            response: HttpResponse {
                status: 200,
                headers: vec![("content-type".into(), "text/event-stream".into())],
                body: b"id: evt-1\ndata: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\nid: evt-2\ndata: {\"answer\":\"done\",\"citations\":[]}\n\ndata: [DONE]\n\n".to_vec(),
            },
            retries: 0,
        };
        let frames = transport::parse_sse(&result.response.body);
        let terminal_data = terminal_stream_data(&frames);
        let lines = stream_ndjson_values(
            &result,
            "answer",
            &[],
            terminal_data.clone(),
            transport::primary_count(&terminal_data),
            transport::data_hash(&terminal_data),
        );

        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["schema"], "exa.cli.event.v1");
        assert_eq!(
            lines[0]["event"]["choices"][0]["delta"]["content"],
            "partial"
        );
        assert_eq!(lines[2]["schema"], "exa.cli.response.v1");
        assert_eq!(
            lines[2]["data"],
            serde_json::json!({"answer":"done","citations":[]})
        );
    }

    struct FailAfterWrites {
        remaining: usize,
        bytes: Vec<u8>,
    }

    impl std::io::Write for FailAfterWrites {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            if self.remaining == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::BrokenPipe,
                    "stdout closed",
                ));
            }
            self.remaining -= 1;
            self.bytes.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn stream_event_ndjson_write_failure_returns_interrupted() {
        let mut out = FailAfterWrites {
            remaining: 2,
            bytes: Vec::new(),
        };
        let mut seq = 0;
        let first = transport::SseFrame {
            id: Some("evt-1".into()),
            data: vec![r#"{"type":"progress","message":"first"}"#.into()],
        };
        write_stream_event_ndjson(&mut out, &first, "agent runs events", &mut seq, None).unwrap();
        assert_eq!(seq, 1);

        let second = transport::SseFrame {
            id: Some("evt-2".into()),
            data: vec![r#"{"type":"progress","message":"second"}"#.into()],
        };
        let err = write_stream_event_ndjson(&mut out, &second, "agent runs events", &mut seq, None)
            .unwrap_err();

        assert_eq!(err.category(), 12);
        assert_eq!(err.diag().code, "interrupted");
        assert_eq!(seq, 1);
        assert!(String::from_utf8_lossy(&out.bytes).contains("\"eventId\":\"evt-1\""));
    }

    #[test]
    fn stream_event_human_writes_progressive_lines() {
        let mut out = Vec::new();
        let frame = transport::SseFrame {
            id: Some("evt-1".into()),
            data: vec![r#"{"message":"first"}"#.into(), "[DONE]".into()],
        };

        write_stream_event_human(&mut out, &frame).unwrap();

        assert_eq!(out, b"{\"message\":\"first\"}\n");
    }

    #[test]
    fn stream_event_human_write_failure_returns_interrupted() {
        let mut out = FailAfterWrites {
            remaining: 1,
            bytes: Vec::new(),
        };
        let frame = transport::SseFrame {
            id: Some("evt-1".into()),
            data: vec![r#"{"message":"first"}"#.into()],
        };

        let err = write_stream_event_human(&mut out, &frame).unwrap_err();

        assert_eq!(err.category(), 12);
        assert_eq!(err.diag().code, "interrupted");
    }

    #[test]
    fn execute_typed_live_accepts_fake_transport_for_streams() {
        let fake = FakeTransport::default();
        fake.push_ok_json(200, "data: {\"answer\":\"done\",\"citations\":[]}\n\n");
        let globals = parse_globals(&["--format", "json", "--api-key", "test-key-abcdef12"]);
        let spec = build_answer_spec(
            &AnswerArgs {
                question: "What is Exa?".into(),
                text: false,
                stream: true,
                output_schema: None,
            },
            &globals,
        )
        .unwrap();
        let credential = auth::resolve_api_credential(
            &CredentialInput {
                explicit: Some("test-key-abcdef12".into()),
                ..Default::default()
            },
            &NoopKeyring,
        )
        .unwrap();

        assert_eq!(
            execute_typed_live(
                &fake,
                &spec,
                &globals,
                &credential,
                TypedExecution {
                    request_id: "req_test",
                    pretty: false,
                    route: TypedRoute {
                        path: spec.op.api_path,
                        query: &[],
                        sse_accept: false,
                    },
                    command_override: None,
                },
            )
            .unwrap(),
            0
        );
        assert!(fake.recorded_requests()[0]
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("accept") && v == "text/event-stream"));
    }

    #[test]
    fn typed_get_live_omits_empty_object_body() {
        let fake = FakeTransport::default();
        fake.push_ok_json(200, r#"{"team":"ok"}"#);
        let globals = parse_globals(&["--format", "json", "--api-key", "test-key-abcdef12"]);
        let op = registry::lookup_by_segments(&["team", "info"]).unwrap();
        let spec = request::build_body(op, &[]).unwrap();
        let credential = auth::resolve_api_credential(
            &CredentialInput {
                explicit: Some("test-key-abcdef12".into()),
                ..Default::default()
            },
            &NoopKeyring,
        )
        .unwrap();

        assert_eq!(
            execute_typed_live(
                &fake,
                &spec,
                &globals,
                &credential,
                TypedExecution {
                    request_id: "req_test",
                    pretty: false,
                    route: TypedRoute {
                        path: spec.op.api_path,
                        query: &[],
                        sse_accept: false,
                    },
                    command_override: None,
                },
            )
            .unwrap(),
            0
        );
        let recorded = fake.recorded_requests();
        assert_eq!(recorded[0].method, "GET");
        assert!(recorded[0].body.is_none());
    }

    #[test]
    fn paginated_research_list_follows_next_cursor() {
        let fake = FakeTransport::default();
        fake.push_ok_json(
            200,
            r#"{"data":[{"researchId":"r1"}],"hasMore":true,"nextCursor":"cur2"}"#,
        );
        fake.push_ok_json(
            200,
            r#"{"data":[{"researchId":"r2"}],"hasMore":false,"nextCursor":null}"#,
        );
        let globals = parse_globals(&[
            "--format",
            "json",
            "--api-key",
            "test-key-abcdef12",
            "--base-url",
            "http://example.test",
        ]);
        let op = registry::lookup_by_segments(&["research", "list"]).unwrap();
        let spec = request::build_body(op, &[]).unwrap();
        let credential = auth::resolve_api_credential(
            &CredentialInput {
                explicit: Some("test-key-abcdef12".into()),
                ..Default::default()
            },
            &NoopKeyring,
        )
        .unwrap();
        let pagination = PaginationArgs {
            limit: Some(1),
            all: true,
            ..Default::default()
        };

        assert_eq!(
            execute_paginated_live(
                &fake,
                &spec,
                &globals,
                &credential,
                false,
                &pagination,
                None,
            )
            .unwrap(),
            0
        );
        let recorded = fake.recorded_requests();
        assert_eq!(recorded.len(), 2);
        assert!(recorded[0].url.ends_with("/research/v1?limit=1"));
        assert!(recorded[1].url.contains("limit=1"));
        assert!(recorded[1].url.contains("cursor=cur2"));
    }

    #[test]
    fn golden_pending_run_record() {
        let pending_path = std::env::temp_dir().join(format!(
            "exa-agent-pending-lib-{}-{}.jsonl",
            std::process::id(),
            transport::new_request_id()
        ));
        let _ = std::fs::remove_file(&pending_path);
        let _pending_override = PendingPathGuard::set(pending_path.clone());

        let fake = FakeTransport::default();
        let mut diag = Diag::new("network_error", "connection reset after send");
        diag.retryable = true;
        fake.push_err(CliError::Network(diag));

        let globals = parse_globals(&["--format", "json", "--api-key", "test-key-abcdef12"]);
        let op = registry::lookup_by_segments(&["agent", "runs", "create"]).unwrap();
        let spec = request::build_request(
            op,
            &[("query", Some("find eval tools".to_string()))],
            RequestOverrides::default(),
        )
        .unwrap();
        let credential = auth::resolve_api_credential(
            &CredentialInput {
                explicit: Some("test-key-abcdef12".into()),
                ..Default::default()
            },
            &NoopKeyring,
        )
        .unwrap();

        let err = execute_typed_live(
            &fake,
            &spec,
            &globals,
            &credential,
            TypedExecution {
                request_id: "req_pending",
                pretty: false,
                route: TypedRoute {
                    path: spec.op.api_path,
                    query: &[],
                    sse_accept: false,
                },
                command_override: None,
            },
        )
        .unwrap_err();

        assert_eq!(fake.recorded_requests().len(), 1);
        assert_eq!(
            err.diag().suggested_command.as_deref(),
            Some("exa-agent agent runs list --limit 10")
        );
        assert!(!err.diag().retryable);
        assert_eq!(
            err.diag().details.as_ref().unwrap()["pendingRunWritten"],
            true
        );

        let raw = std::fs::read_to_string(&pending_path).unwrap();
        let lines: Vec<_> = raw.lines().collect();
        assert_eq!(lines.len(), 1);
        let record: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(record["schema"], pending::SCHEMA);
        assert_eq!(record["command"], "agent runs create");
        assert_eq!(record["operationId"], "createAgentRun");
        assert_eq!(record["apiPath"], "/agent/runs");
        assert_eq!(record["requestId"], "req_pending");
        assert_eq!(
            record["recoveryCommand"],
            "exa-agent agent runs list --limit 10"
        );

        let _ = std::fs::remove_file(&pending_path);
    }

    #[test]
    fn live_macro_metadata_does_not_pollute_data_or_hash() {
        let upstream = serde_json::json!({"answer":"done","citations":[]});
        let count = transport::primary_count(&upstream);
        let hash = transport::data_hash(&upstream);
        let envelope = response_envelope(ResponseEnvelopeArgs {
            command: "answer",
            method: "POST",
            path: "/answer",
            request_id: "req_test",
            profile: "default",
            correlation_id: None,
            data: upstream,
            count,
            data_hash: hash.clone(),
            retries: 0,
            warnings: &[],
        });

        assert!(envelope["data"].get("expandsTo").is_none());
        assert!(envelope["data"].get("expands_to").is_none());
        assert_eq!(envelope["dataHash"].as_str(), hash.as_deref());
    }

    #[test]
    fn context_query_limit_counts_chars_not_bytes() {
        let globals = parse_globals(&[]);
        let two_thousand_multibyte = "é".repeat(2_000);
        let spec = build_context_spec(
            &ContextArgs {
                query: two_thousand_multibyte.clone(),
                tokens: None,
            },
            &globals,
        )
        .unwrap();
        assert_eq!(spec.body["query"], two_thousand_multibyte);

        let err = build_context_spec(
            &ContextArgs {
                query: "é".repeat(2_001),
                tokens: None,
            },
            &globals,
        )
        .unwrap_err();
        assert_eq!(err.diag().code, "invalid_value");
    }

    #[test]
    fn deprecated_warning_is_specific_only_for_similar() {
        let similar = registry::lookup_by_segments(&["similar"]).unwrap();
        let specific = typed_command_warnings(similar);
        assert!(specific[0]["message"]
            .as_str()
            .unwrap()
            .contains("/findSimilar"));

        let generic = typed_command_warnings(&GENERIC_DEPRECATED_OP);
        assert_eq!(generic[0]["code"], "deprecated_upstream");
        assert!(!generic[0]["message"]
            .as_str()
            .unwrap()
            .contains("/findSimilar"));
    }

    #[test]
    fn research_commands_emit_legacy_api_warning() {
        let create = registry::lookup_by_segments(&["research", "create"]).unwrap();
        let list = registry::lookup_by_segments(&["research", "list"]).unwrap();
        let get = registry::lookup_by_segments(&["research", "get"]).unwrap();
        for op in [create, list, get] {
            let warnings = typed_command_warnings(op);
            assert_eq!(warnings.len(), 1);
            assert_eq!(warnings[0]["code"], "legacy_api");
            assert!(warnings[0]["replacement"]
                .as_str()
                .unwrap()
                .contains("agent run"));
        }
    }

    #[test]
    fn substitute_path_encodes_template_segments() {
        assert_eq!(
            substitute_path("/research/v1/{researchId}", &[("researchId", "abc/def")]),
            "/research/v1/abc%2Fdef"
        );
    }
}
