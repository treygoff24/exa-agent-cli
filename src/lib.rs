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
use time::{Date, Duration as TimeDuration, OffsetDateTime, PrimitiveDateTime};

use cli::{
    AdminCmd, AdminKeysCmd, AdminKeysCreateArgs, AgentCmd, AgentRunArgs, AgentRunsCmd,
    AgentRunsEventsArgs, AnswerArgs, AuthCmd, Cli, Command, ConfigCmd, ConfigProfilesCmd,
    ContentsArgs, ContextArgs, FetchArgs, GlobalArgs, GroupBy, MonitorBatchArgs, MonitorCmd,
    MonitorCreateArgs, MonitorListArgs, MonitorRunsCmd, PaginationArgs, ResearchCmd,
    ResearchCreateArgs, RobotDocsCmd, SchemaCmd, SearchArgs, SimilarArgs, TeamCmd,
    WebsetEnrichmentFormat, WebsetsCmd, WebsetsCreateArgs, WebsetsEventsListArgs,
    WebsetsImportsCmd, WebsetsListArgs, WebsetsMonitorsCreateArgs, WebsetsMonitorsListArgs,
    WebsetsMonitorsUpdateArgs, WebsetsPreviewArgs, WebsetsWebhookAttemptsListArgs,
    WebsetsWebhooksCreateArgs, WebsetsWebhooksUpdateArgs, SEARCH_CATEGORY_VALUES,
    SEARCH_TYPE_VALUES,
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
struct TypedPreviewOptions<'a> {
    path: &'a str,
    query: &'a [(String, String)],
    expands_to: Option<&'a str>,
    extra_headers: Option<&'a [(String, String)]>,
    command_override: Option<&'a str>,
    globals: Option<&'a GlobalArgs>,
    warnings: &'a [serde_json::Value],
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
    use clap::error::{ContextKind, ErrorKind};
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
            let mut details = serde_json::Map::new();
            let mut message = first_line(&e.to_string());
            let mut suggestion = None;

            if matches!(kind, ErrorKind::MissingRequiredArgument) {
                let missing = clap_ctx_strings(&e, ContextKind::InvalidArg);
                if !missing.is_empty() {
                    message = format!("missing required argument(s): {}", missing.join(", "));
                    details.insert("missing".to_string(), serde_json::json!(missing));
                }
            }

            let suggested_kind = match kind {
                ErrorKind::UnknownArgument => Some(ContextKind::SuggestedArg),
                ErrorKind::InvalidSubcommand => Some(ContextKind::SuggestedSubcommand),
                ErrorKind::InvalidValue | ErrorKind::ValueValidation => {
                    Some(ContextKind::SuggestedValue)
                }
                _ => None,
            };
            if let Some(kind) = suggested_kind {
                // clap orders suggestions by ascending similarity (most similar LAST)
                // and emits none when there is no good match. Trust it rather than
                // re-deriving a suggestion ourselves — an ad-hoc edit-distance pass
                // invents false matches for nonsense input and can point an agent at a
                // destructive command (`websets event` → `delete`).
                if let Some(did_you_mean) = clap_ctx_strings(&e, kind).into_iter().last() {
                    details.insert(
                        "didYouMean".to_string(),
                        serde_json::Value::String(did_you_mean.clone()),
                    );
                    suggestion = Some(format!("Did you mean `{did_you_mean}`?"));
                }
            }

            let diag = Diag::new(code, message);
            let diag = if details.is_empty() {
                diag
            } else {
                diag.with_details(serde_json::Value::Object(details))
            };
            let diag = if let Some(suggestion) = suggestion {
                diag.with_suggestion(suggestion)
            } else {
                diag
            };
            let err = CliError::Usage(diag);
            let env = ErrorEnvelope::from_error(&err);
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&env.to_json()).unwrap_or_default()
            );
            err.category() as i32
        }
    }
}

fn clap_ctx_strings(e: &clap::Error, kind: clap::error::ContextKind) -> Vec<String> {
    use clap::error::ContextValue;

    match e.get(kind) {
        Some(ContextValue::String(value)) => vec![value.clone()],
        Some(ContextValue::Strings(values)) => values.clone(),
        Some(ContextValue::StyledStr(value)) => vec![value.to_string()],
        Some(ContextValue::StyledStrs(values)) => values.iter().map(ToString::to_string).collect(),
        _ => Vec::new(),
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
        Command::Schema { sub } => dispatch_schema(sub, &cli.globals, pretty),
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
        Command::Monitor { sub } => dispatch_monitor(sub, &cli.globals, pretty),
        Command::Websets { sub } => dispatch_websets(sub, &cli.globals, pretty),
        Command::Team { sub } => dispatch_team(sub, &cli.globals, pretty),
        Command::Admin { sub } => dispatch_admin(sub, &cli.globals, pretty),
        Command::Agent { sub } => dispatch_agent(sub, &cli.globals, pretty),
        Command::Research { sub } => dispatch_research(sub, &cli.globals, pretty),
        Command::Ask(args) => dispatch_ask(args, &cli.globals, pretty),
        Command::Fetch(args) => dispatch_fetch(args, &cli.globals, pretty),
        Command::Raw(args) => dispatch_raw(args, &cli.globals, pretty),
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
    validate_search_intent_args(args)?;
    let flag_values = [
        ("query", Some(args.query.clone())),
        ("num-results", args.num_results.clone()),
        ("text", args.text.then_some("true".to_string())),
        ("type", args.r#type.map(|t| t.as_str().to_string())),
        ("category", args.category.clone()),
        (
            "include-domain",
            (!args.include_domain.is_empty())
                .then(|| request::encode_str_array(&args.include_domain)),
        ),
        (
            "exclude-domain",
            (!args.exclude_domain.is_empty())
                .then(|| request::encode_str_array(&args.exclude_domain)),
        ),
        ("start-published-date", args.start_published_date.clone()),
        ("end-published-date", args.end_published_date.clone()),
    ];
    let mut spec = build_typed_spec(op, &flag_values, globals)?;
    normalize_and_validate_search_body(&mut spec.body, &args.query)?;
    Ok(spec)
}

fn validate_search_intent_args(args: &SearchArgs) -> Result<(), CliError> {
    if let Some(num_results) = args.num_results.as_deref() {
        validate_search_num_results(&args.query, num_results)?;
    }

    if let Some(limit) = args.limit.as_deref() {
        return Err(CliError::Usage(
            Diag::new(
                "invalid_flag_combination",
                "search is not cursor-paginated; use `--num-results N` (1..100), not `--limit`",
            )
            .with_suggestion(format!(
                "exa-agent search {} --num-results {}",
                shell_quote(&args.query),
                replacement_positive_int(limit, Some(100))
            )),
        ));
    }

    if let Some(count) = args.count.as_deref() {
        return Err(CliError::Usage(
            Diag::new(
                "invalid_flag_combination",
                "search uses `--num-results`, not Websets-style `--count`",
            )
            .with_suggestion(format!(
                "exa-agent search {} --num-results {}",
                shell_quote(&args.query),
                replacement_positive_int(count, Some(100))
            )),
        ));
    }

    if args.all {
        return Err(CliError::Usage(
            Diag::new(
                "invalid_flag_combination",
                "`--all` is only for cursor-paginated list commands; search returns at most 100 results",
            )
            .with_suggestion(format!(
                "exa-agent search {} --num-results 100",
                shell_quote(&args.query)
            )),
        ));
    }

    if let Some(filter) = args.filter.as_deref() {
        let suggestion = search_filter_suggestion(&args.query, filter);
        return Err(CliError::Usage(
            Diag::new(
                "invalid_flag_combination",
                "`search --filter` is not a v1 flag; use typed filters such as `--category`, `--include-domain`, or `--exclude-domain`",
            )
            .with_suggestion(suggestion),
        ));
    }

    Ok(())
}

fn validate_search_num_results(query: &str, raw: &str) -> Result<(), CliError> {
    if matches!(raw.parse::<u32>(), Ok(1..=100)) {
        return Ok(());
    }

    Err(CliError::Usage(
        Diag::new(
            "invalid_value",
            "`search --num-results` must be an integer between 1 and 100",
        )
        .with_details(serde_json::json!({ "min": 1, "max": 100, "received": raw }))
        .with_suggestion(format!(
            "exa-agent search {} --num-results {}",
            shell_quote(query),
            replacement_positive_int(raw, Some(100))
        )),
    ))
}

fn replacement_positive_int(raw: &str, max: Option<u32>) -> u32 {
    let parsed = raw.parse::<u64>().ok().filter(|value| *value >= 1);
    let value = parsed.unwrap_or(1);
    let max = max.unwrap_or(u32::MAX) as u64;
    value.min(max).min(u32::MAX as u64) as u32
}

fn search_filter_suggestion(query: &str, filter: &str) -> String {
    if let Some((key, value)) = filter.split_once('=') {
        let normalized_key = normalize_filter_key(key);
        let flag = match normalized_key.as_str() {
            "category" => Some("category"),
            "domain" | "domains" | "includedomain" | "includedomains" => Some("include-domain"),
            "excludedomain" | "excludedomains" => Some("exclude-domain"),
            "startpublisheddate" | "publishedafter" => Some("start-published-date"),
            "endpublisheddate" | "publishedbefore" => Some("end-published-date"),
            _ => None,
        };
        if let Some(flag) = flag {
            if flag == "category" {
                let Some(category) = suggested_search_category(value) else {
                    return "exa-agent schema show search --compact".to_string();
                };
                return format!(
                    "exa-agent search {} --category {}",
                    shell_quote(query),
                    shell_quote(category)
                );
            }
            return format!(
                "exa-agent search {} --{} {}",
                shell_quote(query),
                flag,
                shell_quote(value)
            );
        }
    }

    "exa-agent schema show search --compact".to_string()
}

fn normalize_filter_key(key: &str) -> String {
    key.chars()
        .filter(|ch| !matches!(ch, '-' | '_'))
        .collect::<String>()
        .to_ascii_lowercase()
}

fn normalize_and_validate_search_body(
    body: &mut serde_json::Value,
    query: &str,
) -> Result<(), CliError> {
    validate_search_num_results_body(body, query)?;

    if let Some(raw) = body.get("category").cloned() {
        let Some(raw) = raw.as_str() else {
            return Err(CliError::Usage(
                Diag::new(
                    "invalid_value",
                    "search category must be a string; valid categories are company, people, research paper, news, personal site, financial report",
                )
                .with_details(serde_json::json!({ "validCategories": SEARCH_CATEGORY_VALUES }))
                .with_suggestion("exa-agent schema show search --compact"),
            ));
        };
        let category = canonical_search_category(raw, query)?;
        body["category"] = serde_json::Value::String(category.to_string());
    }

    validate_search_category_filter_combinations(body, query)
}

fn validate_search_num_results_body(body: &serde_json::Value, query: &str) -> Result<(), CliError> {
    let Some(raw) = body.get("numResults") else {
        return Ok(());
    };
    if matches!(raw.as_u64(), Some(1..=100)) {
        return Ok(());
    }

    let received = match raw {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    Err(CliError::Usage(
        Diag::new(
            "invalid_value",
            "`search numResults` must be an integer between 1 and 100",
        )
        .with_details(serde_json::json!({
            "min": 1,
            "max": 100,
            "received": raw,
        }))
        .with_suggestion(format!(
            "exa-agent search {} --num-results {}",
            shell_quote(query),
            replacement_positive_int(&received, Some(100))
        )),
    ))
}

fn canonical_search_category(raw: &str, query: &str) -> Result<&'static str, CliError> {
    if let Some(category) = exact_search_category(raw) {
        return Ok(category);
    }

    let did_you_mean = search_category_alias(raw);

    let mut details = serde_json::json!({ "validCategories": SEARCH_CATEGORY_VALUES });
    if let Some(suggestion) = did_you_mean {
        details["didYouMean"] = serde_json::Value::String(suggestion.to_string());
    }

    let suggested_command = if let Some(suggested_category) = did_you_mean {
        format!(
            "exa-agent search {} --category {}",
            shell_quote(query),
            shell_quote(suggested_category)
        )
    } else {
        "exa-agent schema show search --compact".to_string()
    };

    Err(CliError::Usage(
        Diag::new(
            "invalid_value",
            format!(
                "invalid search category `{raw}`; valid categories are company, people, research paper, news, personal site, financial report"
            ),
        )
        .with_details(details)
        .with_suggestion(suggested_command),
    ))
}

fn suggested_search_category(raw: &str) -> Option<&'static str> {
    exact_search_category(raw).or_else(|| search_category_alias(raw))
}

fn exact_search_category(raw: &str) -> Option<&'static str> {
    let lower = raw.trim().to_ascii_lowercase();
    match lower.as_str() {
        "company" => Some("company"),
        "people" => Some("people"),
        "research paper" => Some("research paper"),
        "news" => Some("news"),
        "personal site" => Some("personal site"),
        "financial report" => Some("financial report"),
        _ => None,
    }
}

fn search_category_alias(raw: &str) -> Option<&'static str> {
    let lower = raw.trim().to_ascii_lowercase();
    let compact: String = lower
        .chars()
        .filter(|ch| !matches!(ch, ' ' | '-' | '_'))
        .collect();
    match compact.as_str() {
        "companys" | "companies" => Some("company"),
        "person" | "peoples" => Some("people"),
        "researchpaper" => Some("research paper"),
        "personalsite" => Some("personal site"),
        "financialreport" => Some("financial report"),
        _ => None,
    }
}

fn validate_search_category_filter_combinations(
    body: &serde_json::Value,
    query: &str,
) -> Result<(), CliError> {
    let category = body.get("category").and_then(serde_json::Value::as_str);
    if matches!(category, Some("company" | "people")) {
        let unsupported_filters = [
            ("exclude-domain", "excludeDomains"),
            ("start-published-date", "startPublishedDate"),
            ("end-published-date", "endPublishedDate"),
        ]
        .into_iter()
        .filter_map(|(flag, key)| json_field_has_value(body, key).then_some(flag))
        .collect::<Vec<_>>();
        if !unsupported_filters.is_empty() {
            let category = category.expect("matches! above proves category");
            return Err(CliError::Usage(
                Diag::new(
                    "invalid_flag_combination",
                    format!(
                        "`category={category}` does not support {} filters",
                        unsupported_filters.join(", ")
                    ),
                )
                .with_details(serde_json::json!({
                    "category": category,
                    "unsupportedFilters": unsupported_filters,
                }))
                .with_suggestion(format!(
                    "exa-agent search {} --category {}",
                    shell_quote(query),
                    category
                )),
            ));
        }
    }

    if category == Some("people") {
        let include_domains = domains_from_body(body.get("includeDomains"));
        if let Some(invalid) = include_domains
            .iter()
            .find(|domain| !is_linkedin_domain(domain))
        {
            return Err(CliError::Usage(
                Diag::new(
                    "invalid_flag_combination",
                    "`category=people` only supports LinkedIn include-domain filters",
                )
                .with_details(serde_json::json!({
                    "invalidDomain": invalid,
                    "allowedDomains": ["linkedin.com", "*.linkedin.com"],
                }))
                .with_suggestion(format!(
                    "exa-agent search {} --category people --include-domain linkedin.com",
                    shell_quote(query)
                )),
            ));
        }
    }

    Ok(())
}

fn json_field_has_value(body: &serde_json::Value, key: &str) -> bool {
    match body.get(key) {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::String(value)) => !value.is_empty(),
        Some(serde_json::Value::Array(values)) => !values.is_empty(),
        Some(serde_json::Value::Object(values)) => !values.is_empty(),
        Some(_) => true,
    }
}

fn domains_from_body(value: Option<&serde_json::Value>) -> Vec<String> {
    match value {
        Some(serde_json::Value::String(domain)) if !domain.is_empty() => vec![domain.clone()],
        Some(serde_json::Value::Array(domains)) => domains
            .iter()
            .filter_map(serde_json::Value::as_str)
            .filter(|domain| !domain.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn is_linkedin_domain(raw: &str) -> bool {
    let raw = raw.trim().trim_end_matches('.').to_ascii_lowercase();
    let without_scheme = raw
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(raw.as_str());
    let authority = without_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(without_scheme)
        .rsplit('@')
        .next()
        .unwrap_or(without_scheme);
    let host = authority.split(':').next().unwrap_or(authority);
    let host = host.strip_prefix("www.").unwrap_or(host);
    host == "linkedin.com" || host.ends_with(".linkedin.com")
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
    let spec = build_typed_spec(op, &flag_values, globals)?;
    validate_contents_body_shape(&spec.body)?;
    Ok(spec)
}

fn validate_contents_body_shape(body: &serde_json::Value) -> Result<(), CliError> {
    if body.get("contents").is_some() {
        return Err(CliError::Usage(
            Diag::new(
                "invalid_flag_combination",
                "`contents.*` is only valid on `search`; the /contents endpoint uses top-level `--text` and `--summary-query`",
            )
            .with_suggestion("exa-agent contents <url-or-id> --text"),
        ));
    }
    Ok(())
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

fn dispatch_admin(sub: &AdminCmd, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    match sub {
        AdminCmd::Keys { sub } => dispatch_admin_keys(sub, globals, pretty),
    }
}

fn dispatch_admin_keys(
    sub: &AdminKeysCmd,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    match sub {
        AdminKeysCmd::Create(args) => dispatch_admin_keys_create(args, globals, pretty),
        AdminKeysCmd::List => dispatch_admin_keys_list(globals, pretty),
        AdminKeysCmd::Get { key_id } => dispatch_admin_keys_get(key_id, globals, pretty),
        AdminKeysCmd::Update {
            key_id,
            name,
            rate_limit,
            budget_cents,
            clear_budget_cents,
        } => dispatch_admin_keys_update(
            key_id,
            name.as_deref(),
            *rate_limit,
            *budget_cents,
            *clear_budget_cents,
            globals,
            pretty,
        ),
        AdminKeysCmd::Delete { key_id, confirm } => {
            dispatch_admin_keys_delete(key_id, confirm.as_deref(), globals, pretty)
        }
        AdminKeysCmd::Usage {
            key_id,
            start_date,
            end_date,
            group_by,
        } => dispatch_admin_keys_usage(
            key_id,
            start_date.as_deref(),
            end_date.as_deref(),
            *group_by,
            globals,
            pretty,
        ),
    }
}

fn dispatch_admin_keys_create(
    args: &AdminKeysCreateArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["admin", "keys", "create"])
        .expect("admin keys create is in registry");
    with_typed_error_context(op, globals, || {
        if args.secret_output.as_deref() == Some("-") {
            return Err(CliError::Usage(Diag::new(
                "invalid_value",
                "Refusing to write the created API key to stdout; use a file path for --secret-output",
            )));
        }
        let spec = build_admin_keys_create_spec(args, globals)?;
        if globals.print_request || globals.dry_run {
            return dispatch_typed_command(spec, globals, pretty);
        }
        if globals.raw {
            return Err(CliError::Usage(
                Diag::new(
                    "invalid_flag_combination",
                    "`--raw` cannot be combined with `admin keys create`; create responses include a one-time API key",
                )
                .with_suggestion("Use `--secret-output FILE` and the default JSON envelope."),
            ));
        }
        let Some(secret_path) = args.secret_output.as_deref() else {
            return Err(CliError::Usage(
                Diag::new(
                    "missing_required_argument",
                    "admin keys create returns the new key once and never prints it to stdout; pass --secret-output FILE to capture it",
                )
                .with_suggestion(
                    "exa-agent admin keys create --name ci --secret-output ./exa-key.secret",
                ),
            ));
        };
        let secret_output = reserve_webhook_secret_file(secret_path)?;
        dispatch_admin_keys_create_live(spec, secret_output, globals, pretty)
    })
}

fn dispatch_admin_keys_create_live(
    spec: request::RequestSpec,
    secret_output: SecretOutputReservation,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    parse_user_headers(&globals.headers)?;
    let credential = resolve_operation_credential(spec.op, globals)?;
    let cfg = config::Config::load()?;
    let timeout = transport::resolve_timeout(globals, &cfg)?;
    let transport = UreqTransport::new(timeout);
    let request_id = transport::new_request_id();
    let warnings = typed_command_warnings(spec.op);
    let result = match execute_raw_with_request_id(
        &transport,
        RawExecuteParams {
            method: spec.op.method.as_str(),
            path: spec.op.api_path,
            query_raw: &[],
            body: typed_wire_body(&spec),
            globals,
            credential: &credential,
            request_id: request_id.clone(),
        },
    ) {
        Ok(result) => result,
        Err(err) => {
            return Err(maybe_record_pending_run_on_create_failure(
                err,
                &spec,
                globals,
                &request_id,
                spec.op.api_path,
            ));
        }
    };
    let mut data = transport::parse_response_data(&result.response.body);
    let secret = data
        .get("apiKey")
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            CliError::Upstream(
                Diag::new(
                    "upstream_malformed",
                    "admin keys create response did not include string `apiKey`; reserved --secret-output file was not written",
                )
                .with_suggestion("exa-agent admin keys list"),
            )
        })?;
    secret_output.commit(secret)?;
    redaction::redact_json_value(&mut data);
    let count = transport::primary_count(&data);
    let hash = transport::data_hash(&data);
    let envelope = response_envelope(ResponseEnvelopeArgs {
        command: &spec.op.command(),
        method: &result.method,
        path: &result.path,
        operation: Some(spec.op),
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
        emit_stdout(&envelope, pretty);
    }
    Ok(0)
}

fn build_admin_keys_create_spec(
    args: &AdminKeysCreateArgs,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let op = registry::lookup_by_segments(&["admin", "keys", "create"])
        .expect("admin keys create is in registry");
    let body = admin_keys_body(args.name.as_deref(), args.rate_limit, args.budget_cents);
    let body = merge_manual_body_overrides(body, globals)?;
    validate_admin_keys_body(&body, "admin keys create")?;
    Ok(request::RequestSpec { op, body })
}

fn dispatch_admin_keys_list(globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["admin", "keys", "list"])
        .expect("admin keys list is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        dispatch_typed_command(spec, globals, pretty)
    })
}

fn dispatch_admin_keys_get(
    key_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["admin", "keys", "get"])
        .expect("admin keys get is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", key_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_admin_keys_update(
    key_id: &str,
    name: Option<&str>,
    rate_limit: Option<u32>,
    budget_cents: Option<u64>,
    clear_budget_cents: bool,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["admin", "keys", "update"])
        .expect("admin keys update is in registry");
    with_typed_error_context(op, globals, || {
        let mut body = admin_keys_body(name, rate_limit, budget_cents);
        if clear_budget_cents {
            body["budgetCents"] = serde_json::Value::Null;
        }
        let body = merge_manual_body_overrides(body, globals)?;
        validate_admin_keys_body(&body, "admin keys update")?;
        if body.as_object().is_some_and(serde_json::Map::is_empty) {
            return Err(CliError::Usage(
                Diag::new(
                    "missing_required_argument",
                    "admin keys update requires at least one field via named flags, --body, or --set",
                )
                .with_suggestion("exa-agent admin keys update <id> --name renamed"),
            ));
        }
        let spec = request::RequestSpec { op, body };
        let path = checked_substitute_path(op.api_path, &[("id", key_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_admin_keys_delete(
    key_id: &str,
    confirm: Option<&str>,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["admin", "keys", "delete"])
        .expect("admin keys delete is in registry");
    with_typed_error_context(op, globals, || {
        ensure_confirm_by_id(
            key_id,
            confirm,
            globals,
            format!("Refusing to delete API key `{key_id}` without `--confirm {key_id}`"),
            format!("exa-agent admin keys delete {key_id} --confirm {key_id}"),
        )?;
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", key_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_admin_keys_usage(
    key_id: &str,
    start_date: Option<&str>,
    end_date: Option<&str>,
    group_by: Option<GroupBy>,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["admin", "keys", "usage"])
        .expect("admin keys usage is in registry");
    with_typed_error_context(op, globals, || {
        validate_admin_usage_dates(start_date, end_date)?;
        let spec = build_typed_spec(op, &[], globals)?;
        let mut query = Vec::new();
        if let Some(start_date) = start_date {
            query.push(("start_date".to_string(), start_date.to_string()));
        }
        if let Some(end_date) = end_date {
            query.push(("end_date".to_string(), end_date.to_string()));
        }
        if let Some(group_by) = group_by {
            query.push(("group_by".to_string(), admin_group_by(group_by).to_string()));
        }
        let path = checked_substitute_path(op.api_path, &[("id", key_id)])?;
        dispatch_typed_command_routed(
            spec,
            globals,
            pretty,
            Some(path.as_str()),
            &query,
            false,
            None,
        )
    })
}

fn admin_keys_body(
    name: Option<&str>,
    rate_limit: Option<u32>,
    budget_cents: Option<u64>,
) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    if let Some(name) = name {
        body.insert(
            "name".to_string(),
            serde_json::Value::String(name.to_string()),
        );
    }
    if let Some(rate_limit) = rate_limit {
        body.insert("rateLimit".to_string(), serde_json::json!(rate_limit));
    }
    if let Some(budget_cents) = budget_cents {
        body.insert("budgetCents".to_string(), serde_json::json!(budget_cents));
    }
    serde_json::Value::Object(body)
}

fn merge_manual_body_overrides(
    mut body: serde_json::Value,
    globals: &GlobalArgs,
) -> Result<serde_json::Value, CliError> {
    if let Some(source) = globals
        .body
        .as_deref()
        .map(request::parse_body_source)
        .transpose()?
    {
        let overlay = request::read_body_source(source)?;
        if !overlay.is_object() {
            return Err(CliError::Usage(Diag::new(
                "invalid_value",
                "`--body` must be a JSON object when merging with named flags",
            )));
        }
        request::deep_merge(&mut body, overlay);
    }
    for entry in &globals.set {
        let (path, value) = request::parse_set(entry)?;
        request::set_at_path(&mut body, &path, value)?;
    }
    Ok(body)
}

fn validate_admin_keys_body(body: &serde_json::Value, command: &str) -> Result<(), CliError> {
    let Some(obj) = body.as_object() else {
        return Err(CliError::Usage(Diag::new(
            "invalid_value",
            format!("{command} request body must be a JSON object"),
        )));
    };
    if let Some(value) = obj.get("rateLimit") {
        if value.as_u64().is_none_or(|rate| rate > u32::MAX as u64) {
            return Err(CliError::Usage(Diag::new(
                "invalid_value",
                format!("{command} rateLimit must be a non-negative integer"),
            )));
        }
    }
    if let Some(value) = obj.get("budgetCents") {
        if !(value.is_null() || value.as_u64().is_some()) {
            return Err(CliError::Usage(Diag::new(
                "invalid_value",
                format!("{command} budgetCents must be a non-negative integer or null"),
            )));
        }
    }
    Ok(())
}

fn ensure_confirm_by_id(
    id: &str,
    confirm: Option<&str>,
    globals: &GlobalArgs,
    message: impl Into<String>,
    suggestion: impl Into<String>,
) -> Result<(), CliError> {
    if globals.dry_run || globals.print_request {
        return Ok(());
    }
    match confirm {
        Some(confirm) if confirm == id => Ok(()),
        _ => Err(CliError::Safety(
            Diag::new("confirmation_required", message.into()).with_suggestion(suggestion.into()),
        )),
    }
}

fn admin_group_by(group_by: GroupBy) -> &'static str {
    match group_by {
        GroupBy::Hour => "hour",
        GroupBy::Day => "day",
        GroupBy::Month => "month",
    }
}

fn validate_admin_usage_dates(
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> Result<(), CliError> {
    let start = start_date.map(parse_admin_usage_date).transpose()?;
    let end = end_date.map(parse_admin_usage_date).transpose()?;
    let now = now_utc_for_admin_validation();
    let oldest = utc_midnight(now - TimeDuration::days(180));
    if let (Some(start), Some(end)) = (start, end) {
        if start > end {
            return Err(CliError::Usage(
                Diag::new(
                    "invalid_value",
                    "admin keys usage --start-date must be before or equal to --end-date",
                )
                .with_suggestion(
                    "exa-agent admin keys usage <id> --start-date 2026-01-01 --end-date 2026-01-31",
                ),
            ));
        }
        if end - start > TimeDuration::days(180) {
            return Err(admin_usage_window_error(
                "admin keys usage date range must be 180 days or less",
            ));
        }
    }
    for (label, date) in [("--start-date", start), ("--end-date", end)] {
        if let Some(date) = date {
            if date < oldest {
                return Err(admin_usage_window_error(format!(
                    "admin keys usage {label} must be within the last 180 days"
                )));
            }
            if date > now {
                return Err(admin_usage_window_error(format!(
                    "admin keys usage {label} cannot be in the future"
                )));
            }
        }
    }
    Ok(())
}

fn admin_usage_window_error(message: impl Into<String>) -> CliError {
    CliError::Usage(Diag::new("invalid_value", message.into()).with_suggestion(
        "exa-agent admin keys usage <id> --start-date 2026-01-01 --end-date 2026-01-31",
    ))
}

fn parse_admin_usage_date(raw: &str) -> Result<OffsetDateTime, CliError> {
    if raw.contains('T') {
        return OffsetDateTime::parse(raw, &time::format_description::well_known::Rfc3339).map_err(
            |_| {
                CliError::Usage(
                    Diag::new(
                        "invalid_value",
                        "admin keys usage dates must be ISO dates or RFC3339 date-times",
                    )
                    .with_suggestion("Use YYYY-MM-DD or 2026-01-01T00:00:00Z"),
                )
            },
        );
    }
    let mut parts = raw.split('-');
    let year = parts.next().and_then(|part| part.parse::<i32>().ok());
    let month = parts.next().and_then(|part| part.parse::<u8>().ok());
    let day = parts.next().and_then(|part| part.parse::<u8>().ok());
    if parts.next().is_some() {
        return Err(admin_usage_date_error());
    }
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return Err(admin_usage_date_error());
    };
    let month = time::Month::try_from(month).map_err(|_| admin_usage_date_error())?;
    let date = Date::from_calendar_date(year, month, day).map_err(|_| admin_usage_date_error())?;
    Ok(PrimitiveDateTime::new(date, time::Time::MIDNIGHT).assume_utc())
}

fn admin_usage_date_error() -> CliError {
    CliError::Usage(
        Diag::new(
            "invalid_value",
            "admin keys usage dates must be ISO dates or RFC3339 date-times",
        )
        .with_suggestion("Use YYYY-MM-DD or 2026-01-01T00:00:00Z"),
    )
}

fn now_utc_for_admin_validation() -> OffsetDateTime {
    std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|raw| raw.parse::<i64>().ok())
        .and_then(|seconds| OffsetDateTime::from_unix_timestamp(seconds).ok())
        .unwrap_or_else(OffsetDateTime::now_utc)
}

fn utc_midnight(datetime: OffsetDateTime) -> OffsetDateTime {
    PrimitiveDateTime::new(datetime.date(), time::Time::MIDNIGHT).assume_utc()
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
            dispatch_paginated_typed_command(spec, globals, pretty, pagination, None, &[])
        } else {
            dispatch_typed_command_routed(spec, globals, pretty, None, &query, false, None)
        }
    })
}

fn dispatch_agent_runs_get(id: &str, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["agent", "runs", "get"]).expect("agent runs get");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", id)])?;
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
        let path = checked_substitute_path(op.api_path, &[("id", &args.id)])?;
        let query = pagination_query(&args.pagination);
        let extra_headers = agent_runs_events_headers(args);
        if args.pagination.all && !args.stream && !(globals.print_request || globals.dry_run) {
            dispatch_paginated_typed_command(
                spec,
                globals,
                pretty,
                &args.pagination,
                Some(path.as_str()),
                &[],
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
        let path = checked_substitute_path(op.api_path, &[("id", id)])?;
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
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
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
            dispatch_paginated_typed_command(spec, globals, pretty, pagination, None, &[])
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
        let path = checked_substitute_path(op.api_path, &[("researchId", research_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_monitor(sub: &MonitorCmd, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    match sub {
        MonitorCmd::Create(args) => dispatch_monitor_create(args, globals, pretty),
        MonitorCmd::List(args) => dispatch_monitor_list(args, globals, pretty),
        MonitorCmd::Get { id } => dispatch_monitor_get(id, globals, pretty),
        MonitorCmd::Update {
            id,
            name,
            query,
            schedule,
            status,
            webhook_url,
        } => dispatch_monitor_update(
            id,
            MonitorUpdateFields {
                name: name.as_deref(),
                query: query.as_deref(),
                schedule: schedule.as_deref(),
                status: status.as_deref(),
                webhook_url: webhook_url.as_deref(),
            },
            globals,
            pretty,
        ),
        MonitorCmd::Delete { id } => dispatch_monitor_delete(id, globals, pretty),
        MonitorCmd::Trigger { id } => dispatch_monitor_trigger(id, globals, pretty),
        MonitorCmd::Batch(args) => dispatch_monitor_batch(args, globals, pretty),
        MonitorCmd::Runs { sub } => match sub {
            MonitorRunsCmd::List {
                monitor_id,
                pagination,
            } => dispatch_monitor_runs_list(monitor_id, pagination, globals, pretty),
            MonitorRunsCmd::Get { monitor_id, run_id } => {
                dispatch_monitor_runs_get(monitor_id, run_id, globals, pretty)
            }
        },
    }
}

fn dispatch_monitor_create(
    args: &MonitorCreateArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["monitor", "create"])
        .expect("monitor create is in registry");
    with_typed_error_context(op, globals, || {
        if args.secret_output.as_deref() == Some("-") {
            return Err(CliError::Usage(Diag::new(
                "invalid_value",
                "Refusing to write webhook secret to stdout; use a file path for --secret-output",
            )));
        }
        let spec = build_monitor_create_spec(args, globals)?;
        let extra_warnings = monitor_webhook_secret_warnings(args, &spec.body);
        if globals.print_request || globals.dry_run {
            return dispatch_typed_preview_with_warnings(
                spec,
                globals,
                pretty,
                TypedDispatchOptions::default(),
                &extra_warnings,
            );
        }
        if globals.raw {
            return Err(CliError::Usage(
                Diag::new(
                    "invalid_flag_combination",
                    "`--raw` cannot be combined with `monitor create`; create responses can include one-time webhook secrets",
                )
                .with_suggestion("Use `--secret-output FILE` and the default JSON envelope."),
            ));
        }
        let secret_output = args
            .secret_output
            .as_deref()
            .map(reserve_webhook_secret_file)
            .transpose()?;
        dispatch_monitor_create_live(spec, secret_output, globals, pretty, extra_warnings)
    })
}

fn build_monitor_create_spec(
    args: &MonitorCreateArgs,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let op = registry::lookup_by_segments(&["monitor", "create"])
        .expect("monitor create is in registry");
    let mut body = build_monitor_create_named_body(args);
    body = apply_request_overrides(body, globals)?;
    if !monitor_create_has_required_fields(&body) {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "monitor create requires search.query and webhook.url (via --query/--webhook-url, --body, or --set)",
            )
            .with_suggestion(
                "exa-agent monitor create --query \"AI news\" --webhook-url https://example.com/hook",
            ),
        ));
    }
    Ok(request::RequestSpec { op, body })
}

fn build_monitor_create_named_body(args: &MonitorCreateArgs) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    if let Some(name) = &args.name {
        body.insert("name".to_string(), serde_json::Value::String(name.clone()));
    }
    if let Some(query) = &args.query {
        body.insert("search".to_string(), serde_json::json!({ "query": query }));
    }
    if let Some(schedule) = &args.schedule {
        body.insert(
            "trigger".to_string(),
            serde_json::json!({ "type": "interval", "period": schedule }),
        );
    }
    if let Some(url) = &args.webhook_url {
        body.insert("webhook".to_string(), serde_json::json!({ "url": url }));
    }
    serde_json::Value::Object(body)
}

fn monitor_create_has_required_fields(body: &serde_json::Value) -> bool {
    let has_search_query = body
        .get("search")
        .and_then(|search| search.get("query"))
        .and_then(|query| query.as_str())
        .is_some_and(|query| !query.is_empty());
    let has_webhook_url = body
        .get("webhook")
        .and_then(|webhook| webhook.get("url"))
        .and_then(|url| url.as_str())
        .is_some_and(|url| !url.is_empty());
    has_search_query && has_webhook_url
}

fn monitor_webhook_secret_warnings(
    args: &MonitorCreateArgs,
    body: &serde_json::Value,
) -> Vec<serde_json::Value> {
    let has_webhook_url = args.webhook_url.is_some()
        || body
            .get("webhook")
            .and_then(|webhook| webhook.get("url"))
            .and_then(|url| url.as_str())
            .is_some_and(|url| !url.is_empty());
    if has_webhook_url && args.secret_output.is_none() {
        vec![serde_json::json!({
            "code": "webhook_secret_ephemeral",
            "message": "Create responses include webhookSecret once; use --secret-output FILE to capture it or it will be lost.",
            "replacement": "exa-agent monitor create --query \"...\" --webhook-url https://example.com/hook --secret-output ./webhook.secret"
        })]
    } else {
        Vec::new()
    }
}

fn apply_request_overrides(
    mut body: serde_json::Value,
    globals: &GlobalArgs,
) -> Result<serde_json::Value, CliError> {
    if let Some(raw) = globals.body.as_deref() {
        let source = request::parse_body_source(raw)?;
        let overlay = request::read_body_source(source)?;
        if !overlay.is_object() {
            return Err(CliError::Usage(Diag::new(
                "invalid_value",
                "`--body` must be a JSON object when merging with named flags",
            )));
        }
        request::deep_merge(&mut body, overlay);
    }
    for entry in &globals.set {
        let (path, value) = request::parse_set(entry)?;
        request::set_at_path(&mut body, &path, value)?;
    }
    Ok(body)
}

fn dispatch_monitor_create_live(
    spec: request::RequestSpec,
    secret_output: Option<SecretOutputReservation>,
    globals: &GlobalArgs,
    pretty: bool,
    extra_warnings: Vec<serde_json::Value>,
) -> Result<i32, CliError> {
    parse_user_headers(&globals.headers)?;
    let api_input = credential_input(auth::CredentialNamespace::Api, globals)?;
    let credential = auth::resolve_api_credential(&api_input, &auth::NoopKeyring)
        .map_err(|missing| auth::not_authenticated_error(&missing))?;
    let cfg = config::Config::load()?;
    let timeout = transport::resolve_timeout(globals, &cfg)?;
    let transport = UreqTransport::new(timeout);
    let request_id = transport::new_request_id();
    let mut warnings = typed_command_warnings(spec.op);
    warnings.extend(extra_warnings);
    let result = match execute_raw_with_request_id(
        &transport,
        RawExecuteParams {
            method: spec.op.method.as_str(),
            path: spec.op.api_path,
            query_raw: &[],
            body: typed_wire_body(&spec),
            globals,
            credential: &credential,
            request_id: request_id.clone(),
        },
    ) {
        Ok(result) => result,
        Err(err) => {
            return Err(maybe_record_pending_run_on_create_failure(
                err,
                &spec,
                globals,
                &request_id,
                spec.op.api_path,
            ));
        }
    };
    let mut data = transport::parse_response_data(&result.response.body);
    if let Some(output) = secret_output {
        let secret = data
            .get("webhookSecret")
            .and_then(|value| value.as_str())
            .ok_or_else(|| {
                CliError::Upstream(
                    Diag::new(
                        "upstream_malformed",
                        "monitor create response did not include string `webhookSecret`; reserved --secret-output file was not written",
                    )
                    .with_suggestion("exa-agent monitor list --limit 10"),
                )
            })?;
        output.commit(secret)?;
    }
    redaction::redact_json_value(&mut data);
    let count = transport::primary_count(&data);
    let hash = transport::data_hash(&data);
    let envelope = response_envelope(ResponseEnvelopeArgs {
        command: &spec.op.command(),
        method: &result.method,
        path: &result.path,
        operation: Some(spec.op),
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
        emit_stdout(&envelope, pretty);
    }
    Ok(0)
}

struct SecretOutputReservation {
    target: std::path::PathBuf,
    file: Option<std::fs::File>,
    committed: bool,
}

impl SecretOutputReservation {
    fn commit(mut self, secret: &str) -> Result<(), CliError> {
        use std::io::Write;
        let path = self.target.display().to_string();
        let Some(mut file) = self.file.take() else {
            return Err(CliError::Usage(Diag::new(
                "invalid_value",
                format!("failed to write --secret-output file `{path}`"),
            )));
        };
        file.write_all(secret.as_bytes()).map_err(|err| {
            CliError::Usage(Diag::new(
                "invalid_value",
                format!("failed to write --secret-output file `{path}`: {err}"),
            ))
        })?;
        // After the full secret is written, never remove the final output path on a later
        // best-effort finalization error. `webhookSecret` is returned once; preserving the
        // captured file is safer than returning an error and deleting the only copy.
        self.committed = true;
        file.sync_all().map_err(|err| {
            CliError::Usage(Diag::new(
                "invalid_value",
                format!("failed to flush --secret-output file `{path}`: {err}"),
            ))
        })?;
        drop(file);
        Ok(())
    }
}

impl Drop for SecretOutputReservation {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.target);
        }
    }
}

fn reserve_webhook_secret_file(path: &str) -> Result<SecretOutputReservation, CliError> {
    let target = std::path::PathBuf::from(path);
    if target.is_dir() {
        return Err(CliError::Usage(Diag::new(
            "invalid_value",
            format!("--secret-output `{path}` must be a file path, not a directory"),
        )));
    }
    if target
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .is_none()
    {
        return Err(CliError::Usage(Diag::new(
            "invalid_value",
            format!("--secret-output `{path}` must include a file name"),
        )));
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(&target).map_err(|err| {
        let message = if err.kind() == std::io::ErrorKind::AlreadyExists {
            format!("--secret-output file `{path}` already exists; choose a new path")
        } else {
            format!("failed to reserve --secret-output file `{path}` before request: {err}")
        };
        CliError::Usage(Diag::new("invalid_value", message))
    })?;
    Ok(SecretOutputReservation {
        target,
        file: Some(file),
        committed: false,
    })
}

fn dispatch_monitor_list(
    args: &MonitorListArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op =
        registry::lookup_by_segments(&["monitor", "list"]).expect("monitor list is in registry");
    with_typed_error_context(op, globals, || {
        validate_cursor_pagination(&args.pagination)?;
        let spec = build_typed_spec(op, &[], globals)?;
        let static_query = monitor_list_static_query(args)?;
        let query = merge_static_and_pagination_query(&static_query, &args.pagination);
        if args.pagination.all && !(globals.print_request || globals.dry_run) {
            dispatch_paginated_typed_command(
                spec,
                globals,
                pretty,
                &args.pagination,
                None,
                &static_query,
            )
        } else {
            dispatch_typed_command_routed(spec, globals, pretty, None, &query, false, None)
        }
    })
}

fn monitor_list_static_query(args: &MonitorListArgs) -> Result<Vec<(String, String)>, CliError> {
    let mut query = Vec::new();
    if let Some(status) = &args.status {
        query.push(("status".to_string(), status.clone()));
    }
    if let Some(name) = &args.name {
        query.push(("name".to_string(), name.clone()));
    }
    for entry in &args.metadata {
        let (key, value) = parse_metadata_kv(entry)?;
        query.push((format!("metadata[{key}]"), value));
    }
    Ok(query)
}

fn merge_static_and_pagination_query(
    static_query: &[(String, String)],
    pagination: &PaginationArgs,
) -> Vec<(String, String)> {
    let mut query = static_query.to_vec();
    query.extend(pagination_query(pagination));
    query
}

fn parse_metadata_kv(raw: &str) -> Result<(String, String), CliError> {
    let (key, value) = raw.split_once('=').ok_or_else(|| {
        CliError::Usage(
            Diag::new("invalid_value", "`--metadata` expects `key=value`")
                .with_suggestion("exa-agent monitor list --metadata slack_channel_id=C123"),
        )
    })?;
    if key.is_empty() {
        return Err(CliError::Usage(
            Diag::new("invalid_value", "`--metadata` key must not be empty")
                .with_suggestion("exa-agent monitor list --metadata owner=ops"),
        ));
    }
    Ok((key.to_string(), value.to_string()))
}

fn dispatch_monitor_get(id: &str, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["monitor", "get"]).expect("monitor get is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

#[derive(Clone, Copy)]
struct MonitorUpdateFields<'a> {
    name: Option<&'a str>,
    query: Option<&'a str>,
    schedule: Option<&'a str>,
    status: Option<&'a str>,
    webhook_url: Option<&'a str>,
}

fn dispatch_monitor_update(
    id: &str,
    fields: MonitorUpdateFields<'_>,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["monitor", "update"])
        .expect("monitor update is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_monitor_update_spec(op, fields, globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn build_monitor_update_spec(
    op: &'static registry::OperationDef,
    fields: MonitorUpdateFields<'_>,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let mut body = serde_json::Map::new();
    if let Some(name) = fields.name {
        body.insert(
            "name".to_string(),
            serde_json::Value::String(name.to_string()),
        );
    }
    if let Some(query) = fields.query {
        body.insert("search".to_string(), serde_json::json!({ "query": query }));
    }
    if let Some(schedule) = fields.schedule {
        body.insert(
            "trigger".to_string(),
            serde_json::json!({ "type": "interval", "period": schedule }),
        );
    }
    if let Some(status) = fields.status {
        body.insert(
            "status".to_string(),
            serde_json::Value::String(status.to_string()),
        );
    }
    if let Some(url) = fields.webhook_url {
        body.insert("webhook".to_string(), serde_json::json!({ "url": url }));
    }
    let body = apply_request_overrides(serde_json::Value::Object(body), globals)?;
    if body.as_object().is_some_and(|object| object.is_empty()) {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "monitor update requires at least one field via named flags, --body, or --set",
            )
            .with_suggestion("exa-agent monitor update <id> --status paused"),
        ));
    }
    Ok(request::RequestSpec { op, body })
}

fn dispatch_monitor_delete(id: &str, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["monitor", "delete"])
        .expect("monitor delete is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_monitor_trigger(id: &str, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["monitor", "trigger"])
        .expect("monitor trigger is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_monitor_batch(
    args: &MonitorBatchArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op =
        registry::lookup_by_segments(&["monitor", "batch"]).expect("monitor batch is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_monitor_batch_spec(op, globals)?;
        validate_monitor_batch_live(&spec.body, globals, args.confirm.as_deref())?;
        dispatch_typed_command(spec, globals, pretty)
    })
}

fn build_monitor_batch_spec(
    op: &'static registry::OperationDef,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let mut spec = build_typed_spec(op, &[], globals)?;
    inject_batch_dry_run_default(&mut spec.body);
    validate_monitor_batch_shape(&spec.body)?;
    Ok(spec)
}

fn inject_batch_dry_run_default(body: &mut serde_json::Value) {
    if body.get("dry_run").is_none() && body.get("dryRun").is_none() {
        if let Some(obj) = body.as_object_mut() {
            obj.insert("dry_run".to_string(), serde_json::Value::Bool(true));
        }
    }
}

fn validate_monitor_batch_shape(body: &serde_json::Value) -> Result<(), CliError> {
    let action = body.get("action").and_then(|value| value.as_str()).ok_or_else(|| {
        CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "monitor batch requires `action` (delete, pause, or unpause) in the request body",
            )
            .with_suggestion(
                "exa-agent monitor batch --body '{\"action\":\"pause\",\"filter\":{\"status\":\"active\"}}'",
            ),
        )
    })?;
    if !matches!(action, "delete" | "pause" | "unpause") {
        return Err(CliError::Usage(Diag::new(
            "invalid_value",
            format!("monitor batch action must be delete, pause, or unpause (got `{action}`)"),
        )));
    }
    let filter = body.get("filter").and_then(|value| value.as_object()).ok_or_else(|| {
        CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "monitor batch requires a non-empty `filter` object",
            )
            .with_suggestion(
                "exa-agent monitor batch --body '{\"action\":\"pause\",\"filter\":{\"name\":\"daily\"}}'",
            ),
        )
    })?;
    if filter.is_empty() {
        return Err(CliError::Usage(Diag::new(
            "missing_required_argument",
            "monitor batch filter must include at least one field",
        )));
    }
    monitor_batch_dry_run(body)?;
    Ok(())
}

fn monitor_batch_dry_run(body: &serde_json::Value) -> Result<Option<bool>, CliError> {
    let Some(value) = body.get("dry_run").or_else(|| body.get("dryRun")) else {
        return Ok(None);
    };
    value.as_bool().map(Some).ok_or_else(|| {
        CliError::Usage(Diag::new(
            "invalid_value",
            "monitor batch `dry_run` must be a boolean",
        ))
    })
}

fn validate_monitor_batch_live(
    body: &serde_json::Value,
    globals: &GlobalArgs,
    confirm: Option<&str>,
) -> Result<(), CliError> {
    if globals.dry_run || globals.print_request {
        return Ok(());
    }
    let dry_run = monitor_batch_dry_run(body)?;
    if dry_run != Some(false) {
        return Ok(());
    }
    if !globals.yes {
        return Err(CliError::Safety(
            Diag::new(
                "confirmation_required",
                "Refusing live monitor batch with dry_run:false without --yes; preview first with --dry-run",
            )
            .with_suggestion(
                "exa-agent monitor batch --body '{\"action\":\"pause\",\"filter\":{\"status\":\"active\"},\"dry_run\":false}' --yes",
            ),
        ));
    }
    if body.get("action").and_then(|value| value.as_str()) == Some("delete")
        && confirm != Some("delete")
    {
        return Err(CliError::Safety(
            Diag::new(
                "confirmation_required",
                "Refusing live monitor batch delete without --confirm delete",
            )
            .with_suggestion(
                "exa-agent monitor batch --confirm delete --body '{\"action\":\"delete\",\"filter\":{\"name\":\"daily\"},\"dry_run\":false}' --yes",
            ),
        ));
    }
    Ok(())
}

fn dispatch_monitor_runs_list(
    monitor_id: &str,
    pagination: &PaginationArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["monitor", "runs", "list"]).expect("monitor runs list");
    with_typed_error_context(op, globals, || {
        validate_cursor_pagination(pagination)?;
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", monitor_id)])?;
        let query = pagination_query(pagination);
        if pagination.all && !(globals.print_request || globals.dry_run) {
            dispatch_paginated_typed_command(
                spec,
                globals,
                pretty,
                pagination,
                Some(path.as_str()),
                &[],
            )
        } else {
            dispatch_typed_command_routed(
                spec,
                globals,
                pretty,
                Some(path.as_str()),
                &query,
                false,
                None,
            )
        }
    })
}

fn dispatch_monitor_runs_get(
    monitor_id: &str,
    run_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["monitor", "runs", "get"]).expect("monitor runs get");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", monitor_id), ("runId", run_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets(sub: &WebsetsCmd, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    match sub {
        WebsetsCmd::Create(args) => dispatch_websets_create(args, globals, pretty),
        WebsetsCmd::List(args) => dispatch_websets_list(args, globals, pretty),
        WebsetsCmd::Get { id } => dispatch_websets_get(id, globals, pretty),
        WebsetsCmd::Update { id } => dispatch_websets_update(id, globals, pretty),
        WebsetsCmd::Delete { id } => dispatch_websets_delete(id, globals, pretty),
        WebsetsCmd::Cancel { id } => dispatch_websets_cancel(id, globals, pretty),
        WebsetsCmd::Preview(args) => dispatch_websets_preview(args, globals, pretty),
        WebsetsCmd::Items { sub } => dispatch_websets_items(sub, globals, pretty),
        WebsetsCmd::Searches { sub } => dispatch_websets_searches(sub, globals, pretty),
        WebsetsCmd::Enrichments { sub } => dispatch_websets_enrichments(sub, globals, pretty),
        WebsetsCmd::Imports { sub } => dispatch_websets_imports(sub, globals, pretty),
        WebsetsCmd::Monitors { sub } => dispatch_websets_monitors(sub, globals, pretty),
        WebsetsCmd::Events { sub } => dispatch_websets_events(sub, globals, pretty),
        WebsetsCmd::Webhooks { sub } => dispatch_websets_webhooks(sub, globals, pretty),
    }
}

fn insert_search_field(
    body: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: serde_json::Value,
) {
    let search = body
        .entry("search".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if let Some(obj) = search.as_object_mut() {
        obj.insert(key.to_string(), value);
    }
}

fn websets_body_is_non_empty_object(body: &serde_json::Value) -> bool {
    body.as_object().is_some_and(|object| !object.is_empty())
}

fn websets_search_has_query(body: &serde_json::Value) -> bool {
    let search = body.get("search").and_then(|value| value.as_object());
    search
        .and_then(|obj| obj.get("query"))
        .and_then(|query| query.as_str())
        .is_some_and(|query| !query.is_empty())
}

fn build_websets_create_named_body(args: &WebsetsCreateArgs) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    if let Some(query) = &args.query {
        insert_search_field(&mut body, "query", serde_json::Value::String(query.clone()));
    }
    if let Some(count) = args.count {
        insert_search_field(&mut body, "count", serde_json::json!(count));
    }
    serde_json::Value::Object(body)
}

fn build_websets_create_spec(
    args: &WebsetsCreateArgs,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let op = registry::lookup_by_segments(&["websets", "create"])
        .expect("websets create is in registry");
    validate_websets_create_intent_args(args)?;
    let mut body = build_websets_create_named_body(args);
    body = apply_request_overrides(body, globals)?;
    if !websets_body_is_non_empty_object(&body) {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "websets create requires at least one field (via --query, --body, or --set)",
            )
            .with_suggestion("exa-agent websets create --query \"SF startups\""),
        ));
    }
    if body.get("search").is_some() && !websets_search_has_query(&body) {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "websets create search requires search.query (via --query, --body, or --set)",
            )
            .with_suggestion("exa-agent websets create --query \"SF startups\""),
        ));
    }
    Ok(request::RequestSpec { op, body })
}

fn validate_websets_create_intent_args(args: &WebsetsCreateArgs) -> Result<(), CliError> {
    if let Some(num_results) = args.num_results.as_deref() {
        return Err(CliError::Usage(
            Diag::new(
                "invalid_flag_combination",
                "websets create uses `--count`, not search-style `--num-results`",
            )
            .with_suggestion(format!(
                "exa-agent websets create --query {} --count {}",
                shell_quote(args.query.as_deref().unwrap_or("<query>")),
                replacement_positive_int(num_results, None)
            )),
        ));
    }
    Ok(())
}

fn dispatch_websets_create(
    args: &WebsetsCreateArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "create"])
        .expect("websets create is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_websets_create_spec(args, globals)?;
        dispatch_typed_command(spec, globals, pretty)
    })
}

fn build_websets_preview_named_body(args: &WebsetsPreviewArgs) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    if let Some(query) = &args.query {
        insert_search_field(&mut body, "query", serde_json::Value::String(query.clone()));
    }
    if let Some(count) = args.count {
        insert_search_field(&mut body, "count", serde_json::json!(count));
    }
    serde_json::Value::Object(body)
}

fn build_websets_preview_spec(
    args: &WebsetsPreviewArgs,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let op = registry::lookup_by_segments(&["websets", "preview"])
        .expect("websets preview is in registry");
    if !args.criteria.is_empty() {
        return Err(CliError::Usage(
            Diag::new(
                "invalid_value",
                "`websets preview --criteria` is not supported by the current upstream preview schema",
            )
            .with_suggestion(
                "Use `websets searches create ... --criteria` or pass a current schema-compatible body with --body/--set",
            ),
        ));
    }
    let body = build_websets_preview_named_body(args);
    let body = apply_request_overrides(body, globals)?;
    if !websets_search_has_query(&body) {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "websets preview requires search.query (via --query, --body, or --set)",
            )
            .with_suggestion("exa-agent websets preview --query \"AI tools\" --count 3"),
        ));
    }
    Ok(request::RequestSpec { op, body })
}

fn websets_preview_query(body: &serde_json::Value) -> Vec<(String, String)> {
    if body
        .pointer("/search/count")
        .is_some_and(serde_json::Value::is_number)
    {
        vec![("search".to_string(), "true".to_string())]
    } else {
        Vec::new()
    }
}

fn dispatch_websets_preview(
    args: &WebsetsPreviewArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "preview"])
        .expect("websets preview is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_websets_preview_spec(args, globals)?;
        let query = websets_preview_query(&spec.body);
        dispatch_typed_command_routed(spec, globals, pretty, None, &query, false, None)
    })
}

fn websets_list_static_query(args: &WebsetsListArgs) -> Vec<(String, String)> {
    let mut query = Vec::new();
    if let Some(search) = &args.search {
        query.push(("search".to_string(), search.clone()));
    }
    query
}

fn dispatch_websets_list(
    args: &WebsetsListArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op =
        registry::lookup_by_segments(&["websets", "list"]).expect("websets list is in registry");
    with_typed_error_context(op, globals, || {
        validate_cursor_pagination(&args.pagination)?;
        let spec = build_typed_spec(op, &[], globals)?;
        let static_query = websets_list_static_query(args);
        let query = merge_static_and_pagination_query(&static_query, &args.pagination);
        if args.pagination.all && !(globals.print_request || globals.dry_run) {
            dispatch_paginated_typed_command(
                spec,
                globals,
                pretty,
                &args.pagination,
                None,
                &static_query,
            )
        } else {
            dispatch_typed_command_routed(spec, globals, pretty, None, &query, false, None)
        }
    })
}

fn dispatch_websets_get(id: &str, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "get"]).expect("websets get is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn build_websets_update_spec(
    op: &'static registry::OperationDef,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let body = apply_request_overrides(serde_json::json!({}), globals)?;
    if body.as_object().is_some_and(|object| object.is_empty()) {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "websets update requires a request body via --body or --set",
            )
            .with_suggestion("exa-agent websets update <id> --set title=\"My Webset\""),
        ));
    }
    Ok(request::RequestSpec { op, body })
}

fn dispatch_websets_update(id: &str, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "update"])
        .expect("websets update is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_websets_update_spec(op, globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_delete(id: &str, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "delete"])
        .expect("websets delete is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_cancel(id: &str, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "cancel"])
        .expect("websets cancel is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_items(
    sub: &cli::WebsetsItemsCmd,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    use cli::WebsetsItemsCmd;
    match sub {
        WebsetsItemsCmd::List {
            webset_id,
            pagination,
            source_id,
        } => dispatch_websets_items_list(
            webset_id,
            source_id.as_deref(),
            pagination,
            globals,
            pretty,
        ),
        WebsetsItemsCmd::Get { webset_id, item_id } => {
            dispatch_websets_items_get(webset_id, item_id, globals, pretty)
        }
        WebsetsItemsCmd::Delete { webset_id, item_id } => {
            dispatch_websets_items_delete(webset_id, item_id, globals, pretty)
        }
    }
}

fn websets_items_list_static_query(source_id: Option<&str>) -> Vec<(String, String)> {
    let mut query = Vec::new();
    if let Some(source_id) = source_id {
        query.push(("sourceId".to_string(), source_id.to_string()));
    }
    query
}

fn dispatch_websets_items_list(
    webset_id: &str,
    source_id: Option<&str>,
    pagination: &PaginationArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "items", "list"])
        .expect("websets items list is in registry");
    with_typed_error_context(op, globals, || {
        validate_cursor_pagination(pagination)?;
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("webset", webset_id)])?;
        let static_query = websets_items_list_static_query(source_id);
        let query = merge_static_and_pagination_query(&static_query, pagination);
        if pagination.all && !(globals.print_request || globals.dry_run) {
            dispatch_paginated_typed_command(
                spec,
                globals,
                pretty,
                pagination,
                Some(path.as_str()),
                &static_query,
            )
        } else {
            dispatch_typed_command_routed(
                spec,
                globals,
                pretty,
                Some(path.as_str()),
                &query,
                false,
                None,
            )
        }
    })
}

fn dispatch_websets_items_get(
    webset_id: &str,
    item_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "items", "get"])
        .expect("websets items get is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("webset", webset_id), ("id", item_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_items_delete(
    webset_id: &str,
    item_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "items", "delete"])
        .expect("websets items delete is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("webset", webset_id), ("id", item_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_searches(
    sub: &cli::WebsetsSearchesCmd,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    use cli::WebsetsSearchesCmd;
    match sub {
        WebsetsSearchesCmd::Create {
            webset_id,
            query,
            count,
            criteria,
        } => dispatch_websets_searches_create(
            webset_id,
            query.as_deref(),
            *count,
            criteria,
            globals,
            pretty,
        ),
        WebsetsSearchesCmd::Get {
            webset_id,
            search_id,
        } => dispatch_websets_searches_get(webset_id, search_id, globals, pretty),
        WebsetsSearchesCmd::Cancel {
            webset_id,
            search_id,
        } => dispatch_websets_searches_cancel(webset_id, search_id, globals, pretty),
    }
}

fn websets_searches_create_has_query_and_count(body: &serde_json::Value) -> bool {
    let has_query = body
        .get("query")
        .and_then(|query| query.as_str())
        .is_some_and(|query| !query.is_empty());
    let has_count = body
        .get("count")
        .and_then(serde_json::Value::as_u64)
        .is_some();
    has_query && has_count
}

fn build_websets_searches_create_spec(
    webset_id: &str,
    query: Option<&str>,
    count: Option<u32>,
    criteria: &[String],
    globals: &GlobalArgs,
) -> Result<(request::RequestSpec, String), CliError> {
    let op = registry::lookup_by_segments(&["websets", "searches", "create"])
        .expect("websets searches create is in registry");
    let mut body = serde_json::Map::new();
    if let Some(query) = query {
        body.insert(
            "query".to_string(),
            serde_json::Value::String(query.to_string()),
        );
    }
    if let Some(count) = count {
        body.insert("count".to_string(), serde_json::json!(count));
    }
    if !criteria.is_empty() {
        let criteria: Vec<serde_json::Value> = criteria
            .iter()
            .map(|description| serde_json::json!({ "description": description }))
            .collect();
        body.insert("criteria".to_string(), serde_json::Value::Array(criteria));
    }
    let body = apply_request_overrides(serde_json::Value::Object(body), globals)?;
    if !websets_searches_create_has_query_and_count(&body) {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "websets searches create requires query and count (via --query/--count, --body, or --set)",
            )
            .with_suggestion(
                "exa-agent websets searches create <webset> --query \"founders\" --count 25",
            ),
        ));
    }
    let path = checked_substitute_path(op.api_path, &[("webset", webset_id)])?;
    Ok((request::RequestSpec { op, body }, path))
}

fn dispatch_websets_searches_create(
    webset_id: &str,
    query: Option<&str>,
    count: Option<u32>,
    criteria: &[String],
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "searches", "create"])
        .expect("websets searches create is in registry");
    with_typed_error_context(op, globals, || {
        let (spec, path) =
            build_websets_searches_create_spec(webset_id, query, count, criteria, globals)?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_searches_get(
    webset_id: &str,
    search_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "searches", "get"])
        .expect("websets searches get is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path =
            checked_substitute_path(op.api_path, &[("webset", webset_id), ("id", search_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_searches_cancel(
    webset_id: &str,
    search_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "searches", "cancel"])
        .expect("websets searches cancel is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path =
            checked_substitute_path(op.api_path, &[("webset", webset_id), ("id", search_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_enrichments(
    sub: &cli::WebsetsEnrichmentsCmd,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    use cli::WebsetsEnrichmentsCmd;
    match sub {
        WebsetsEnrichmentsCmd::Create {
            webset_id,
            description,
            enrichment_format,
        } => dispatch_websets_enrichments_create(
            webset_id,
            description.as_deref(),
            *enrichment_format,
            globals,
            pretty,
        ),
        WebsetsEnrichmentsCmd::Get {
            webset_id,
            enrichment_id,
        } => dispatch_websets_enrichments_get(webset_id, enrichment_id, globals, pretty),
        WebsetsEnrichmentsCmd::Update {
            webset_id,
            enrichment_id,
            description,
            enrichment_format,
        } => dispatch_websets_enrichments_update(
            webset_id,
            enrichment_id,
            description.as_deref(),
            *enrichment_format,
            globals,
            pretty,
        ),
        WebsetsEnrichmentsCmd::Delete {
            webset_id,
            enrichment_id,
        } => dispatch_websets_enrichments_delete(webset_id, enrichment_id, globals, pretty),
        WebsetsEnrichmentsCmd::Cancel {
            webset_id,
            enrichment_id,
        } => dispatch_websets_enrichments_cancel(webset_id, enrichment_id, globals, pretty),
    }
}

fn build_websets_enrichments_create_spec(
    webset_id: &str,
    description: Option<&str>,
    format: Option<WebsetEnrichmentFormat>,
    globals: &GlobalArgs,
) -> Result<(request::RequestSpec, String), CliError> {
    let op = registry::lookup_by_segments(&["websets", "enrichments", "create"])
        .expect("websets enrichments create is in registry");
    let mut body = serde_json::Map::new();
    if let Some(description) = description {
        body.insert(
            "description".to_string(),
            serde_json::Value::String(description.to_string()),
        );
    }
    if let Some(format) = format {
        body.insert(
            "format".to_string(),
            serde_json::Value::String(format.as_str().to_string()),
        );
    }
    let body = apply_request_overrides(serde_json::Value::Object(body), globals)?;
    let has_description = body
        .get("description")
        .and_then(|value| value.as_str())
        .is_some_and(|value| !value.is_empty());
    if !has_description {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "websets enrichments create requires description (via --description, --body, or --set)",
            )
            .with_suggestion(
                "exa-agent websets enrichments create <webset> --description \"Company size\" --enrichment-format text",
            ),
        ));
    }
    let path = checked_substitute_path(op.api_path, &[("webset", webset_id)])?;
    Ok((request::RequestSpec { op, body }, path))
}

fn dispatch_websets_enrichments_create(
    webset_id: &str,
    description: Option<&str>,
    format: Option<WebsetEnrichmentFormat>,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "enrichments", "create"])
        .expect("websets enrichments create is in registry");
    with_typed_error_context(op, globals, || {
        let (spec, path) =
            build_websets_enrichments_create_spec(webset_id, description, format, globals)?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_enrichments_get(
    webset_id: &str,
    enrichment_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "enrichments", "get"])
        .expect("websets enrichments get is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path =
            checked_substitute_path(op.api_path, &[("webset", webset_id), ("id", enrichment_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn build_websets_enrichments_update_spec(
    webset_id: &str,
    enrichment_id: &str,
    description: Option<&str>,
    format: Option<WebsetEnrichmentFormat>,
    globals: &GlobalArgs,
) -> Result<(request::RequestSpec, String), CliError> {
    let op = registry::lookup_by_segments(&["websets", "enrichments", "update"])
        .expect("websets enrichments update is in registry");
    let mut body = serde_json::Map::new();
    if let Some(description) = description {
        body.insert(
            "description".to_string(),
            serde_json::Value::String(description.to_string()),
        );
    }
    if let Some(format) = format {
        body.insert(
            "format".to_string(),
            serde_json::Value::String(format.as_str().to_string()),
        );
    }
    let body = apply_request_overrides(serde_json::Value::Object(body), globals)?;
    if body.as_object().is_some_and(|object| object.is_empty()) {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "websets enrichments update requires at least one field via named flags, --body, or --set",
            )
            .with_suggestion(
                "exa-agent websets enrichments update <webset> <id> --description \"Updated label\"",
            ),
        ));
    }
    let path =
        checked_substitute_path(op.api_path, &[("webset", webset_id), ("id", enrichment_id)])?;
    Ok((request::RequestSpec { op, body }, path))
}

fn dispatch_websets_enrichments_update(
    webset_id: &str,
    enrichment_id: &str,
    description: Option<&str>,
    format: Option<WebsetEnrichmentFormat>,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "enrichments", "update"])
        .expect("websets enrichments update is in registry");
    with_typed_error_context(op, globals, || {
        let (spec, path) = build_websets_enrichments_update_spec(
            webset_id,
            enrichment_id,
            description,
            format,
            globals,
        )?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_enrichments_delete(
    webset_id: &str,
    enrichment_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "enrichments", "delete"])
        .expect("websets enrichments delete is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path =
            checked_substitute_path(op.api_path, &[("webset", webset_id), ("id", enrichment_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_enrichments_cancel(
    webset_id: &str,
    enrichment_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "enrichments", "cancel"])
        .expect("websets enrichments cancel is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path =
            checked_substitute_path(op.api_path, &[("webset", webset_id), ("id", enrichment_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_imports(
    sub: &WebsetsImportsCmd,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    match sub {
        WebsetsImportsCmd::Create { source, url, csv } => dispatch_websets_imports_create(
            source.as_deref(),
            url.as_deref(),
            csv.as_deref(),
            globals,
            pretty,
        ),
        WebsetsImportsCmd::List(pagination) => {
            dispatch_websets_imports_list(pagination, globals, pretty)
        }
        WebsetsImportsCmd::Get { import_id } => {
            dispatch_websets_imports_get(import_id, globals, pretty)
        }
        WebsetsImportsCmd::Update { import_id } => {
            dispatch_websets_imports_update(import_id, globals, pretty)
        }
        WebsetsImportsCmd::Delete { import_id } => {
            dispatch_websets_imports_delete(import_id, globals, pretty)
        }
    }
}

fn build_websets_imports_create_spec(
    source: Option<&str>,
    url: Option<&str>,
    csv: Option<&str>,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    if source.is_some_and(|source| source != "csv") {
        return Err(CliError::Usage(
            Diag::new(
                "invalid_value",
                "`websets imports create --source` currently supports only `csv`",
            )
            .with_suggestion("exa-agent websets imports create --source csv --body @import.json"),
        ));
    }
    if csv.is_some() {
        return Err(CliError::Usage(Diag::new(
            "not_implemented",
            "CSV upload convenience via --csv is deferred; build the import body with --body/--set instead",
        )));
    }
    if url.is_some() {
        return Err(CliError::Usage(Diag::new(
            "invalid_value",
            "`--url` upload convenience is deferred; build the import body with --body/--set instead",
        )));
    }
    let op = registry::lookup_by_segments(&["websets", "imports", "create"])
        .expect("websets imports create is in registry");
    let mut body = serde_json::Map::new();
    if source == Some("csv") {
        body.insert(
            "format".to_string(),
            serde_json::Value::String("csv".to_string()),
        );
    }
    let body = apply_request_overrides(serde_json::Value::Object(body), globals)?;
    validate_websets_import_create_body(&body)?;
    Ok(request::RequestSpec { op, body })
}

fn validate_websets_import_create_body(body: &serde_json::Value) -> Result<(), CliError> {
    let format = body.get("format").and_then(serde_json::Value::as_str);
    if let Some(format) = format {
        if format != "csv" {
            return Err(CliError::Usage(
                Diag::new(
                    "invalid_value",
                    "websets imports create currently supports only `format: \"csv\"`",
                )
                .with_suggestion("Set `format` to `csv` or pass `--source csv`."),
            ));
        }
    }

    let mut missing = Vec::new();
    if !body.get("size").is_some_and(serde_json::Value::is_number) {
        missing.push("size");
    }
    if !body.get("count").is_some_and(serde_json::Value::is_number) {
        missing.push("count");
    }
    if format != Some("csv") {
        missing.push("format=csv");
    }
    if !body.get("entity").is_some_and(serde_json::Value::is_object) {
        missing.push("entity");
    }
    if !missing.is_empty() {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                format!(
                    "websets imports create requires {} (via --body/--set; --source csv can fill format)",
                    missing.join(", ")
                ),
            )
            .with_suggestion(
                r#"exa-agent websets imports create --source csv --body '{"size":1024,"count":10,"entity":{"type":"company"}}'"#,
            ),
        ));
    }

    Ok(())
}

fn dispatch_websets_imports_create(
    source: Option<&str>,
    url: Option<&str>,
    csv: Option<&str>,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "imports", "create"])
        .expect("websets imports create is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_websets_imports_create_spec(source, url, csv, globals)?;
        dispatch_typed_command(spec, globals, pretty)
    })
}

fn dispatch_websets_imports_list(
    pagination: &PaginationArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "imports", "list"])
        .expect("websets imports list is in registry");
    with_typed_error_context(op, globals, || {
        validate_cursor_pagination(pagination)?;
        let spec = build_typed_spec(op, &[], globals)?;
        let query = pagination_query(pagination);
        if pagination.all && !(globals.print_request || globals.dry_run) {
            dispatch_paginated_typed_command(spec, globals, pretty, pagination, None, &[])
        } else {
            dispatch_typed_command_routed(spec, globals, pretty, None, &query, false, None)
        }
    })
}

fn dispatch_websets_imports_get(
    import_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "imports", "get"])
        .expect("websets imports get is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", import_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn build_websets_imports_update_spec(
    op: &'static registry::OperationDef,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let body = apply_request_overrides(serde_json::json!({}), globals)?;
    if body.as_object().is_some_and(|object| object.is_empty()) {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "websets imports update requires a request body via --body or --set",
            )
            .with_suggestion(
                "exa-agent websets imports update <id> --set title=\"Updated import\"",
            ),
        ));
    }
    Ok(request::RequestSpec { op, body })
}

fn dispatch_websets_imports_update(
    import_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "imports", "update"])
        .expect("websets imports update is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_websets_imports_update_spec(op, globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", import_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_imports_delete(
    import_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "imports", "delete"])
        .expect("websets imports delete is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", import_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn insert_monitor_behavior_config_field(
    body: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: serde_json::Value,
) {
    let behavior = body
        .entry("behavior".to_string())
        .or_insert_with(|| serde_json::json!({ "type": "search", "config": {} }));
    if let Some(behavior_obj) = behavior.as_object_mut() {
        if behavior_obj.get("type").is_none() {
            behavior_obj.insert("type".to_string(), serde_json::json!("search"));
        }
        let config = behavior_obj
            .entry("config".to_string())
            .or_insert_with(|| serde_json::json!({}));
        if let Some(config_obj) = config.as_object_mut() {
            config_obj.insert(key.to_string(), value);
        }
    }
}

fn build_websets_monitors_create_named_body(args: &WebsetsMonitorsCreateArgs) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    if let Some(webset_id) = &args.webset_id {
        body.insert(
            "websetId".to_string(),
            serde_json::Value::String(webset_id.clone()),
        );
    }
    if args.cron.is_some() || args.timezone.is_some() {
        let mut cadence = serde_json::Map::new();
        if let Some(cron) = &args.cron {
            cadence.insert("cron".to_string(), serde_json::Value::String(cron.clone()));
        }
        if let Some(timezone) = &args.timezone {
            cadence.insert(
                "timezone".to_string(),
                serde_json::Value::String(timezone.clone()),
            );
        }
        body.insert("cadence".to_string(), serde_json::Value::Object(cadence));
    }
    if let Some(query) = &args.query {
        insert_monitor_behavior_config_field(
            &mut body,
            "query",
            serde_json::Value::String(query.clone()),
        );
    }
    if let Some(count) = args.count {
        insert_monitor_behavior_config_field(&mut body, "count", serde_json::json!(count));
    }
    if !args.criteria.is_empty() {
        let criteria: Vec<serde_json::Value> = args
            .criteria
            .iter()
            .map(|description| serde_json::json!({ "description": description }))
            .collect();
        insert_monitor_behavior_config_field(
            &mut body,
            "criteria",
            serde_json::Value::Array(criteria),
        );
    }
    if let Some(behavior) = args.search_behavior {
        insert_monitor_behavior_config_field(
            &mut body,
            "behavior",
            serde_json::Value::String(behavior.as_str().to_string()),
        );
    }
    serde_json::Value::Object(body)
}

fn monitor_behavior_count_is_valid(body: &serde_json::Value) -> bool {
    body.pointer("/behavior/config/count")
        .and_then(serde_json::Value::as_f64)
        .is_some_and(|count| count.is_finite() && count >= 1.0)
}

fn websets_monitor_create_has_required_fields(body: &serde_json::Value) -> bool {
    let has_webset_id = body
        .get("websetId")
        .and_then(|value| value.as_str())
        .is_some_and(|value| !value.is_empty());
    let has_cron = body
        .pointer("/cadence/cron")
        .and_then(|value| value.as_str())
        .is_some_and(|value| !value.is_empty());
    let has_behavior_type = body
        .get("behavior")
        .and_then(|value| value.get("type"))
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .is_some();
    has_webset_id && has_cron && has_behavior_type && monitor_behavior_count_is_valid(body)
}

fn validate_websets_monitor_create_body(body: &serde_json::Value) -> Result<(), CliError> {
    if !websets_monitor_create_has_required_fields(body) {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "websets monitors create requires websetId, cadence.cron, behavior.type, and behavior.config.count >= 1 (via named flags, --body, or --set)",
            )
            .with_suggestion(
                "exa-agent websets monitors create --webset-id ws_abc --cron '0 9 * * 1' --count 10 --query \"new items\"",
            ),
        ));
    }
    Ok(())
}

fn build_websets_monitors_create_spec(
    args: &WebsetsMonitorsCreateArgs,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let op = registry::lookup_by_segments(&["websets", "monitors", "create"])
        .expect("websets monitors create is in registry");
    let body = build_websets_monitors_create_named_body(args);
    let body = apply_request_overrides(body, globals)?;
    validate_websets_monitor_create_body(&body)?;
    Ok(request::RequestSpec { op, body })
}

fn dispatch_websets_monitors_create(
    args: &WebsetsMonitorsCreateArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "monitors", "create"])
        .expect("websets monitors create is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_websets_monitors_create_spec(args, globals)?;
        dispatch_typed_command(spec, globals, pretty)
    })
}

fn websets_monitors_list_static_query(args: &WebsetsMonitorsListArgs) -> Vec<(String, String)> {
    let mut query = Vec::new();
    if let Some(webset_id) = &args.webset_id {
        query.push(("websetId".to_string(), webset_id.clone()));
    }
    query
}

fn dispatch_websets_monitors_list(
    args: &WebsetsMonitorsListArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "monitors", "list"])
        .expect("websets monitors list is in registry");
    with_typed_error_context(op, globals, || {
        validate_cursor_pagination(&args.pagination)?;
        let spec = build_typed_spec(op, &[], globals)?;
        let static_query = websets_monitors_list_static_query(args);
        let query = merge_static_and_pagination_query(&static_query, &args.pagination);
        if args.pagination.all && !(globals.print_request || globals.dry_run) {
            dispatch_paginated_typed_command(
                spec,
                globals,
                pretty,
                &args.pagination,
                None,
                &static_query,
            )
        } else {
            dispatch_typed_command_routed(spec, globals, pretty, None, &query, false, None)
        }
    })
}

fn dispatch_websets_monitors_get(
    monitor_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "monitors", "get"])
        .expect("websets monitors get is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", monitor_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn build_websets_monitors_update_named_body(args: &WebsetsMonitorsUpdateArgs) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    if let Some(status) = args.status {
        body.insert(
            "status".to_string(),
            serde_json::Value::String(status.as_str().to_string()),
        );
    }
    if args.cron.is_some() || args.timezone.is_some() {
        let mut cadence = serde_json::Map::new();
        if let Some(cron) = &args.cron {
            cadence.insert("cron".to_string(), serde_json::Value::String(cron.clone()));
        }
        if let Some(timezone) = &args.timezone {
            cadence.insert(
                "timezone".to_string(),
                serde_json::Value::String(timezone.clone()),
            );
        }
        body.insert("cadence".to_string(), serde_json::Value::Object(cadence));
    }
    if let Some(query) = &args.query {
        insert_monitor_behavior_config_field(
            &mut body,
            "query",
            serde_json::Value::String(query.clone()),
        );
    }
    if let Some(count) = args.count {
        insert_monitor_behavior_config_field(&mut body, "count", serde_json::json!(count));
    }
    if let Some(behavior) = args.search_behavior {
        insert_monitor_behavior_config_field(
            &mut body,
            "behavior",
            serde_json::Value::String(behavior.as_str().to_string()),
        );
    }
    serde_json::Value::Object(body)
}

fn validate_websets_monitor_update_body(body: &serde_json::Value) -> Result<(), CliError> {
    if body.as_object().is_some_and(|object| object.is_empty()) {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "websets monitors update requires at least one field via named flags, --body, or --set",
            )
            .with_suggestion(
                "exa-agent websets monitors update <id> --status disabled --cron '0 14 * * *'",
            ),
        ));
    }
    if body.get("behavior").is_some() {
        let behavior = body
            .get("behavior")
            .and_then(|value| value.as_object())
            .ok_or_else(|| {
                CliError::Usage(Diag::new(
                    "invalid_value",
                    "websets monitors update behavior must be an object when provided",
                ))
            })?;
        if behavior
            .get("type")
            .and_then(|value| value.as_str())
            .is_none_or(|value| value.is_empty())
        {
            return Err(CliError::Usage(Diag::new(
                "invalid_value",
                "websets monitors update behavior.type must be non-empty when behavior is provided",
            )));
        }
        if !monitor_behavior_count_is_valid(body) {
            return Err(CliError::Usage(Diag::new(
                "invalid_value",
                "websets monitors update behavior.config.count must be a number >= 1 when behavior is provided",
            )));
        }
    }
    if body.get("cadence").is_some() {
        let cadence = body
            .get("cadence")
            .and_then(|value| value.as_object())
            .ok_or_else(|| {
                CliError::Usage(Diag::new(
                    "invalid_value",
                    "websets monitors update cadence must be an object when provided",
                ))
            })?;
        if cadence
            .get("cron")
            .and_then(|value| value.as_str())
            .is_none_or(|value| value.is_empty())
        {
            return Err(CliError::Usage(Diag::new(
                "invalid_value",
                "websets monitors update cadence.cron must be non-empty when cadence is provided",
            )));
        }
    }
    Ok(())
}

fn build_websets_monitors_update_spec(
    args: &WebsetsMonitorsUpdateArgs,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let op = registry::lookup_by_segments(&["websets", "monitors", "update"])
        .expect("websets monitors update is in registry");
    let body = build_websets_monitors_update_named_body(args);
    let body = apply_request_overrides(body, globals)?;
    validate_websets_monitor_update_body(&body)?;
    Ok(request::RequestSpec { op, body })
}

fn dispatch_websets_monitors_update(
    monitor_id: &str,
    args: &WebsetsMonitorsUpdateArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "monitors", "update"])
        .expect("websets monitors update is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_websets_monitors_update_spec(args, globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", monitor_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_monitors_delete(
    monitor_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "monitors", "delete"])
        .expect("websets monitors delete is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", monitor_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_monitors_runs_list(
    monitor_id: &str,
    pagination: &PaginationArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "monitors", "runs", "list"])
        .expect("websets monitors runs list is in registry");
    with_typed_error_context(op, globals, || {
        validate_cursor_pagination(pagination)?;
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("monitor", monitor_id)])?;
        let query = pagination_query(pagination);
        if pagination.all && !(globals.print_request || globals.dry_run) {
            dispatch_paginated_typed_command(
                spec,
                globals,
                pretty,
                pagination,
                Some(path.as_str()),
                &[],
            )
        } else {
            dispatch_typed_command_routed(
                spec,
                globals,
                pretty,
                Some(path.as_str()),
                &query,
                false,
                None,
            )
        }
    })
}

fn dispatch_websets_monitors_runs_get(
    monitor_id: &str,
    run_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "monitors", "runs", "get"])
        .expect("websets monitors runs get is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path =
            checked_substitute_path(op.api_path, &[("monitor", monitor_id), ("id", run_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_monitors(
    sub: &cli::WebsetsMonitorsCmd,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    use cli::WebsetsMonitorsCmd;
    match sub {
        WebsetsMonitorsCmd::Create(args) => dispatch_websets_monitors_create(args, globals, pretty),
        WebsetsMonitorsCmd::List(args) => dispatch_websets_monitors_list(args, globals, pretty),
        WebsetsMonitorsCmd::Get { monitor_id } => {
            dispatch_websets_monitors_get(monitor_id, globals, pretty)
        }
        WebsetsMonitorsCmd::Update { monitor_id, args } => {
            dispatch_websets_monitors_update(monitor_id, args, globals, pretty)
        }
        WebsetsMonitorsCmd::Delete { monitor_id } => {
            dispatch_websets_monitors_delete(monitor_id, globals, pretty)
        }
        WebsetsMonitorsCmd::Runs { sub } => match sub {
            cli::WebsetsMonitorRunsCmd::List {
                monitor_id,
                pagination,
            } => dispatch_websets_monitors_runs_list(monitor_id, pagination, globals, pretty),
            cli::WebsetsMonitorRunsCmd::Get { monitor_id, run_id } => {
                dispatch_websets_monitors_runs_get(monitor_id, run_id, globals, pretty)
            }
        },
    }
}

fn websets_events_list_static_query(args: &WebsetsEventsListArgs) -> Vec<(String, String)> {
    let mut query = Vec::new();
    for event_type in &args.types {
        query.push(("types".to_string(), event_type.clone()));
    }
    if let Some(created_before) = &args.created_before {
        query.push(("createdBefore".to_string(), created_before.clone()));
    }
    if let Some(created_after) = &args.created_after {
        query.push(("createdAfter".to_string(), created_after.clone()));
    }
    query
}

fn dispatch_websets_events_list(
    args: &WebsetsEventsListArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "events", "list"])
        .expect("websets events list is in registry");
    with_typed_error_context(op, globals, || {
        validate_cursor_pagination(&args.pagination)?;
        let spec = build_typed_spec(op, &[], globals)?;
        let static_query = websets_events_list_static_query(args);
        let query = merge_static_and_pagination_query(&static_query, &args.pagination);
        if args.pagination.all && !(globals.print_request || globals.dry_run) {
            dispatch_paginated_typed_command(
                spec,
                globals,
                pretty,
                &args.pagination,
                None,
                &static_query,
            )
        } else {
            dispatch_typed_command_routed(spec, globals, pretty, None, &query, false, None)
        }
    })
}

fn dispatch_websets_events_get(
    event_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "events", "get"])
        .expect("websets events get is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", event_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_events(
    sub: &cli::WebsetsEventsCmd,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    use cli::WebsetsEventsCmd;
    match sub {
        WebsetsEventsCmd::List(args) => dispatch_websets_events_list(args, globals, pretty),
        WebsetsEventsCmd::Get { event_id } => {
            dispatch_websets_events_get(event_id, globals, pretty)
        }
    }
}

fn build_websets_webhooks_create_named_body(args: &WebsetsWebhooksCreateArgs) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    if let Some(url) = &args.url {
        body.insert("url".to_string(), serde_json::Value::String(url.clone()));
    }
    if !args.events.is_empty() {
        body.insert(
            "events".to_string(),
            serde_json::Value::Array(
                args.events
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }
    serde_json::Value::Object(body)
}

fn websets_webhooks_create_has_required_fields(body: &serde_json::Value) -> bool {
    let has_url = body
        .get("url")
        .and_then(|value| value.as_str())
        .is_some_and(|value| !value.is_empty());
    let has_events = websets_webhook_events_are_valid(body);
    has_url && has_events
}

fn websets_webhook_events_are_valid(body: &serde_json::Value) -> bool {
    body.get("events")
        .and_then(|value| value.as_array())
        .is_some_and(|events| {
            !events.is_empty()
                && events
                    .iter()
                    .all(|event| event.as_str().is_some_and(|event| !event.is_empty()))
        })
}

fn build_websets_webhooks_create_spec(
    args: &WebsetsWebhooksCreateArgs,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let op = registry::lookup_by_segments(&["websets", "webhooks", "create"])
        .expect("websets webhooks create is in registry");
    let body = build_websets_webhooks_create_named_body(args);
    let body = apply_request_overrides(body, globals)?;
    if !websets_webhooks_create_has_required_fields(&body) {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "websets webhooks create requires url and a non-empty events array (via --url/--event, --body, or --set)",
            )
            .with_suggestion(
                "exa-agent websets webhooks create --url https://example.com/hook --event webset.item.created",
            ),
        ));
    }
    Ok(request::RequestSpec { op, body })
}

fn websets_webhook_secret_warnings(args: &WebsetsWebhooksCreateArgs) -> Vec<serde_json::Value> {
    if args.secret_output.is_none() {
        vec![serde_json::json!({
            "code": "webhook_secret_ephemeral",
            "message": "Create responses include secret once; use --secret-output FILE to capture it or it will be lost.",
            "replacement": "exa-agent websets webhooks create --url https://example.com/hook --event webset.item.created --secret-output ./webhook.secret"
        })]
    } else {
        Vec::new()
    }
}

fn dispatch_websets_webhooks_create_live(
    spec: request::RequestSpec,
    secret_output: Option<SecretOutputReservation>,
    globals: &GlobalArgs,
    pretty: bool,
    extra_warnings: Vec<serde_json::Value>,
) -> Result<i32, CliError> {
    parse_user_headers(&globals.headers)?;
    let api_input = credential_input(auth::CredentialNamespace::Api, globals)?;
    let credential = auth::resolve_api_credential(&api_input, &auth::NoopKeyring)
        .map_err(|missing| auth::not_authenticated_error(&missing))?;
    let cfg = config::Config::load()?;
    let timeout = transport::resolve_timeout(globals, &cfg)?;
    let transport = UreqTransport::new(timeout);
    let request_id = transport::new_request_id();
    let mut warnings = typed_command_warnings(spec.op);
    warnings.extend(extra_warnings);
    let result = match execute_raw_with_request_id(
        &transport,
        RawExecuteParams {
            method: spec.op.method.as_str(),
            path: spec.op.api_path,
            query_raw: &[],
            body: typed_wire_body(&spec),
            globals,
            credential: &credential,
            request_id: request_id.clone(),
        },
    ) {
        Ok(result) => result,
        Err(err) => {
            return Err(maybe_record_pending_run_on_create_failure(
                err,
                &spec,
                globals,
                &request_id,
                spec.op.api_path,
            ));
        }
    };
    let mut data = transport::parse_response_data(&result.response.body);
    if let Some(output) = secret_output {
        let secret = data.get("secret").and_then(|value| value.as_str()).ok_or_else(|| {
            CliError::Upstream(
                Diag::new(
                    "upstream_malformed",
                    "websets webhooks create response did not include string `secret`; reserved --secret-output file was not written",
                )
                .with_suggestion("exa-agent websets webhooks list --limit 10"),
            )
        })?;
        output.commit(secret)?;
    }
    redaction::redact_json_value(&mut data);
    let count = transport::primary_count(&data);
    let hash = transport::data_hash(&data);
    let envelope = response_envelope(ResponseEnvelopeArgs {
        command: &spec.op.command(),
        method: &result.method,
        path: &result.path,
        operation: Some(spec.op),
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
        emit_stdout(&envelope, pretty);
    }
    Ok(0)
}

fn dispatch_websets_webhooks_create(
    args: &WebsetsWebhooksCreateArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "webhooks", "create"])
        .expect("websets webhooks create is in registry");
    with_typed_error_context(op, globals, || {
        if args.secret_output.as_deref() == Some("-") {
            return Err(CliError::Usage(Diag::new(
                "invalid_value",
                "Refusing to write webhook secret to stdout; use a file path for --secret-output",
            )));
        }
        let spec = build_websets_webhooks_create_spec(args, globals)?;
        let extra_warnings = websets_webhook_secret_warnings(args);
        if globals.print_request || globals.dry_run {
            return dispatch_typed_preview_with_warnings(
                spec,
                globals,
                pretty,
                TypedDispatchOptions::default(),
                &extra_warnings,
            );
        }
        if globals.raw {
            return Err(CliError::Usage(
                Diag::new(
                    "invalid_flag_combination",
                    "`--raw` cannot be combined with `websets webhooks create`; create responses can include one-time webhook secrets",
                )
                .with_suggestion("Use `--secret-output FILE` and the default JSON envelope."),
            ));
        }
        let secret_output = args
            .secret_output
            .as_deref()
            .map(reserve_webhook_secret_file)
            .transpose()?;
        dispatch_websets_webhooks_create_live(spec, secret_output, globals, pretty, extra_warnings)
    })
}

fn dispatch_websets_webhooks_list(
    pagination: &PaginationArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "webhooks", "list"])
        .expect("websets webhooks list is in registry");
    with_typed_error_context(op, globals, || {
        validate_cursor_pagination(pagination)?;
        let spec = build_typed_spec(op, &[], globals)?;
        let query = pagination_query(pagination);
        if pagination.all && !(globals.print_request || globals.dry_run) {
            dispatch_paginated_typed_command(spec, globals, pretty, pagination, None, &[])
        } else {
            dispatch_typed_command_routed(spec, globals, pretty, None, &query, false, None)
        }
    })
}

fn dispatch_websets_webhooks_get(
    webhook_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "webhooks", "get"])
        .expect("websets webhooks get is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", webhook_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn build_websets_webhooks_update_named_body(args: &WebsetsWebhooksUpdateArgs) -> serde_json::Value {
    let mut body = serde_json::Map::new();
    if let Some(url) = &args.url {
        body.insert("url".to_string(), serde_json::Value::String(url.clone()));
    }
    if !args.events.is_empty() {
        body.insert(
            "events".to_string(),
            serde_json::Value::Array(
                args.events
                    .iter()
                    .cloned()
                    .map(serde_json::Value::String)
                    .collect(),
            ),
        );
    }
    serde_json::Value::Object(body)
}

fn build_websets_webhooks_update_spec(
    args: &WebsetsWebhooksUpdateArgs,
    globals: &GlobalArgs,
) -> Result<request::RequestSpec, CliError> {
    let op = registry::lookup_by_segments(&["websets", "webhooks", "update"])
        .expect("websets webhooks update is in registry");
    let body = build_websets_webhooks_update_named_body(args);
    let body = apply_request_overrides(body, globals)?;
    if body.as_object().is_some_and(|object| object.is_empty()) {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "websets webhooks update requires at least one field via --url/--event, --body, or --set",
            )
            .with_suggestion(
                "exa-agent websets webhooks update <id> --url https://example.com/new-hook",
            ),
        ));
    }
    if body.get("events").is_some() && !websets_webhook_events_are_valid(&body) {
        return Err(CliError::Usage(Diag::new(
            "invalid_value",
            "websets webhooks update events must be a non-empty array of non-empty strings when provided",
        )));
    }
    Ok(request::RequestSpec { op, body })
}

fn dispatch_websets_webhooks_update(
    webhook_id: &str,
    args: &WebsetsWebhooksUpdateArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "webhooks", "update"])
        .expect("websets webhooks update is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_websets_webhooks_update_spec(args, globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", webhook_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn dispatch_websets_webhooks_delete(
    webhook_id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "webhooks", "delete"])
        .expect("websets webhooks delete is in registry");
    with_typed_error_context(op, globals, || {
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", webhook_id)])?;
        dispatch_typed_command_routed(spec, globals, pretty, Some(path.as_str()), &[], false, None)
    })
}

fn websets_webhook_attempts_list_static_query(
    args: &WebsetsWebhookAttemptsListArgs,
) -> Vec<(String, String)> {
    let mut query = Vec::new();
    if let Some(event_type) = &args.event_type {
        query.push(("eventType".to_string(), event_type.clone()));
    }
    if let Some(successful) = args.successful {
        query.push(("successful".to_string(), successful.to_string()));
    }
    query
}

fn dispatch_websets_webhook_attempts_list(
    webhook_id: &str,
    args: &WebsetsWebhookAttemptsListArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["websets", "webhooks", "attempts", "list"])
        .expect("websets webhooks attempts list is in registry");
    with_typed_error_context(op, globals, || {
        validate_cursor_pagination(&args.pagination)?;
        let spec = build_typed_spec(op, &[], globals)?;
        let path = checked_substitute_path(op.api_path, &[("id", webhook_id)])?;
        let static_query = websets_webhook_attempts_list_static_query(args);
        let query = merge_static_and_pagination_query(&static_query, &args.pagination);
        if args.pagination.all && !(globals.print_request || globals.dry_run) {
            dispatch_paginated_typed_command(
                spec,
                globals,
                pretty,
                &args.pagination,
                Some(path.as_str()),
                &static_query,
            )
        } else {
            dispatch_typed_command_routed(
                spec,
                globals,
                pretty,
                Some(path.as_str()),
                &query,
                false,
                None,
            )
        }
    })
}

fn dispatch_websets_webhooks(
    sub: &cli::WebsetsWebhooksCmd,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    use cli::WebsetsWebhooksCmd;
    match sub {
        WebsetsWebhooksCmd::Create(args) => dispatch_websets_webhooks_create(args, globals, pretty),
        WebsetsWebhooksCmd::List(pagination) => {
            dispatch_websets_webhooks_list(pagination, globals, pretty)
        }
        WebsetsWebhooksCmd::Get { webhook_id } => {
            dispatch_websets_webhooks_get(webhook_id, globals, pretty)
        }
        WebsetsWebhooksCmd::Update { webhook_id, args } => {
            dispatch_websets_webhooks_update(webhook_id, args, globals, pretty)
        }
        WebsetsWebhooksCmd::Delete { webhook_id } => {
            dispatch_websets_webhooks_delete(webhook_id, globals, pretty)
        }
        WebsetsWebhooksCmd::Attempts { sub } => match sub {
            cli::WebsetsWebhookAttemptsCmd::List { webhook_id, args } => {
                dispatch_websets_webhook_attempts_list(webhook_id, args, globals, pretty)
            }
        },
    }
}

fn dispatch_typed_preview_with_warnings(
    spec: request::RequestSpec,
    globals: &GlobalArgs,
    pretty: bool,
    options: TypedDispatchOptions<'_>,
    extra_warnings: &[serde_json::Value],
) -> Result<i32, CliError> {
    let op = spec.op;
    let path = options.path_override.unwrap_or(op.api_path);
    let mut warnings = typed_command_warnings(op);
    warnings.extend_from_slice(extra_warnings);
    emit_stdout(
        &redacted_preview_expanded(
            &spec,
            TypedPreviewOptions {
                path,
                query: options.query,
                expands_to: options.expands_to,
                extra_headers: options.extra_headers,
                command_override: options.command_override,
                globals: Some(globals),
                warnings: &warnings,
            },
        ),
        pretty,
    );
    Ok(0)
}

fn checked_substitute_path(template: &str, params: &[(&str, &str)]) -> Result<String, CliError> {
    for (key, value) in params {
        reject_placeholder_value(value, key)?;
    }
    Ok(substitute_path(template, params))
}

fn substitute_path(template: &str, params: &[(&str, &str)]) -> String {
    let mut path = template.to_string();
    for (key, value) in params {
        let value = transport::encode_path_segment(value);
        path = path.replace(&format!("{{{key}}}"), &value);
    }
    path
}

fn reject_placeholder_value(value: &str, arg_name: &str) -> Result<(), CliError> {
    if !looks_like_placeholder(value) {
        return Ok(());
    }
    Err(CliError::Usage(
        Diag::new(
            "placeholder_argument",
            format!("{arg_name} looks like a placeholder: `{value}`"),
        )
        .with_suggestion(placeholder_example_command(arg_name)),
    ))
}

fn looks_like_placeholder(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() {
        return false;
    }
    if is_placeholder_token(value) {
        return true;
    }
    value.split('/').any(is_placeholder_token)
}

fn is_placeholder_token(value: &str) -> bool {
    // `<...>` and `$VAR` are unambiguous placeholders. The word-prefix forms are matched
    // case-SENSITIVELY (uppercase only) so a real lowercase id like `example_abc123` is
    // never rejected, while the canonical agent paste `YOUR_WEBSET_ID` still is.
    (value.starts_with('<') && value.ends_with('>') && value.len() > 2)
        || value.starts_with('$')
        || value.starts_with("YOUR_")
        || value.starts_with("REPLACE_")
        || value.starts_with("EXAMPLE_")
}

fn placeholder_example_command(arg_name: &str) -> &'static str {
    match arg_name {
        "path" => "exa-agent raw GET /search --dry-run",
        "researchId" => "exa-agent research get research_123 --dry-run",
        "runId" => "exa-agent monitor runs get mon_123 run_123 --dry-run",
        "monitor" => "exa-agent websets monitors get mon_123 --dry-run",
        _ => "exa-agent websets get webset_123 --dry-run",
    }
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

fn typed_preview_headers(
    body: &serde_json::Value,
    extra_headers: Option<&[(String, String)]>,
    globals: Option<&GlobalArgs>,
) -> Vec<serde_json::Value> {
    let mut headers = if let Some(headers) = extra_headers.filter(|headers| !headers.is_empty()) {
        header_preview(headers)
    } else if body_wants_stream(body) {
        vec![serde_json::json!({
            "name": "Accept",
            "value": "text/event-stream"
        })]
    } else {
        Vec::new()
    };
    if let Some(globals) = globals {
        if let Some(key) = globals.idempotency_key.as_deref() {
            headers.extend(header_preview(&[(
                "Idempotency-Key".to_string(),
                key.to_string(),
            )]));
        }
        if let Some(beta) = globals.beta.as_deref() {
            headers.extend(header_preview(&[(
                "x-exa-beta".to_string(),
                beta.to_string(),
            )]));
        }
    }
    headers
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
        _ => Err(CliError::Usage(Diag::new(
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

fn ensure_registry_yes_confirmed(
    op: &'static registry::OperationDef,
    globals: &GlobalArgs,
) -> Result<(), CliError> {
    if !op.destructive()
        || globals.yes
        || !matches!(op.confirm_protocol(), Some(registry::ConfirmProtocol::Yes))
    {
        return Ok(());
    }
    let command = op.command();
    Err(CliError::Safety(
        Diag::new(
            "confirmation_required",
            format!("Refusing live `{command}` because it is destructive; preview with `--dry-run` or pass `--yes` after inspection"),
        )
        .with_suggestion(format!("exa-agent {command} ... --yes")),
    ))
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
        let warnings = typed_command_warnings(spec.op);
        emit_stdout(
            &redacted_preview_expanded(
                spec,
                TypedPreviewOptions {
                    path,
                    query: options.query,
                    expands_to: options.expands_to,
                    extra_headers: options.extra_headers,
                    command_override: options.command_override,
                    globals: Some(globals),
                    warnings: &warnings,
                },
            ),
            pretty,
        );
        return Ok(0);
    }
    ensure_registry_yes_confirmed(spec.op, globals)?;

    let effective_globals = options
        .extra_headers
        .filter(|headers| !headers.is_empty())
        .map(|headers| globals_with_extra_headers(globals, headers))
        .unwrap_or_else(|| globals.clone());
    let credential = resolve_operation_credential(spec.op, &effective_globals)?;
    let cfg = config::Config::load()?;
    let timeout = transport::resolve_timeout(&effective_globals, &cfg)?;
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
    static_query: &[(String, String)],
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
    let credential = resolve_operation_credential(spec.op, globals)?;
    let cfg = config::Config::load()?;
    let timeout = transport::resolve_timeout(globals, &cfg)?;
    let transport = UreqTransport::new(timeout);
    execute_paginated_live(
        &transport,
        &spec,
        PaginatedExecution {
            globals,
            credential: &credential,
            pretty,
            pagination,
            route: PaginatedRoute {
                path_override,
                static_query,
            },
        },
    )
}

struct PaginatedExecution<'a> {
    globals: &'a GlobalArgs,
    credential: &'a auth::ResolvedCredential,
    pretty: bool,
    pagination: &'a PaginationArgs,
    route: PaginatedRoute<'a>,
}

struct PaginatedRoute<'a> {
    path_override: Option<&'a str>,
    static_query: &'a [(String, String)],
}

fn execute_paginated_live<T: Transport>(
    transport: &T,
    spec: &request::RequestSpec,
    execution: PaginatedExecution<'_>,
) -> Result<i32, CliError> {
    let globals = execution.globals;
    let credential = execution.credential;
    let pretty = execution.pretty;
    let pagination = execution.pagination;
    let path_override = execution.route.path_override;
    let static_query = execution.route.static_query;
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
        let mut query = static_query.to_vec();
        query.extend(pagination_query_with_cursor(pagination, cursor.as_deref()));
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
                spec.op,
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
            operation: Some(spec.op),
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
    op: &registry::OperationDef,
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
        operation: Some(op),
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

    let op = specs
        .first()
        .map(|spec| spec.op)
        .expect("contents chunking creates at least one spec");
    let credential = resolve_operation_credential(op, globals)?;
    let cfg = config::Config::load()?;
    let timeout = transport::resolve_timeout(globals, &cfg)?;
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
            Some(spec.op),
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

    let mut data = transport::parse_response_data(&result.response.body);
    // Defense in depth: a create response may carry secret material (e.g. a freshly
    // minted key). Secret-capturing creates use their own --secret-output path; this
    // scrubs secret-named/shaped fields on the generic typed create path so nothing
    // sensitive is echoed to stdout.
    if spec.op.idempotency_sensitive {
        redaction::redact_json_value(&mut data);
    }
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
        operation: Some(spec.op),
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
    operation: Option<&registry::OperationDef>,
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
        let mut data = transport::parse_response_data(&result.response.body);
        if operation.is_some_and(|op| op.idempotency_sensitive) {
            redaction::redact_json_value(&mut data);
        }
        let envelope = response_envelope(ResponseEnvelopeArgs {
            command,
            method: &result.method,
            path: &result.path,
            operation,
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
        operation,
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
        operation: None,
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
            let mut warnings = Vec::new();
            let (authenticated, source, key_fingerprint, last4, checked) = match api {
                Ok(resolved) => {
                    let status = resolved.status();
                    if auth::looks_like_service_key(resolved.secret.expose()) {
                        warnings.push(
                            "EXA_API_KEY looks like a service key; API commands require a normal Exa API key"
                                .to_string(),
                        );
                    }
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
        AuthCmd::Test => dispatch_auth_test(globals, pretty),
    }
}

fn dispatch_auth_test(globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
    let op = registry::lookup_by_segments(&["team", "info"]).expect("team info is in the registry");
    // Honor the universal contract: --dry-run/--print-request never touch the network.
    // `auth test` makes a live probe, so a preview just describes the request it would send.
    if globals.print_request || globals.dry_run {
        emit_stdout(
            &serde_json::json!({
                "schema": "exa.cli.auth_test.v1",
                "ok": true,
                "dryRun": true,
                "method": op.method.as_str(),
                "endpoint": op.api_path,
                "note": "auth test makes a live authenticated request to verify the credential; run without --dry-run to probe.",
            }),
            pretty,
        );
        return Ok(0);
    }
    let api_input = credential_input(auth::CredentialNamespace::Api, globals)?;
    let credential = auth::resolve_api_credential(&api_input, &auth::NoopKeyring)
        .map_err(|missing| auth::not_authenticated_error(&missing))?;
    reject_mismatched_credential_scope(&credential)?;
    let cfg = config::Config::load()?;
    let timeout = transport::resolve_timeout(globals, &cfg)?;
    let transport = UreqTransport::new(timeout);
    let request_id = transport::new_request_id();
    let result = execute_raw_with_request_id(
        &transport,
        RawExecuteParams {
            method: op.method.as_str(),
            path: op.api_path,
            query_raw: &[],
            body: serde_json::Value::Null,
            globals,
            credential: &credential,
            request_id: request_id.clone(),
        },
    )?;
    let mut data = transport::parse_response_data(&result.response.body);
    redaction::redact_json_value(&mut data);
    emit_stdout(
        &serde_json::json!({
            "schema": "exa.cli.auth_test.v1",
            "ok": true,
            "authenticated": true,
            "source": credential.source.label(),
            "profile": credential.profile,
            "endpoint": op.api_path,
            "team": data,
        }),
        pretty,
    );
    Ok(0)
}

fn dispatch_schema(sub: &SchemaCmd, globals: &GlobalArgs, pretty: bool) -> Result<i32, CliError> {
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
            let body = read_validate_input_body(globals)?;
            let validation = validate_registry_input(op, &body);
            emit_stdout(
                &serde_json::json!({
                    "schema": "exa.cli.schema_validate_input.v1",
                    "ok": true,
                    "command": op.command(),
                    "valid": validation.valid,
                    "details": validation.details,
                    "suggestedCommand": validation.suggested_command,
                    "note": validation.note,
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
                    "Search is not cursor-paginated: use --num-results and follow error.suggestedCommand when an invocation is rejected.",
                    "Do not pass managed auth headers; use EXA_API_KEY or auth login.",
                    "Errors are JSON on stderr with stable error.code values; run robot-docs errors for the full dictionary."
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
                    "exa-agent team info --dry-run --print-request --compact"
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

struct ValidateInputOutcome {
    valid: serde_json::Value,
    details: Option<serde_json::Value>,
    suggested_command: Option<String>,
    note: Option<String>,
}

fn read_validate_input_body(globals: &GlobalArgs) -> Result<serde_json::Value, CliError> {
    let Some(raw) = globals.body.as_deref() else {
        return Err(CliError::Usage(
            Diag::new(
                "missing_required_argument",
                "`schema validate-input` requires `--body` with a JSON object to validate",
            )
            .with_suggestion(
                "exa-agent schema validate-input search --body '{\"query\":\"example\"}'",
            ),
        ));
    };
    let source = request::parse_body_source(raw)?;
    let body = request::read_body_source(source)?;
    if !body.is_object() {
        return Err(CliError::Usage(
            Diag::new(
                "invalid_value",
                "`schema validate-input --body` must be a JSON object",
            )
            .with_suggestion(
                "exa-agent schema validate-input search --body '{\"query\":\"example\"}'",
            ),
        ));
    }
    Ok(body)
}

fn validate_registry_input(
    op: &registry::OperationDef,
    body: &serde_json::Value,
) -> ValidateInputOutcome {
    if op.fields.is_empty() {
        return ValidateInputOutcome {
            valid: serde_json::Value::Null,
            details: None,
            suggested_command: None,
            note: Some(format!(
                "structural validation is unsupported for `{}` because no request fields are modeled in the registry",
                op.command()
            )),
        };
    }

    for field in op.fields {
        if field.required && !body_field_present(body, field.body_path) {
            return ValidateInputOutcome {
                valid: serde_json::Value::Bool(false),
                details: Some(serde_json::json!({
                    "issue": "missing_required_field",
                    "field": field.body_path,
                    "flag": field.flag,
                })),
                suggested_command: Some(suggested_validate_input_command(op, body, field)),
                note: None,
            };
        }
    }

    // Type check every modeled field actually present, keyed off the registry's
    // FieldKind — so validate-input is genuinely registry-driven, not just a
    // required-presence + two-enum check. Catches e.g. numResults:"five".
    for field in op.fields {
        if let Some(value) = body_value_at_path(body, field.body_path) {
            if let Some(issue) = validate_field_kind(field, value) {
                return ValidateInputOutcome {
                    valid: serde_json::Value::Bool(false),
                    details: Some(issue),
                    suggested_command: Some(suggested_validate_input_command(op, body, field)),
                    note: None,
                };
            }
        }
    }

    for field in op.fields {
        if let Some(value) = body_value_at_path(body, field.body_path) {
            if let Some(issue) = validate_enum_field(op, field, value) {
                return ValidateInputOutcome {
                    valid: serde_json::Value::Bool(false),
                    details: Some(issue),
                    suggested_command: Some(suggested_validate_input_command(op, body, field)),
                    note: None,
                };
            }
        }
    }

    // Reuse the live command's own value-range validator so validate-input never
    // reports `valid:true` for a body the command would reject (e.g. numResults:500).
    if op.command() == "search" {
        let query = body
            .get("query")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("your query");
        if let Err(err) = validate_search_num_results_body(body, query) {
            let diag = err.diag();
            return ValidateInputOutcome {
                valid: serde_json::Value::Bool(false),
                details: Some(serde_json::json!({
                    "issue": "invalid_value",
                    "field": "numResults",
                    "message": diag.message,
                })),
                suggested_command: diag.suggested_command.clone(),
                note: None,
            };
        }
    }

    ValidateInputOutcome {
        valid: serde_json::Value::Bool(true),
        details: None,
        suggested_command: None,
        note: None,
    }
}

fn validate_field_kind(
    field: &registry::FieldDef,
    value: &serde_json::Value,
) -> Option<serde_json::Value> {
    use registry::FieldKind;
    if value.is_null() {
        return None; // absence is the required-check's job, not a type error
    }
    let (ok, expected) = match field.kind {
        FieldKind::Str => (value.is_string(), "string"),
        FieldKind::Int => (value.is_i64() || value.is_u64(), "integer"),
        FieldKind::Num => (value.is_number(), "number"),
        FieldKind::Bool => (value.is_boolean(), "boolean"),
        FieldKind::StrArray => (
            value
                .as_array()
                .is_some_and(|items| items.iter().all(serde_json::Value::is_string)),
            "array of strings",
        ),
        FieldKind::Json => (true, "json"),
    };
    if ok {
        return None;
    }
    Some(serde_json::json!({
        "issue": "invalid_field_type",
        "field": field.body_path,
        "flag": field.flag,
        "expected": expected,
    }))
}

fn body_field_present(body: &serde_json::Value, path: &str) -> bool {
    body_value_at_path(body, path).is_some_and(|value| !value.is_null())
}

fn body_value_at_path<'a>(
    body: &'a serde_json::Value,
    path: &str,
) -> Option<&'a serde_json::Value> {
    if path.is_empty() || path.split('.').any(str::is_empty) {
        return None;
    }
    let mut current = body;
    for segment in path.split('.') {
        current = if segment.bytes().all(|b| b.is_ascii_digit()) {
            let idx = segment.parse::<usize>().ok()?;
            current.as_array()?.get(idx)?
        } else {
            current.as_object()?.get(segment)?
        };
    }
    Some(current)
}

fn validate_enum_field(
    op: &registry::OperationDef,
    field: &registry::FieldDef,
    value: &serde_json::Value,
) -> Option<serde_json::Value> {
    let raw = value.as_str()?;
    let allowed = enum_values_for_field(op, field)?;
    let normalized = raw.trim().to_ascii_lowercase();
    if allowed
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(&normalized))
    {
        return None;
    }
    Some(serde_json::json!({
        "issue": "invalid_enum_value",
        "field": field.body_path,
        "flag": field.flag,
        "value": raw,
        "allowed": allowed,
    }))
}

fn enum_values_for_field(
    op: &registry::OperationDef,
    field: &registry::FieldDef,
) -> Option<&'static [&'static str]> {
    if op.command() != "search" {
        return None;
    }
    match field.flag {
        "type" => Some(SEARCH_TYPE_VALUES),
        "category" => Some(SEARCH_CATEGORY_VALUES),
        _ => None,
    }
}

fn suggested_validate_input_command(
    op: &registry::OperationDef,
    body: &serde_json::Value,
    field: &registry::FieldDef,
) -> String {
    if op.command() == "search" {
        let query = body
            .get("query")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("your query");
        if field.flag == "type" {
            return format!("exa-agent search {} --type auto", shell_quote(query));
        }
        if field.flag == "category" {
            return format!("exa-agent search {} --category company", shell_quote(query));
        }
        if field.body_path == "query" {
            let mut command = format!("exa-agent search {} --compact", shell_quote("your query"));
            if let Some(num_results) = body.get("numResults").and_then(serde_json::Value::as_i64) {
                command = format!(
                    "exa-agent search {} --num-results {num_results}",
                    shell_quote("your query")
                );
            }
            return command;
        }
        return format!(
            "exa-agent search {} --body '{}'",
            shell_quote(query),
            shell_quote(&body.to_string())
        );
    }

    format!("exa-agent schema show {} --compact", op.command())
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

fn resolve_operation_credential(
    op: &'static registry::OperationDef,
    globals: &GlobalArgs,
) -> Result<auth::ResolvedCredential, CliError> {
    let namespace = match op.namespace {
        registry::Namespace::Api => auth::CredentialNamespace::Api,
        registry::Namespace::Service => auth::CredentialNamespace::Service,
    };
    let input = credential_input(namespace, globals)?;
    let resolved = match namespace {
        auth::CredentialNamespace::Api => auth::resolve_api_credential(&input, &auth::NoopKeyring),
        auth::CredentialNamespace::Service => {
            auth::resolve_service_credential(&input, &auth::NoopKeyring)
        }
    }
    .map_err(|missing| auth::not_authenticated_error(&missing))?;
    if namespace == auth::CredentialNamespace::Service
        && auth::looks_like_api_key(resolved.secret.expose())
    {
        return Err(CliError::Auth(
            Diag::new(
                "key_scope_mismatch",
                "resolved service key looks like a normal Exa API key; admin commands require EXA_SERVICE_KEY",
            )
            .with_suggestion("Set EXA_SERVICE_KEY to a service/admin key, not EXA_API_KEY."),
        ));
    }
    reject_mismatched_credential_scope(&resolved)?;
    Ok(resolved)
}

fn reject_mismatched_credential_scope(
    credential: &auth::ResolvedCredential,
) -> Result<(), CliError> {
    if credential.namespace == auth::CredentialNamespace::Api
        && auth::looks_like_service_key(credential.secret.expose())
    {
        return Err(CliError::Auth(
            Diag::new(
                "key_scope_mismatch",
                "resolved API key looks like a service/admin key; API commands require EXA_API_KEY",
            )
            .with_suggestion("Set EXA_API_KEY to a normal Exa API key, not EXA_SERVICE_KEY."),
        ));
    }
    Ok(())
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
    let warnings = typed_command_warnings(spec.op);
    redacted_preview_expanded(
        spec,
        TypedPreviewOptions {
            path: spec.op.api_path,
            query: &[],
            expands_to: None,
            extra_headers: None,
            command_override: None,
            globals: None,
            warnings: &warnings,
        },
    )
}

fn redacted_preview_expanded(
    spec: &request::RequestSpec,
    preview: TypedPreviewOptions<'_>,
) -> serde_json::Value {
    let mut body = typed_wire_body(spec);
    redaction::redact_json_value(&mut body);
    let command = preview
        .command_override
        .map(str::to_string)
        .unwrap_or_else(|| spec.op.command());
    let mut request = serde_json::json!({
        "method": spec.op.method.as_str(),
        "path": preview.path,
        "query": query_preview(preview.query),
        "body": body,
    });
    let headers = typed_preview_headers(&body, preview.extra_headers, preview.globals);
    if !headers.is_empty() {
        request["headers"] = serde_json::Value::Array(headers);
    }
    let data = data_with_expands_to(
        serde_json::json!({
            "request": request,
            "dryRun": true,
        }),
        preview.expands_to,
    );
    let count = transport::primary_count(data.get("request").unwrap_or(&data));
    let hash = transport::data_hash(&data);
    response_envelope(ResponseEnvelopeArgs {
        command: &command,
        method: spec.op.method.as_str(),
        path: preview.path,
        operation: Some(spec.op),
        request_id: "req_dry_run",
        profile: "default",
        correlation_id: None,
        data,
        count,
        data_hash: hash,
        retries: 0,
        warnings: preview.warnings,
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
    reject_placeholder_value(&args.path, "path")?;
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
                operation: None,
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
    reject_mismatched_credential_scope(&credential)?;
    let body = raw_body(globals)?;
    let cfg = config::Config::load()?;
    let timeout = transport::resolve_timeout(globals, &cfg)?;
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
            None,
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

    let mut data = transport::parse_response_data(&result.response.body);
    // `raw` is the escape hatch; `--raw` above already emits exact upstream bytes for
    // callers who want them. The enveloped form scrubs secret-named/shaped fields so an
    // agent that hits a key/secret-minting endpoint via `raw` never prints credentials.
    redaction::redact_json_value(&mut data);
    let count = transport::primary_count(&data);
    let hash = transport::data_hash(&data);
    let envelope = response_envelope(ResponseEnvelopeArgs {
        command: "raw",
        method: &result.method,
        path: &result.path,
        operation: None,
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

    static PENDING_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
        capabilities: &[],
        body_builder: None,
        validators: &[],
        mixed_status_exit: false,
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
            "https://example.test",
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
                PaginatedExecution {
                    globals: &globals,
                    credential: &credential,
                    pretty: false,
                    pagination: &pagination,
                    route: PaginatedRoute {
                        path_override: None,
                        static_query: &[],
                    },
                },
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
        let _pending_lock = PENDING_TEST_LOCK.lock().unwrap();
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
    fn admin_create_idempotency_controls_retry_and_header() {
        let _pending_lock = PENDING_TEST_LOCK.lock().unwrap();
        let op = registry::lookup_by_segments(&["admin", "keys", "create"]).unwrap();
        let spec = request::RequestSpec {
            op,
            body: serde_json::json!({"name":"ci-key"}),
        };
        let credential = auth::resolve_service_credential(
            &CredentialInput {
                explicit: Some("svc-admin-secret".into()),
                ..Default::default()
            },
            &NoopKeyring,
        )
        .unwrap();

        let pending_path = std::env::temp_dir().join(format!(
            "exa-agent-pending-admin-lib-{}-{}.jsonl",
            std::process::id(),
            transport::new_request_id()
        ));
        let _ = std::fs::remove_file(&pending_path);
        let _pending_override = PendingPathGuard::set(pending_path.clone());

        let unkeyed = FakeTransport::default();
        unkeyed.push_ok_json(503, "down");
        unkeyed.push_ok_json(200, r#"{"id":"key_abc"}"#);
        let unkeyed_globals =
            parse_globals(&["--format", "json", "--service-key", "svc-admin-secret"]);
        let err = execute_typed_live(
            &unkeyed,
            &spec,
            &unkeyed_globals,
            &credential,
            TypedExecution {
                request_id: "req_admin_pending",
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
        assert_eq!(unkeyed.recorded_requests().len(), 1);
        assert_eq!(
            err.diag().details.as_ref().unwrap()["pendingRunWritten"],
            true
        );
        let raw = std::fs::read_to_string(&pending_path).unwrap();
        let record: serde_json::Value = serde_json::from_str(raw.lines().next().unwrap()).unwrap();
        assert_eq!(record["operationId"], "create-api-key");
        assert_eq!(record["command"], "admin keys create");

        let keyed = FakeTransport::default();
        keyed.push_ok_json(503, "down");
        keyed.push_ok_json(200, r#"{"id":"key_abc"}"#);
        let keyed_globals = parse_globals(&[
            "--format",
            "json",
            "--service-key",
            "svc-admin-secret",
            "--idempotency-key",
            "idem-admin-create",
        ]);
        assert_eq!(
            execute_typed_live(
                &keyed,
                &spec,
                &keyed_globals,
                &credential,
                TypedExecution {
                    request_id: "req_admin_keyed",
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
        let recorded = keyed.recorded_requests();
        assert_eq!(recorded.len(), 2);
        assert!(recorded.iter().all(|request| request
            .headers
            .iter()
            .any(|(name, value)| name == "Idempotency-Key" && value == "idem-admin-create")));

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
            operation: None,
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
