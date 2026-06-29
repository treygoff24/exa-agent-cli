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
#[derive(Args)]
pub struct GlobalArgs {
    #[arg(long, global = true, value_enum, ignore_case = true)]
    pub format: Option<Format>,
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long, global = true)]
    pub ndjson: bool,
    #[arg(long, global = true)]
    pub raw: bool,
    #[arg(long, global = true, conflicts_with = "compact")]
    pub pretty: bool,
    #[arg(long, global = true)]
    pub compact: bool,
    #[arg(short = 'o', long, global = true)]
    pub output: Option<String>,
    #[arg(long, global = true, default_value_t = 1_048_576)]
    pub max_output_bytes: u64,
    #[arg(long, global = true, env = "EXA_CORRELATION_ID")]
    pub correlation_id: Option<String>,
    #[arg(long, global = true, conflicts_with = "api_key_stdin")]
    pub api_key: Option<String>,
    #[arg(
        long,
        global = true,
        conflicts_with_all = ["api_key", "service_key_stdin"]
    )]
    pub api_key_stdin: bool,
    #[arg(long, global = true, conflicts_with = "service_key_stdin")]
    pub service_key: Option<String>,
    #[arg(
        long,
        global = true,
        conflicts_with_all = ["service_key", "api_key_stdin"]
    )]
    pub service_key_stdin: bool,
    #[arg(long, global = true, env = "EXA_PROFILE")]
    pub profile: Option<String>,
    #[arg(long, global = true)]
    pub base_url: Option<String>,
    #[arg(long = "header", global = true)]
    pub headers: Vec<String>,
    #[arg(long, global = true)]
    pub beta: Option<String>,
    #[arg(long, global = true)]
    pub timeout: Option<String>,
    #[arg(long, global = true)]
    pub connect_timeout: Option<String>,
    #[arg(long, global = true, default_value_t = 2)]
    pub retry: u32,
    #[arg(long, global = true, default_value_t = true)]
    pub retry_after: bool,
    #[arg(long, global = true)]
    pub idempotency_key: Option<String>,
    #[arg(long, global = true)]
    pub input: Option<String>,
    #[arg(long, global = true, value_enum, ignore_case = true)]
    pub input_format: Option<InputFormat>,
    #[arg(long = "set", global = true)]
    pub set: Vec<String>,
    #[arg(long, global = true)]
    pub body: Option<String>,
    #[arg(long, global = true)]
    pub quiet: bool,
    #[arg(long, global = true, conflicts_with = "quiet")]
    pub verbose: bool,
    #[arg(long, global = true)]
    pub trace: Option<String>,
    #[arg(long, global = true)]
    pub no_color: bool,
    #[arg(long, global = true)]
    pub yes: bool,
    #[arg(long, global = true)]
    pub dry_run: bool,
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
                    .map(|h| crate::redaction::redact_header(h))
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
                    .map(|v| crate::redaction::redact_set_value(v))
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
    /// Websets API (/v0/websets).
    Websets {
        #[command(subcommand)]
        sub: WebsetsCmd,
    },
    /// Team quota and concurrency (GET /v0/teams/me).
    Team {
        #[command(subcommand)]
        sub: TeamCmd,
    },
    /// Gated admin surface (EXA_SERVICE_KEY + admin host).
    Admin {
        #[command(subcommand)]
        sub: AdminCmd,
    },
    /// CLI self-description (offline). Alias: describe.
    #[command(visible_alias = "describe")]
    Capabilities,
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
    /// Macro → `answer QUESTION --text`.
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
    #[arg(short = 'n', long, value_parser = clap::value_parser!(u32).range(1..=100))]
    pub num_results: Option<u32>,
    /// Search type.
    #[arg(long, value_enum, ignore_case = true)]
    pub r#type: Option<SearchType>,
    /// Result category.
    #[arg(long, value_enum, ignore_case = true)]
    pub category: Option<SearchCategory>,
}

#[derive(Args, Debug)]
pub struct ContentsArgs {
    /// URLs to fetch.
    #[arg(required_unless_present = "ids", conflicts_with = "ids", num_args = 1..)]
    pub urls: Vec<String>,
    #[arg(long, conflicts_with = "urls", num_args = 1..)]
    pub ids: Vec<String>,
    #[arg(long)]
    pub chunk_size: Option<u32>,
}

#[derive(Args, Debug)]
pub struct SimilarArgs {
    pub url: String,
    #[arg(short = 'n', long, value_parser = clap::value_parser!(u32).range(1..=100))]
    pub num_results: Option<u32>,
}

#[derive(Args, Debug)]
pub struct AnswerArgs {
    pub question: String,
    #[arg(long)]
    pub text: bool,
    #[arg(long)]
    pub stream: bool,
}

#[derive(Args, Debug)]
pub struct ContextArgs {
    pub query: String,
    #[arg(long)]
    pub tokens: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum MonitorCmd {
    /// POST /monitors [create-POST].
    Create(MonitorCreateArgs),
    /// GET /monitors.
    List(PaginationArgs),
    /// GET /monitors/{id}.
    Get { id: String },
    /// PATCH /monitors/{id}.
    Update {
        id: String,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        query: Option<String>,
    },
    /// DELETE /monitors/{id}.
    Delete { id: String },
    /// POST /monitors/{id}/trigger.
    Trigger { id: String },
    /// POST /monitors/batch.
    Batch,
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
    #[arg(long, value_enum, ignore_case = true)]
    pub effort: Option<Effort>,
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
    /// POST /v0/websets [create-POST].
    Create(WebsetsCreateArgs),
    /// GET /v0/websets.
    List(PaginationArgs),
    /// GET /v0/websets/{id}.
    Get { id: String },
    /// POST /v0/websets/{id}.
    Update { id: String },
    /// DELETE /v0/websets/{id}.
    Delete { id: String },
    /// POST /v0/websets/{id}/cancel.
    Cancel { id: String },
    /// POST /v0/websets/preview.
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
    /// Websets monitors (/v0/monitors) — distinct from top-level `monitor`.
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

#[derive(Args, Debug)]
pub struct WebsetsCreateArgs {
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long)]
    pub count: Option<u32>,
}

#[derive(Args, Debug)]
pub struct WebsetsPreviewArgs {
    #[arg(long)]
    pub query: Option<String>,
    #[arg(long)]
    pub criteria: Option<String>,
    #[arg(long)]
    pub count: Option<u32>,
}

#[derive(Subcommand, Debug)]
pub enum WebsetsItemsCmd {
    /// GET /v0/websets/{webset}/items.
    List {
        webset_id: String,
        #[command(flatten)]
        pagination: PaginationArgs,
    },
    /// GET /v0/websets/{webset}/items/{id}.
    Get { webset_id: String, item_id: String },
    /// DELETE /v0/websets/{webset}/items/{id}.
    Delete { webset_id: String, item_id: String },
}

#[derive(Subcommand, Debug)]
pub enum WebsetsSearchesCmd {
    /// POST /v0/websets/{webset}/searches [create-POST].
    Create {
        webset_id: String,
        #[arg(long)]
        query: Option<String>,
        #[arg(long)]
        count: Option<u32>,
    },
    /// GET /v0/websets/{webset}/searches/{id}.
    Get {
        webset_id: String,
        search_id: String,
    },
    /// POST /v0/websets/{webset}/searches/{id}/cancel.
    Cancel {
        webset_id: String,
        search_id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum WebsetsEnrichmentsCmd {
    /// POST /v0/websets/{webset}/enrichments [create-POST].
    Create {
        webset_id: String,
        #[arg(long)]
        description: Option<String>,
    },
    /// GET /v0/websets/{webset}/enrichments/{id}.
    Get {
        webset_id: String,
        enrichment_id: String,
    },
    /// PATCH /v0/websets/{webset}/enrichments/{id}.
    Update {
        webset_id: String,
        enrichment_id: String,
    },
    /// DELETE /v0/websets/{webset}/enrichments/{id}.
    Delete {
        webset_id: String,
        enrichment_id: String,
    },
    /// POST /v0/websets/{webset}/enrichments/{id}/cancel.
    Cancel {
        webset_id: String,
        enrichment_id: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum WebsetsImportsCmd {
    /// POST /v0/imports [create-POST].
    Create {
        #[arg(long)]
        source: Option<String>,
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        csv: Option<String>,
    },
    /// GET /v0/imports.
    List(PaginationArgs),
    /// GET /v0/imports/{id}.
    Get { import_id: String },
    /// PATCH /v0/imports/{id}.
    Update { import_id: String },
    /// DELETE /v0/imports/{id}.
    Delete { import_id: String },
}

#[derive(Subcommand, Debug)]
pub enum WebsetsMonitorsCmd {
    /// POST /v0/monitors [create-POST].
    Create,
    /// GET /v0/monitors.
    List(PaginationArgs),
    /// GET /v0/monitors/{id}.
    Get { monitor_id: String },
    /// PATCH /v0/monitors/{id}.
    Update { monitor_id: String },
    /// DELETE /v0/monitors/{id}.
    Delete { monitor_id: String },
    /// Monitor runs under Websets monitors.
    Runs {
        #[command(subcommand)]
        sub: WebsetsMonitorRunsCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum WebsetsMonitorRunsCmd {
    /// GET /v0/monitors/{monitor}/runs.
    List {
        monitor_id: String,
        #[command(flatten)]
        pagination: PaginationArgs,
    },
    /// GET /v0/monitors/{monitor}/runs/{id}.
    Get { monitor_id: String, run_id: String },
}

#[derive(Subcommand, Debug)]
pub enum WebsetsEventsCmd {
    /// GET /v0/events.
    List(PaginationArgs),
    /// GET /v0/events/{id}.
    Get { event_id: String },
}

#[derive(Subcommand, Debug)]
pub enum WebsetsWebhooksCmd {
    /// POST /v0/webhooks [create-POST].
    Create {
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        event: Option<String>,
        #[arg(long)]
        secret_output: Option<String>,
    },
    /// GET /v0/webhooks.
    List(PaginationArgs),
    /// GET /v0/webhooks/{id}.
    Get { webhook_id: String },
    /// PATCH /v0/webhooks/{id}.
    Update { webhook_id: String },
    /// DELETE /v0/webhooks/{id}.
    Delete { webhook_id: String },
    /// Webhook delivery attempts.
    Attempts {
        #[command(subcommand)]
        sub: WebsetsWebhookAttemptsCmd,
    },
}

#[derive(Subcommand, Debug)]
pub enum WebsetsWebhookAttemptsCmd {
    /// GET /v0/webhooks/{id}/attempts.
    List {
        webhook_id: String,
        #[command(flatten)]
        pagination: PaginationArgs,
    },
}

#[derive(Subcommand, Debug)]
pub enum TeamCmd {
    /// GET /v0/teams/me.
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
        budget_cents: Option<i64>,
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
    pub budget_cents: Option<i64>,
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
    /// Store key in OS keyring (reads stdin).
    Login,
    /// Clear keyring entry for active profile.
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
            MonitorCmd::Batch => "monitor batch".to_string(),
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
            TeamCmd::Info => "team info".to_string(),
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
        Command::Capabilities => "capabilities".to_string(),
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

fn websets_command_path(sub: &WebsetsCmd) -> String {
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
            WebsetsMonitorsCmd::Create => "websets monitors create".to_string(),
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
