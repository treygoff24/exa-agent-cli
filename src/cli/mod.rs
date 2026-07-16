//! The clap derive surface (D13). The only place clap types live. Command structs
//! collect flags; logic lives in `request`/`exec`/dispatch.

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(
    name = "exa-agent",
    version,
    about = "Agent-first CLI over the full Exa API surface",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(flatten)]
    pub globals: GlobalArgs,
    #[command(subcommand)]
    pub command: Command,
}

impl std::fmt::Debug for Cli {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cli")
            .field("globals", &self.globals)
            .field("command", &command_path(&self.command))
            .finish()
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[value(rename_all = "lower")]
pub enum Format {
    Human,
    Json,
    Ndjson,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[value(rename_all = "kebab-case")]
pub enum SearchType {
    Auto,
    Fast,
    Instant,
    DeepLite,
    Deep,
    DeepReasoning,
}

impl SearchType {
    pub fn as_str(self) -> &'static str {
        match self {
            SearchType::Auto => "auto",
            SearchType::Fast => "fast",
            SearchType::Instant => "instant",
            SearchType::DeepLite => "deep-lite",
            SearchType::Deep => "deep",
            SearchType::DeepReasoning => "deep-reasoning",
        }
    }
}

pub const SEARCH_TYPE_VALUES: &[&str] = &[
    "auto",
    "fast",
    "instant",
    "deep-lite",
    "deep",
    "deep-reasoning",
];

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchCategory {
    Company,
    People,
    #[value(name = "research paper")]
    ResearchPaper,
    News,
    #[value(name = "personal site")]
    PersonalSite,
    #[value(name = "financial report")]
    FinancialReport,
}

pub const SEARCH_CATEGORY_VALUES: &[&str] = &[
    "company",
    "people",
    "research paper",
    "news",
    "personal site",
    "financial report",
];

impl SearchCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            SearchCategory::Company => "company",
            SearchCategory::People => "people",
            SearchCategory::ResearchPaper => "research paper",
            SearchCategory::News => "news",
            SearchCategory::PersonalSite => "personal site",
            SearchCategory::FinancialReport => "financial report",
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[value(rename_all = "lower")]
pub enum Effort {
    Auto,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

impl Effort {
    pub fn as_str(self) -> &'static str {
        match self {
            Effort::Auto => "auto",
            Effort::Minimal => "minimal",
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[value(rename_all = "lower")]
pub enum InputFormat {
    Text,
    Json,
    Jsonl,
    Csv,
    Auto,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[value(rename_all = "lower")]
pub enum GroupBy {
    Hour,
    Day,
    Month,
}

/// Universal flags, inherited by every subcommand (`global = true`).
#[derive(Args, Clone)]
#[command(next_help_heading = "Global options")]
pub struct GlobalArgs {
    /// Output format. `ndjson` emits one result per line for list-shaped data, else compact JSON.
    #[arg(long, global = true, value_enum, ignore_case = true)]
    pub format: Option<Format>,
    /// Force JSON output.
    #[arg(long, global = true)]
    pub json: bool,
    /// Force NDJSON output; list-shaped data emits item lines plus a final summary line.
    #[arg(long, global = true)]
    pub ndjson: bool,
    /// Emit exact upstream bytes without the CLI envelope.
    #[arg(long, global = true)]
    pub raw: bool,
    /// Pretty-print JSON envelopes.
    #[arg(long, global = true, conflicts_with = "compact")]
    pub pretty: bool,
    /// Emit compact single-line JSON envelopes.
    #[arg(long, global = true)]
    pub compact: bool,
    /// Write output to a file when supported.
    #[arg(short = 'o', long, global = true)]
    pub output: Option<String>,
    /// Spill oversized `data` payloads above this byte limit; 0 disables.
    #[arg(long, global = true, default_value_t = crate::DEFAULT_MAX_OUTPUT_BYTES)]
    pub max_output_bytes: u64,
    /// Correlation id echoed into request metadata.
    #[arg(long, global = true, env = "EXA_CORRELATION_ID")]
    pub correlation_id: Option<String>,
    /// API key for normal Exa API calls.
    #[arg(long, global = true, conflicts_with = "api_key_stdin")]
    pub api_key: Option<String>,
    /// Read the API key from stdin.
    #[arg(
        long,
        global = true,
        conflicts_with_all = ["api_key", "service_key_stdin"]
    )]
    pub api_key_stdin: bool,
    /// Service key for admin/team-management calls.
    #[arg(long, global = true, conflicts_with = "service_key_stdin")]
    pub service_key: Option<String>,
    /// Read the service key from stdin.
    #[arg(
        long,
        global = true,
        conflicts_with_all = ["service_key", "api_key_stdin"]
    )]
    pub service_key_stdin: bool,
    /// Named config/auth profile.
    #[arg(long, global = true, env = "EXA_PROFILE")]
    pub profile: Option<String>,
    /// Override the Exa API base URL.
    #[arg(long, global = true)]
    pub base_url: Option<String>,
    /// Add a non-secret HTTP header as `Name: value`.
    #[arg(long = "header", global = true)]
    pub headers: Vec<String>,
    /// Opt into an upstream beta header value.
    #[arg(long, global = true)]
    pub beta: Option<String>,
    /// Total request timeout, e.g. `30s` or `250ms`.
    #[arg(long, global = true)]
    pub timeout: Option<String>,
    /// Connect timeout, e.g. `10s`.
    #[arg(long, global = true)]
    pub connect_timeout: Option<String>,
    /// Max retry count for retry-safe failures.
    #[arg(long, global = true, default_value_t = 2)]
    pub retry: u32,
    /// Honor upstream Retry-After delays when retrying.
    #[arg(long, global = true, default_value_t = true)]
    pub retry_after: bool,
    /// Idempotency key for safe create retries.
    #[arg(long, global = true)]
    pub idempotency_key: Option<String>,
    /// Read command input from a file or `-` for stdin where supported.
    #[arg(long, global = true)]
    pub input: Option<String>,
    /// Format of `--input`.
    #[arg(long, global = true, value_enum, ignore_case = true)]
    pub input_format: Option<InputFormat>,
    /// Set a JSON body field as `path=value`; repeatable and applied last.
    #[arg(long = "set", global = true)]
    pub set: Vec<String>,
    /// Merge a JSON object body from inline JSON, `@file`, or `-`.
    #[arg(long, global = true)]
    pub body: Option<String>,
    /// Reduce diagnostics.
    #[arg(long, global = true)]
    pub quiet: bool,
    /// Increase diagnostics.
    #[arg(long, global = true, conflicts_with = "quiet")]
    pub verbose: bool,
    /// Append redacted request/response trace JSONL to FILE.
    #[arg(long, global = true)]
    pub trace: Option<String>,
    /// Disable ANSI color.
    #[arg(long, global = true)]
    pub no_color: bool,
    /// Confirm destructive operations that require yes.
    #[arg(long, global = true)]
    pub yes: bool,
    /// Build the request but do not send it.
    #[arg(long, global = true)]
    pub dry_run: bool,
    /// Include the exact upstream request preview in dry-run output.
    #[arg(long, global = true)]
    pub print_request: bool,
}

impl std::fmt::Debug for GlobalArgs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GlobalArgs")
            .field("format", &self.format)
            .field("json", &self.json)
            .field("ndjson", &self.ndjson)
            .field("raw", &self.raw)
            .field("pretty", &self.pretty)
            .field("compact", &self.compact)
            .field("output", &self.output)
            .field("max_output_bytes", &self.max_output_bytes)
            .field("correlation_id", &self.correlation_id)
            .field("api_key", &self.api_key.as_ref().map(|_| "<redacted>"))
            .field("api_key_stdin", &self.api_key_stdin)
            .field(
                "service_key",
                &self.service_key.as_ref().map(|_| "<redacted>"),
            )
            .field("service_key_stdin", &self.service_key_stdin)
            .field("profile", &self.profile)
            .field("base_url", &self.base_url)
            .field(
                "headers",
                &self
                    .headers
                    .iter()
                    .map(|h| {
                        let name = h.split_once(':').map(|(name, _)| name).unwrap_or(h);
                        if crate::redaction::is_secret_name(name) {
                            format!("{}: <redacted>", name.trim())
                        } else {
                            h.clone()
                        }
                    })
                    .collect::<Vec<_>>(),
            )
            .field("beta", &self.beta)
            .field("timeout", &self.timeout)
            .field("connect_timeout", &self.connect_timeout)
            .field("retry", &self.retry)
            .field("retry_after", &self.retry_after)
            .field("idempotency_key", &self.idempotency_key)
            .field("input", &self.input)
            .field("input_format", &self.input_format)
            .field(
                "set",
                &self
                    .set
                    .iter()
                    .map(|v| {
                        let key = v.split_once('=').map(|(key, _)| key).unwrap_or(v);
                        if crate::redaction::is_secret_name(key) {
                            format!("{key}=<redacted>")
                        } else {
                            v.clone()
                        }
                    })
                    .collect::<Vec<_>>(),
            )
            .field("body", &self.body.as_ref().map(|_| "<redacted>"))
            .field("quiet", &self.quiet)
            .field("verbose", &self.verbose)
            .field("trace", &self.trace)
            .field("no_color", &self.no_color)
            .field("yes", &self.yes)
            .field("dry_run", &self.dry_run)
            .field("print_request", &self.print_request)
            .finish()
    }
}

#[derive(Args, Debug, Default)]
pub struct PaginationArgs {
    #[arg(long)]
    pub limit: Option<u32>,
    #[arg(long)]
    pub cursor: Option<String>,
    #[arg(long)]
    pub all: bool,
    #[arg(long)]
    pub max_pages: Option<u32>,
    #[arg(long)]
    pub page_delay: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run a search (POST /search).
    Search(SearchArgs),
    /// Fetch page contents (POST /contents).
    Contents(ContentsArgs),
    /// Find similar pages (POST /findSimilar). Deprecated upstream.
    Similar(SimilarArgs),
    /// Cited answer (POST /answer).
    Answer(AnswerArgs),
    /// Exa Code context snippets (POST /context).
    Context(ContextArgs),
    /// Top-level Search Monitors (/monitors).
    Monitor {
        #[command(subcommand)]
        sub: MonitorCmd,
    },
    /// Agent API (/agent/runs).
    Agent {
        #[command(subcommand)]
        sub: AgentCmd,
    },
    /// Legacy research API (/research/v1).
    Research {
        #[command(subcommand)]
        sub: ResearchCmd,
    },
    /// Websets API (/websets/v0/websets).
    Websets {
        #[command(subcommand)]
        sub: WebsetsCmd,
    },
    /// Team quota and concurrency (GET /websets/v0/teams/me).
    Team {
        #[command(subcommand)]
        sub: Option<TeamCmd>,
    },
    /// Gated admin surface (EXA_SERVICE_KEY + admin host).
    Admin {
        #[command(subcommand)]
        sub: AdminCmd,
    },
    /// CLI self-description (offline). Alias: describe.
    #[command(visible_alias = "describe")]
    Capabilities(CapabilitiesArgs),
    /// Embedded API/CLI schema (offline).
    Schema {
        #[command(subcommand)]
        sub: SchemaCmd,
    },
    /// Paste-ready agent playbook (offline).
    RobotDocs {
        #[command(subcommand)]
        sub: RobotDocsCmd,
    },
    /// Read-only diagnostics (offline by default).
    Doctor(DoctorArgs),
    /// Credential management.
    Auth {
        #[command(subcommand)]
        sub: AuthCmd,
    },
    /// Config file and profiles.
    Config {
        #[command(subcommand)]
        sub: ConfigCmd,
    },
    /// Macro → `answer QUESTION`.
    Ask(AskArgs),
    /// Macro → `contents URL... --text --summary-query ...`.
    Fetch(FetchArgs),
    /// Escape hatch: call any Exa endpoint with full auth/output/error contracts.
    Raw(RawArgs),
}

#[derive(Args, Debug)]
pub struct SearchArgs {
    /// The search query.
    pub query: String,
    /// Number of results, 1..=100 (maps `numResults`). Search is not cursor-paginated.
    #[arg(
        short = 'n',
        long,
        value_name = "N",
        num_args = 0..=1,
        default_missing_value = "",
        allow_negative_numbers = true
    )]
    pub num_results: Option<String>,
    /// Return text in each result. Bare --text caps search text at 1500 chars/result; default highlights are usually smaller. Use --text full for uncapped.
    #[arg(
        long,
        value_name = "N|full",
        num_args = 0..=1,
        default_missing_value = "",
        allow_negative_numbers = true
    )]
    pub text: Option<String>,
    /// Return query-aware highlights. Bare/default caps at 800 chars/result; N overrides the cap.
    #[arg(
        long,
        value_name = "N",
        num_args = 0..=1,
        default_missing_value = "",
        allow_negative_numbers = true
    )]
    pub highlights: Option<String>,
    /// Return metadata-only search results; disables default 800-char highlights.
    #[arg(long, conflicts_with = "highlights")]
    pub no_highlights: bool,
    /// Search type.
    #[arg(long, value_enum, ignore_case = true)]
    pub r#type: Option<SearchType>,
    /// Result category.
    ///
    /// Valid values: company, people, research paper, news, personal site, financial report.
    #[arg(long, value_name = "CATEGORY")]
    pub category: Option<String>,
    /// Restrict results to matching domains.
    #[arg(long, value_name = "DOMAIN")]
    pub include_domain: Vec<String>,
    /// Exclude matching domains.
    #[arg(long, value_name = "DOMAIN")]
    pub exclude_domain: Vec<String>,
    /// Earliest estimated publication date.
    #[arg(long, visible_alias = "published-after", value_name = "ISO")]
    pub start_published_date: Option<String>,
    /// Latest estimated publication date.
    #[arg(long, visible_alias = "published-before", value_name = "ISO")]
    pub end_published_date: Option<String>,
    /// Common mistake: search uses --num-results, not --limit.
    #[arg(
        long,
        hide = true,
        value_name = "N",
        num_args = 0..=1,
        default_missing_value = "",
        allow_negative_numbers = true
    )]
    pub limit: Option<String>,
    /// Common mistake: search uses --num-results, not --count.
    #[arg(
        long,
        hide = true,
        value_name = "N",
        num_args = 0..=1,
        default_missing_value = "",
        allow_negative_numbers = true
    )]
    pub count: Option<String>,
    /// Common mistake: search is not cursor-paginated.
    #[arg(long, hide = true)]
    pub all: bool,
    /// Common mistake: use typed filter flags instead.
    #[arg(long, hide = true, value_name = "FILTER")]
    pub filter: Option<String>,
}

fn search_type_flag(value: &Option<SearchType>) -> Option<String> {
    value.map(|kind| kind.as_str().to_string())
}

impl SearchArgs {
    pub fn into_flag_values(&self) -> Vec<(&'static str, Option<String>)> {
        vec![
            ("query", Some(self.query.clone())),
            ("num-results", self.num_results.clone()),
            ("text", self.text.clone()),
            ("highlights", self.highlights.clone()),
            (
                "no-highlights",
                self.no_highlights.then(|| "false".to_string()),
            ),
            ("type", search_type_flag(&self.r#type)),
            ("category", self.category.clone()),
            ("include-domain", str_array_flag(&self.include_domain)),
            ("exclude-domain", str_array_flag(&self.exclude_domain)),
            ("start-published-date", self.start_published_date.clone()),
            ("end-published-date", self.end_published_date.clone()),
        ]
    }
}

#[derive(Args, Debug)]
pub struct ContentsArgs {
    /// URLs to fetch.
    #[arg(
        required_unless_present = "ids",
        conflicts_with = "ids",
        value_name = crate::registry::field_value_name("contents", "urls").expect("contents urls metadata"),
        num_args = 1..
    )]
    pub urls: Vec<String>,
    #[arg(
        long,
        conflicts_with = "urls",
        value_name = crate::registry::field_value_name("contents", "ids").expect("contents ids metadata"),
        num_args = 1..
    )]
    pub ids: Vec<String>,
    #[arg(
        long,
        help = crate::registry::field_input_help("contents", "text").expect("contents text metadata"),
        value_name = crate::registry::field_value_name("contents", "text").expect("contents text metadata"),
        num_args = 0..=1,
        default_missing_value = "",
        allow_negative_numbers = true
    )]
    pub text: Option<String>,
    #[arg(
        long,
        value_name = crate::registry::field_value_name("contents", "summary-query").expect("contents summary-query metadata"),
        num_args = 1
    )]
    pub summary_query: Option<String>,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=100))]
    pub chunk_size: Option<u32>,
}

impl ContentsArgs {
    pub fn into_flag_values(&self) -> Vec<(&'static str, Option<String>)> {
        vec![
            ("urls", str_array_flag(&self.urls)),
            ("ids", str_array_flag(&self.ids)),
            ("text", self.text.clone()),
            ("summary-query", self.summary_query.clone()),
        ]
    }
}

#[derive(Args, Debug)]
pub struct SimilarArgs {
    pub url: String,
    #[arg(short = 'n', long, value_parser = clap::value_parser!(u32).range(1..=100))]
    pub num_results: Option<u32>,
    #[arg(long)]
    pub exclude_source_domain: bool,
    #[arg(long, value_enum, ignore_case = true)]
    pub category: Option<SearchCategory>,
    /// Return text in each result. Bare --text caps similar text at 1500 chars/result; use --text full for uncapped.
    #[arg(
        long,
        value_name = "N|full",
        num_args = 0..=1,
        default_missing_value = "",
        allow_negative_numbers = true
    )]
    pub text: Option<String>,
}

fn similar_num_results_flag(value: &Option<u32>) -> Option<String> {
    value.map(|n| n.to_string())
}

fn similar_category_flag(value: &Option<SearchCategory>) -> Option<String> {
    value.map(|category| category.as_str().to_string())
}

impl SimilarArgs {
    pub fn into_flag_values(&self) -> Vec<(&'static str, Option<String>)> {
        vec![
            ("url", Some(self.url.clone())),
            ("num-results", similar_num_results_flag(&self.num_results)),
            (
                "exclude-source-domain",
                bool_flag(self.exclude_source_domain),
            ),
            ("category", similar_category_flag(&self.category)),
            ("text", self.text.clone()),
        ]
    }
}

#[derive(Args, Debug)]
pub struct AnswerArgs {
    pub question: String,
    #[arg(long)]
    pub text: bool,
    #[arg(long)]
    pub stream: bool,
    #[arg(long)]
    pub output_schema: Option<String>,
}

impl AnswerArgs {
    pub fn into_flag_values(&self) -> Vec<(&'static str, Option<String>)> {
        vec![
            ("question", Some(self.question.clone())),
            ("text", bool_flag(self.text)),
            ("stream", bool_flag(self.stream)),
        ]
    }
}

#[derive(Args, Debug)]
pub struct ContextArgs {
    pub query: String,
    /// Token budget: `dynamic` (default) or an integer from 50 to 100000.
    #[arg(long)]
    pub tokens: Option<String>,
}

impl ContextArgs {
    pub fn into_flag_values(&self) -> Vec<(&'static str, Option<String>)> {
        vec![
            ("query", Some(self.query.clone())),
            ("tokens", self.tokens.clone()),
        ]
    }
}

fn bool_flag(value: bool) -> Option<String> {
    value.then(|| "true".to_string())
}

fn str_array_flag(values: &[String]) -> Option<String> {
    (!values.is_empty()).then(|| crate::request::encode_str_array(values))
}

#[derive(Subcommand, Debug)]
pub enum MonitorCmd {
    /// POST /monitors [create-POST].
    Create(MonitorCreateArgs),
    /// GET /monitors.
    List(MonitorListArgs),
    /// GET /monitors/{id}.
    Get { id: String },
    /// PATCH /monitors/{id}.
    Update {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        schedule: Option<String>,
        #[arg(long)]
        status: Option<String>,
        #[arg(long)]
        webhook_url: Option<String>,
    },
    /// DELETE /monitors/{id}.
    Delete { id: String },
    /// POST /monitors/{id}/trigger.
    Trigger { id: String },
    /// POST /monitors/batch.
    Batch(MonitorBatchArgs),
    /// Monitor run history.
    Runs {
        #[command(subcommand)]
        sub: MonitorRunsCmd,
    },
}

#[derive(Args, Debug)]
pub struct MonitorCreateArgs {
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long)]
    pub schedule: Option<String>,
    #[arg(long)]
    pub webhook_url: Option<String>,
    #[arg(long)]
    pub secret_output: Option<String>,
}

#[derive(Args, Debug, Default)]
pub struct MonitorListArgs {
    #[command(flatten)]
    pub pagination: PaginationArgs,
    #[arg(long)]
    pub status: Option<String>,
    #[arg(long)]
    pub name: Option<String>,
    /// Metadata filter as `key=value` (repeatable; encoded as `metadata[key]=value`).
    #[arg(long = "metadata", value_name = "KEY=VALUE")]
    pub metadata: Vec<String>,
}

#[derive(Args, Debug, Default)]
pub struct MonitorBatchArgs {
    /// Required with live `dry_run:false` and `action=delete` (pass `delete`).
    #[arg(long)]
    pub confirm: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum MonitorRunsCmd {
    /// GET /monitors/{id}/runs.
    List {
        monitor_id: String,
        #[command(flatten)]
        pagination: PaginationArgs,
    },
    /// GET /monitors/{id}/runs/{runId}.
    Get { monitor_id: String, run_id: String },
}

#[derive(Subcommand, Debug)]
pub enum AgentCmd {
    /// POST /agent/runs (alias of `runs create`).
    Run(AgentRunArgs),
    /// Agent run lifecycle.
    Runs {
        #[command(subcommand)]
        sub: AgentRunsCmd,
    },
}

#[derive(Args, Debug)]
pub struct AgentRunsEventsArgs {
    pub id: String,
    #[arg(long)]
    pub stream: bool,
    #[arg(long)]
    pub last_event_id: Option<String>,
    #[command(flatten)]
    pub pagination: PaginationArgs,
}

#[derive(Args, Debug)]
pub struct AgentRunArgs {
    pub query: String,
    #[arg(long)]
    pub output_schema: Option<String>,
    #[arg(long)]
    pub input: Option<String>,
    #[arg(long)]
    pub input_row: Vec<String>,
    #[arg(long)]
    pub exclusion: Option<String>,
    #[arg(long)]
    pub previous_run_id: Option<String>,
    #[arg(long, value_enum, ignore_case = true)]
    pub effort: Option<Effort>,
    #[arg(long)]
    pub data_source: Vec<String>,
    #[arg(long)]
    pub metadata: Option<String>,
    #[arg(long)]
    pub stream: bool,
}

#[derive(Subcommand, Debug)]
pub enum AgentRunsCmd {
    /// POST /agent/runs [create-POST].
    Create(AgentRunArgs),
    /// GET /agent/runs.
    List(PaginationArgs),
    /// GET /agent/runs/{id}.
    Get { id: String },
    /// GET /agent/runs/{id}/events.
    Events(AgentRunsEventsArgs),
    /// POST /agent/runs/{id}/cancel.
    Cancel { id: String },
    /// DELETE /agent/runs/{id}.
    Delete { id: String },
}

#[derive(Subcommand, Debug)]
pub enum ResearchCmd {
    /// POST /research/v1 [create-POST].
    Create(ResearchCreateArgs),
    /// GET /research/v1.
    List(PaginationArgs),
    /// GET /research/v1/{researchId}.
    Get { research_id: String },
}

#[derive(Args, Debug)]
pub struct ResearchCreateArgs {
    pub query: String,
    #[arg(long)]
    pub stream: bool,
}

#[derive(Subcommand, Debug)]
pub enum WebsetsCmd {
    /// POST /websets/v0/websets [create-POST].
    Create(WebsetsCreateArgs),
    /// GET /websets/v0/websets.
    List(WebsetsListArgs),
    /// GET /websets/v0/websets/{id}.
    Get { id: String },
    /// POST /websets/v0/websets/{id}.
    Update { id: String },
    /// DELETE /websets/v0/websets/{id}.
    Delete { id: String },
    /// POST /websets/v0/websets/{id}/cancel.
    Cancel { id: String },
    /// POST /websets/v0/websets/preview.
    Preview(WebsetsPreviewArgs),
    /// Webset items.
    Items {
        #[command(subcommand)]
        sub: WebsetsItemsCmd,
    },
    /// Webset searches.
    Searches {
        #[command(subcommand)]
        sub: WebsetsSearchesCmd,
    },
    /// Webset enrichments.
    Enrichments {
        #[command(subcommand)]
        sub: WebsetsEnrichmentsCmd,
    },
    /// CSV/URL imports.
    Imports {
        #[command(subcommand)]
        sub: WebsetsImportsCmd,
    },
    /// Websets monitors (/websets/v0/monitors) — distinct from top-level `monitor`.
    Monitors {
        #[command(subcommand)]
        sub: WebsetsMonitorsCmd,
    },
    /// Webset events.
    Events {
        #[command(subcommand)]
        sub: WebsetsEventsCmd,
    },
    /// Webset webhooks.
    Webhooks {
        #[command(subcommand)]
        sub: WebsetsWebhooksCmd,
    },
}

#[derive(Args, Debug, Default)]
pub struct WebsetsListArgs {
    #[command(flatten)]
    pub pagination: PaginationArgs,
    #[arg(long)]
    pub search: Option<String>,
}

#[derive(Args, Debug)]
pub struct WebsetsCreateArgs {
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    pub count: Option<u32>,
    /// Common mistake: Websets search count is `--count`, not `--num-results`.
    #[arg(
        long,
        hide = true,
        value_name = "N",
        num_args = 0..=1,
        default_missing_value = "",
        allow_negative_numbers = true
    )]
    pub num_results: Option<String>,
}

#[derive(Args, Debug)]
pub struct WebsetsPreviewArgs {
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..=10))]
    pub count: Option<u32>,
    #[arg(long)]
    pub criteria: Vec<String>,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[value(rename_all = "lower")]
pub enum WebsetEnrichmentFormat {
    Text,
    Date,
    Number,
    Options,
    Email,
    Phone,
    Url,
}

impl WebsetEnrichmentFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            WebsetEnrichmentFormat::Text => "text",
            WebsetEnrichmentFormat::Date => "date",
            WebsetEnrichmentFormat::Number => "number",
            WebsetEnrichmentFormat::Options => "options",
            WebsetEnrichmentFormat::Email => "email",
            WebsetEnrichmentFormat::Phone => "phone",
            WebsetEnrichmentFormat::Url => "url",
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum WebsetsItemsCmd {
    /// GET /websets/v0/websets/{webset}/items.
    List {
        webset_id: String,
        #[command(flatten)]
        pagination: PaginationArgs,
        #[arg(long = "source-id")]
        source_id: Option<String>,
    },
    /// GET /websets/v0/websets/{webset}/items/{id}.
    Get { webset_id: String, item_id: String },
    /// DELETE /websets/v0/websets/{webset}/items/{id}.
    Delete { webset_id: String, item_id: String },
}

#[derive(Subcommand, Debug)]
pub enum WebsetsSearchesCmd {
    /// POST /websets/v0/websets/{webset}/searches [create-POST].
    Create {
        webset_id: String,
        #[arg(long)]
        query: Option<String>,
        #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
        count: Option<u32>,
        #[arg(long)]
        criteria: Vec<String>,
    },
    /// GET /websets/v0/websets/{webset}/searches/{id}.
    Get {
        webset_id: String,
        search_id: String,
    },
    /// POST /websets/v0/websets/{webset}/searches/{id}/cancel.
    Cancel {
        webset_id: String,
        search_id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum WebsetsEnrichmentsCmd {
    /// POST /websets/v0/websets/{webset}/enrichments [create-POST].
    Create {
        webset_id: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, value_enum, ignore_case = true)]
        enrichment_format: Option<WebsetEnrichmentFormat>,
    },
    /// GET /websets/v0/websets/{webset}/enrichments/{id}.
    Get {
        webset_id: String,
        enrichment_id: String,
    },
    /// PATCH /websets/v0/websets/{webset}/enrichments/{id}.
    Update {
        webset_id: String,
        enrichment_id: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long, value_enum, ignore_case = true)]
        enrichment_format: Option<WebsetEnrichmentFormat>,
    },
    /// DELETE /websets/v0/websets/{webset}/enrichments/{id}.
    Delete {
        webset_id: String,
        enrichment_id: String,
    },
    /// POST /websets/v0/websets/{webset}/enrichments/{id}/cancel.
    Cancel {
        webset_id: String,
        enrichment_id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum WebsetsImportsCmd {
    /// POST /websets/v0/imports [create-POST].
    Create {
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        csv: Option<String>,
    },
    /// GET /websets/v0/imports.
    List(PaginationArgs),
    /// GET /websets/v0/imports/{id}.
    Get { import_id: String },
    /// PATCH /websets/v0/imports/{id}.
    Update { import_id: String },
    /// DELETE /websets/v0/imports/{id}.
    Delete { import_id: String },
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[value(rename_all = "lower")]
pub enum WebsetMonitorSearchBehavior {
    Override,
    Append,
}

impl WebsetMonitorSearchBehavior {
    pub fn as_str(self) -> &'static str {
        match self {
            WebsetMonitorSearchBehavior::Override => "override",
            WebsetMonitorSearchBehavior::Append => "append",
        }
    }
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[value(rename_all = "lower")]
pub enum WebsetMonitorStatus {
    Enabled,
    Disabled,
}

impl WebsetMonitorStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            WebsetMonitorStatus::Enabled => "enabled",
            WebsetMonitorStatus::Disabled => "disabled",
        }
    }
}

#[derive(Args, Debug, Default)]
pub struct WebsetsMonitorsCreateArgs {
    #[arg(long = "webset-id")]
    pub webset_id: Option<String>,
    #[arg(long)]
    pub cron: Option<String>,
    #[arg(long)]
    pub timezone: Option<String>,
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    pub count: Option<u32>,
    #[arg(long)]
    pub criteria: Vec<String>,
    #[arg(long = "search-behavior", value_enum, ignore_case = true)]
    pub search_behavior: Option<WebsetMonitorSearchBehavior>,
}

#[derive(Args, Debug, Default)]
pub struct WebsetsMonitorsListArgs {
    #[command(flatten)]
    pub pagination: PaginationArgs,
    #[arg(long = "webset-id")]
    pub webset_id: Option<String>,
}

#[derive(Args, Debug, Default)]
pub struct WebsetsMonitorsUpdateArgs {
    #[arg(long, value_enum, ignore_case = true)]
    pub status: Option<WebsetMonitorStatus>,
    #[arg(long)]
    pub cron: Option<String>,
    #[arg(long)]
    pub timezone: Option<String>,
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long, value_parser = clap::value_parser!(u32).range(1..))]
    pub count: Option<u32>,
    #[arg(long = "search-behavior", value_enum, ignore_case = true)]
    pub search_behavior: Option<WebsetMonitorSearchBehavior>,
}

#[derive(Subcommand, Debug)]
pub enum WebsetsMonitorsCmd {
    /// POST /websets/v0/monitors [create-POST].
    Create(WebsetsMonitorsCreateArgs),
    /// GET /websets/v0/monitors.
    List(WebsetsMonitorsListArgs),
    /// GET /websets/v0/monitors/{id}.
    Get { monitor_id: String },
    /// PATCH /websets/v0/monitors/{id}.
    Update {
        monitor_id: String,
        #[command(flatten)]
        args: WebsetsMonitorsUpdateArgs,
    },
    /// DELETE /websets/v0/monitors/{id}.
    Delete { monitor_id: String },
    /// Monitor runs under Websets monitors.
    Runs {
        #[command(subcommand)]
        sub: WebsetsMonitorRunsCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum WebsetsMonitorRunsCmd {
    /// GET /websets/v0/monitors/{monitor}/runs.
    List {
        monitor_id: String,
        #[command(flatten)]
        pagination: PaginationArgs,
    },
    /// GET /websets/v0/monitors/{monitor}/runs/{id}.
    Get { monitor_id: String, run_id: String },
}

#[derive(Args, Debug, Default)]
pub struct WebsetsEventsListArgs {
    #[command(flatten)]
    pub pagination: PaginationArgs,
    #[arg(long = "type")]
    pub types: Vec<String>,
    #[arg(long = "created-before")]
    pub created_before: Option<String>,
    #[arg(long = "created-after")]
    pub created_after: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum WebsetsEventsCmd {
    /// GET /websets/v0/events.
    List(WebsetsEventsListArgs),
    /// GET /websets/v0/events/{id}.
    Get { event_id: String },
}

#[derive(Args, Debug, Default)]
pub struct WebsetsWebhooksCreateArgs {
    #[arg(long)]
    pub url: Option<String>,
    #[arg(long = "event")]
    pub events: Vec<String>,
    #[arg(long)]
    pub secret_output: Option<String>,
}

#[derive(Args, Debug, Default)]
pub struct WebsetsWebhooksUpdateArgs {
    #[arg(long)]
    pub url: Option<String>,
    #[arg(long = "event")]
    pub events: Vec<String>,
}

#[derive(Args, Debug, Default)]
pub struct WebsetsWebhookAttemptsListArgs {
    #[command(flatten)]
    pub pagination: PaginationArgs,
    #[arg(long = "event-type")]
    pub event_type: Option<String>,
    #[arg(long)]
    pub successful: Option<bool>,
}

#[derive(Subcommand, Debug)]
pub enum WebsetsWebhooksCmd {
    /// POST /websets/v0/webhooks [create-POST].
    Create(WebsetsWebhooksCreateArgs),
    /// GET /websets/v0/webhooks.
    List(PaginationArgs),
    /// GET /websets/v0/webhooks/{id}.
    Get { webhook_id: String },
    /// PATCH /websets/v0/webhooks/{id}.
    Update {
        webhook_id: String,
        #[command(flatten)]
        args: WebsetsWebhooksUpdateArgs,
    },
    /// DELETE /websets/v0/webhooks/{id}.
    Delete { webhook_id: String },
    /// Webhook delivery attempts.
    Attempts {
        #[command(subcommand)]
        sub: WebsetsWebhookAttemptsCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum WebsetsWebhookAttemptsCmd {
    /// GET /websets/v0/webhooks/{id}/attempts.
    List {
        webhook_id: String,
        #[command(flatten)]
        args: WebsetsWebhookAttemptsListArgs,
    },
}

#[derive(Subcommand, Debug)]
pub enum TeamCmd {
    /// GET /websets/v0/teams/me.
    Info,
}

#[derive(Subcommand, Debug)]
pub enum AdminCmd {
    /// Admin API keys.
    Keys {
        #[command(subcommand)]
        sub: AdminKeysCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum AdminKeysCmd {
    /// POST /api-keys [create-POST].
    Create(AdminKeysCreateArgs),
    /// GET /api-keys.
    List,
    /// GET /api-keys/{id}.
    Get { key_id: String },
    /// PUT /api-keys/{id}.
    Update {
        key_id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        rate_limit: Option<u32>,
        #[arg(long)]
        budget_cents: Option<u64>,
        /// Clear the key budget by sending budgetCents:null.
        #[arg(long, conflicts_with = "budget_cents")]
        clear_budget_cents: bool,
    },
    /// DELETE /api-keys/{id}.
    Delete {
        key_id: String,
        #[arg(long)]
        confirm: Option<String>,
    },
    /// GET /api-keys/{id}/usage.
    Usage {
        key_id: String,
        #[arg(long)]
        start_date: Option<String>,
        #[arg(long)]
        end_date: Option<String>,
        #[arg(long, value_enum, ignore_case = true)]
        group_by: Option<GroupBy>,
    },
}

#[derive(Args, Debug)]
pub struct AdminKeysCreateArgs {
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub rate_limit: Option<u32>,
    #[arg(long)]
    pub budget_cents: Option<u64>,
    /// Write the one-time created API key to this file (mode 0600). Required: the
    /// key is returned once and is never printed to stdout.
    #[arg(long)]
    pub secret_output: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum SchemaCmd {
    /// List every known operation/schema.
    List,
    /// Show one schema by name.
    Show { name: String },
    /// Export embedded spec artifacts.
    Export(SchemaExportArgs),
    /// Validate a request body against a schema.
    ValidateInput(SchemaValidateInputArgs),
    /// Compare embedded vs live spec (writes only with --output).
    Refresh(SchemaRefreshArgs),
}

#[derive(Args, Debug)]
pub struct SchemaExportArgs {
    #[arg(long)]
    pub api: Option<String>,
    #[arg(long)]
    pub cli: Option<String>,
}

#[derive(Args, Debug)]
pub struct SchemaValidateInputArgs {
    pub command: String,
}

#[derive(Args, Debug)]
pub struct SchemaRefreshArgs {
    #[arg(long)]
    pub check: bool,
}

#[derive(Subcommand, Debug)]
pub enum RobotDocsCmd {
    /// Agent playbook overview.
    Guide,
    /// Machine-readable command list.
    Commands,
    /// Error and exit-code table.
    Errors,
    /// Task-oriented examples.
    Examples(RobotDocsExamplesArgs),
    /// Copy-paste prompts for coding agents.
    Prompts,
}

#[derive(Args, Debug)]
pub struct RobotDocsExamplesArgs {
    #[arg(long)]
    pub task: Option<String>,
}

#[derive(Args, Debug)]
pub struct DoctorArgs {
    #[arg(long)]
    pub online: bool,
    #[arg(long)]
    pub check: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub enum AuthCmd {
    /// Show credential source and profile.
    Status,
    /// Network auth probe.
    Test,
    /// Store API key in the credentials file (reads stdin; mode 0600, plaintext on disk).
    Login,
    /// Clear the credentials file for the active profile.
    Logout,
}

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// List config keys.
    List {
        #[arg(long)]
        effective: bool,
    },
    /// Get one config value.
    Get { path: String },
    /// Set a config value.
    Set { path: String, value: String },
    /// Remove a config key.
    Unset { path: String },
    /// Print config file path.
    Path,
    /// Profile management.
    Profiles {
        #[command(subcommand)]
        sub: ConfigProfilesCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum ConfigProfilesCmd {
    /// List profiles.
    List,
    /// Show one profile.
    Show { name: String },
    /// Select active profile.
    Use { name: String },
    /// Create a profile.
    Create { name: String },
    /// Delete a profile.
    Delete { name: String },
}

#[derive(Args, Debug)]
pub struct AskArgs {
    pub question: String,
}

#[derive(Args, Debug, Default)]
pub struct CapabilitiesArgs {
    /// Optional command path to show, e.g. `search` or `websets items list`.
    #[arg(value_name = "COMMAND", num_args = 0..)]
    pub command_path: Vec<String>,
}

#[derive(Args, Debug)]
pub struct FetchArgs {
    #[arg(required = true, num_args = 1..)]
    pub urls: Vec<String>,
}

#[derive(Args, Debug)]
pub struct RawArgs {
    /// HTTP method (GET, POST, ...).
    pub method: String,
    /// API path, e.g. /search.
    pub path: String,
    /// Query parameter: key=value. Repeatable.
    #[arg(long)]
    pub query: Vec<String>,
}

/// Space-joined canonical command path for dispatch and error messages.
pub fn command_path(command: &Command) -> String {
    match command {
        Command::Search(_) => "search".to_string(),
        Command::Contents(_) => "contents".to_string(),
        Command::Similar(_) => "similar".to_string(),
        Command::Answer(_) => "answer".to_string(),
        Command::Context(_) => "context".to_string(),
        Command::Monitor { sub } => match sub {
            MonitorCmd::Create(_) => "monitor create".to_string(),
            MonitorCmd::List(_) => "monitor list".to_string(),
            MonitorCmd::Get { .. } => "monitor get".to_string(),
            MonitorCmd::Update { .. } => "monitor update".to_string(),
            MonitorCmd::Delete { .. } => "monitor delete".to_string(),
            MonitorCmd::Trigger { .. } => "monitor trigger".to_string(),
            MonitorCmd::Batch(_) => "monitor batch".to_string(),
            MonitorCmd::Runs { sub } => match sub {
                MonitorRunsCmd::List { .. } => "monitor runs list".to_string(),
                MonitorRunsCmd::Get { .. } => "monitor runs get".to_string(),
            },
        },
        Command::Agent { sub } => match sub {
            AgentCmd::Run(_) => "agent run".to_string(),
            AgentCmd::Runs { sub } => match sub {
                AgentRunsCmd::Create(_) => "agent runs create".to_string(),
                AgentRunsCmd::List(_) => "agent runs list".to_string(),
                AgentRunsCmd::Get { .. } => "agent runs get".to_string(),
                AgentRunsCmd::Events(_) => "agent runs events".to_string(),
                AgentRunsCmd::Cancel { .. } => "agent runs cancel".to_string(),
                AgentRunsCmd::Delete { .. } => "agent runs delete".to_string(),
            },
        },
        Command::Research { sub } => match sub {
            ResearchCmd::Create(_) => "research create".to_string(),
            ResearchCmd::List(_) => "research list".to_string(),
            ResearchCmd::Get { .. } => "research get".to_string(),
        },
        Command::Websets { sub } => websets_command_path(sub),
        Command::Team { sub } => match sub {
            Some(TeamCmd::Info) | None => "team info".to_string(),
        },
        Command::Admin { sub } => match sub {
            AdminCmd::Keys { sub } => match sub {
                AdminKeysCmd::Create(_) => "admin keys create".to_string(),
                AdminKeysCmd::List => "admin keys list".to_string(),
                AdminKeysCmd::Get { .. } => "admin keys get".to_string(),
                AdminKeysCmd::Update { .. } => "admin keys update".to_string(),
                AdminKeysCmd::Delete { .. } => "admin keys delete".to_string(),
                AdminKeysCmd::Usage { .. } => "admin keys usage".to_string(),
            },
        },
        Command::Capabilities(_) => "capabilities".to_string(),
        Command::Schema { sub } => match sub {
            SchemaCmd::List => "schema list".to_string(),
            SchemaCmd::Show { .. } => "schema show".to_string(),
            SchemaCmd::Export(_) => "schema export".to_string(),
            SchemaCmd::ValidateInput(_) => "schema validate-input".to_string(),
            SchemaCmd::Refresh(_) => "schema refresh".to_string(),
        },
        Command::RobotDocs { sub } => match sub {
            RobotDocsCmd::Guide => "robot-docs guide".to_string(),
            RobotDocsCmd::Commands => "robot-docs commands".to_string(),
            RobotDocsCmd::Errors => "robot-docs errors".to_string(),
            RobotDocsCmd::Examples(_) => "robot-docs examples".to_string(),
            RobotDocsCmd::Prompts => "robot-docs prompts".to_string(),
        },
        Command::Doctor(_) => "doctor".to_string(),
        Command::Auth { sub } => match sub {
            AuthCmd::Status => "auth status".to_string(),
            AuthCmd::Test => "auth test".to_string(),
            AuthCmd::Login => "auth login".to_string(),
            AuthCmd::Logout => "auth logout".to_string(),
        },
        Command::Config { sub } => config_command_path(sub),
        Command::Ask(_) => "ask".to_string(),
        Command::Fetch(_) => "fetch".to_string(),
        Command::Raw(_) => "raw".to_string(),
    }
}

pub(crate) fn websets_command_path(sub: &WebsetsCmd) -> String {
    match sub {
        WebsetsCmd::Create(_) => "websets create".to_string(),
        WebsetsCmd::List(_) => "websets list".to_string(),
        WebsetsCmd::Get { .. } => "websets get".to_string(),
        WebsetsCmd::Update { .. } => "websets update".to_string(),
        WebsetsCmd::Delete { .. } => "websets delete".to_string(),
        WebsetsCmd::Cancel { .. } => "websets cancel".to_string(),
        WebsetsCmd::Preview(_) => "websets preview".to_string(),
        WebsetsCmd::Items { sub } => match sub {
            WebsetsItemsCmd::List { .. } => "websets items list".to_string(),
            WebsetsItemsCmd::Get { .. } => "websets items get".to_string(),
            WebsetsItemsCmd::Delete { .. } => "websets items delete".to_string(),
        },
        WebsetsCmd::Searches { sub } => match sub {
            WebsetsSearchesCmd::Create { .. } => "websets searches create".to_string(),
            WebsetsSearchesCmd::Get { .. } => "websets searches get".to_string(),
            WebsetsSearchesCmd::Cancel { .. } => "websets searches cancel".to_string(),
        },
        WebsetsCmd::Enrichments { sub } => match sub {
            WebsetsEnrichmentsCmd::Create { .. } => "websets enrichments create".to_string(),
            WebsetsEnrichmentsCmd::Get { .. } => "websets enrichments get".to_string(),
            WebsetsEnrichmentsCmd::Update { .. } => "websets enrichments update".to_string(),
            WebsetsEnrichmentsCmd::Delete { .. } => "websets enrichments delete".to_string(),
            WebsetsEnrichmentsCmd::Cancel { .. } => "websets enrichments cancel".to_string(),
        },
        WebsetsCmd::Imports { sub } => match sub {
            WebsetsImportsCmd::Create { .. } => "websets imports create".to_string(),
            WebsetsImportsCmd::List(_) => "websets imports list".to_string(),
            WebsetsImportsCmd::Get { .. } => "websets imports get".to_string(),
            WebsetsImportsCmd::Update { .. } => "websets imports update".to_string(),
            WebsetsImportsCmd::Delete { .. } => "websets imports delete".to_string(),
        },
        WebsetsCmd::Monitors { sub } => match sub {
            WebsetsMonitorsCmd::Create(_) => "websets monitors create".to_string(),
            WebsetsMonitorsCmd::List(_) => "websets monitors list".to_string(),
            WebsetsMonitorsCmd::Get { .. } => "websets monitors get".to_string(),
            WebsetsMonitorsCmd::Update { .. } => "websets monitors update".to_string(),
            WebsetsMonitorsCmd::Delete { .. } => "websets monitors delete".to_string(),
            WebsetsMonitorsCmd::Runs { sub } => match sub {
                WebsetsMonitorRunsCmd::List { .. } => "websets monitors runs list".to_string(),
                WebsetsMonitorRunsCmd::Get { .. } => "websets monitors runs get".to_string(),
            },
        },
        WebsetsCmd::Events { sub } => match sub {
            WebsetsEventsCmd::List(_) => "websets events list".to_string(),
            WebsetsEventsCmd::Get { .. } => "websets events get".to_string(),
        },
        WebsetsCmd::Webhooks { sub } => match sub {
            WebsetsWebhooksCmd::Create { .. } => "websets webhooks create".to_string(),
            WebsetsWebhooksCmd::List(_) => "websets webhooks list".to_string(),
            WebsetsWebhooksCmd::Get { .. } => "websets webhooks get".to_string(),
            WebsetsWebhooksCmd::Update { .. } => "websets webhooks update".to_string(),
            WebsetsWebhooksCmd::Delete { .. } => "websets webhooks delete".to_string(),
            WebsetsWebhooksCmd::Attempts { sub } => match sub {
                WebsetsWebhookAttemptsCmd::List { .. } => {
                    "websets webhooks attempts list".to_string()
                }
            },
        },
    }
}

fn config_command_path(sub: &ConfigCmd) -> String {
    match sub {
        ConfigCmd::List { .. } => "config list".to_string(),
        ConfigCmd::Get { .. } => "config get".to_string(),
        ConfigCmd::Set { .. } => "config set".to_string(),
        ConfigCmd::Unset { .. } => "config unset".to_string(),
        ConfigCmd::Path => "config path".to_string(),
        ConfigCmd::Profiles { sub } => match sub {
            ConfigProfilesCmd::List => "config profiles list".to_string(),
            ConfigProfilesCmd::Show { .. } => "config profiles show".to_string(),
            ConfigProfilesCmd::Use { .. } => "config profiles use".to_string(),
            ConfigProfilesCmd::Create { .. } => "config profiles create".to_string(),
            ConfigProfilesCmd::Delete { .. } => "config profiles delete".to_string(),
        },
    }
}
