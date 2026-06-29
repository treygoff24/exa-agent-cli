//! The clap derive surface (D13). The only place clap types live. Command structs
//! collect flags; logic lives in `request`/`exec`/dispatch.

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
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

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[value(rename_all = "lower")]
pub enum Format {
    Human,
    Json,
    Ndjson,
}

/// Universal flags, inherited by every subcommand (`global = true`).
#[derive(Args, Debug)]
pub struct GlobalArgs {
    #[arg(long, global = true, value_enum)]
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
    #[arg(long, global = true, env = "EXA_API_KEY")]
    pub api_key: Option<String>,
    #[arg(long, global = true, env = "EXA_PROFILE")]
    pub profile: Option<String>,
    #[arg(long, global = true)]
    pub base_url: Option<String>,
    #[arg(long = "header", global = true)]
    pub headers: Vec<String>,
    #[arg(long, global = true, default_value_t = 2)]
    pub retry: u32,
    #[arg(long, global = true)]
    pub no_color: bool,
    #[arg(long, global = true)]
    pub yes: bool,
    #[arg(long, global = true)]
    pub dry_run: bool,
    #[arg(long, global = true)]
    pub print_request: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Run a search (POST /search).
    Search(SearchArgs),
    /// CLI self-description (offline). Alias: describe.
    #[command(visible_alias = "describe")]
    Capabilities,
    /// Embedded API/CLI schema (offline).
    Schema {
        #[command(subcommand)]
        sub: SchemaCmd,
    },
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
    #[arg(long)]
    pub r#type: Option<String>,
    /// Result category.
    #[arg(long)]
    pub category: Option<String>,
}

#[derive(Subcommand, Debug)]
pub enum SchemaCmd {
    /// List every known operation.
    List,
}

#[derive(Args, Debug)]
pub struct RawArgs {
    /// HTTP method (GET, POST, ...).
    pub method: String,
    /// API path, e.g. /search.
    pub path: String,
    /// Request body: inline JSON, @file, or - for stdin.
    #[arg(long)]
    pub body: Option<String>,
}
