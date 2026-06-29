//! One error enum is the single source of both the exit code and the error envelope
//! (arch §10 / contracts §6). Each variant maps to exactly one category in the §6 dictionary.

use std::collections::BTreeMap;

/// Structured diagnostic carried by every `CliError` (contracts §5).
#[derive(Debug, Clone, Default)]
pub struct Diag {
    /// Stable machine string from the published §5.1 dictionary (never free-form).
    pub code: String,
    pub message: String,
    pub suggested_command: Option<String>,
    pub http_status: Option<u16>,
    pub retryable: bool,
}

impl Diag {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Diag {
            code: code.to_string(),
            message: message.into(),
            ..Default::default()
        }
    }

    pub fn with_suggestion(mut self, cmd: impl Into<String>) -> Self {
        self.suggested_command = Some(cmd.into());
        self
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("{0}")]
    Usage(Diag), // 1
    #[error("{0}")]
    Auth(Diag), // 2
    #[error("{0}")]
    Config(Diag), // 3
    #[error("{0}")]
    Network(Diag), // 4
    #[error("{0}")]
    Upstream(Diag), // 5
    #[error("{0}")]
    RateLimit(Diag), // 6
    #[error("{0}")]
    NotFound(Diag), // 7
    #[error("{0}")]
    Conflict(Diag), // 8
    #[error("{0}")]
    Safety(Diag), // 9
    #[error("{0}")]
    Partial(Diag), // 10
    #[error("{0}")]
    NoInput(Diag), // 11
    #[error("{0}")]
    Interrupted(Diag), // 12
}

impl std::fmt::Display for Diag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl CliError {
    pub fn category(&self) -> u8 {
        match self {
            CliError::Usage(_) => 1,
            CliError::Auth(_) => 2,
            CliError::Config(_) => 3,
            CliError::Network(_) => 4,
            CliError::Upstream(_) => 5,
            CliError::RateLimit(_) => 6,
            CliError::NotFound(_) => 7,
            CliError::Conflict(_) => 8,
            CliError::Safety(_) => 9,
            CliError::Partial(_) => 10,
            CliError::NoInput(_) => 11,
            CliError::Interrupted(_) => 12,
        }
    }

    pub fn category_name(&self) -> &'static str {
        // EXIT_CODES is indexed by code: [0]=ok, [1]=usage, ... so the category IS the index.
        EXIT_CODES[self.category() as usize].1
    }

    pub fn diag(&self) -> &Diag {
        match self {
            CliError::Usage(d)
            | CliError::Auth(d)
            | CliError::Config(d)
            | CliError::Network(d)
            | CliError::Upstream(d)
            | CliError::RateLimit(d)
            | CliError::NotFound(d)
            | CliError::Conflict(d)
            | CliError::Safety(d)
            | CliError::Partial(d)
            | CliError::NoInput(d)
            | CliError::Interrupted(d) => d,
        }
    }
}

/// The exit-code dictionary (contracts §6), surfaced in `capabilities.exitCodes`.
pub const EXIT_CODES: &[(u8, &str, &str)] = &[
    (0, "ok", "success"),
    (
        1,
        "usage",
        "bad invocation, parse error, or local validation failure",
    ),
    (2, "auth", "missing, invalid, or wrong-scope credential"),
    (3, "config", "malformed config or unknown profile"),
    (4, "network", "connection/timeout failure reaching Exa"),
    (
        5,
        "upstream",
        "Exa returned a non-2xx the CLI maps to a server error",
    ),
    (6, "rate_limit", "429; budget or concurrency exhausted"),
    (7, "not_found", "resource does not exist"),
    (8, "conflict", "duplicate/externalId conflict"),
    (9, "safety", "destructive op refused without confirmation"),
    (
        10,
        "partial",
        "batch partially succeeded (per-item statuses)",
    ),
    (
        11,
        "no_input",
        "required stdin/@file input absent or a TTY would block",
    ),
    (12, "interrupted", "SIGINT / stream interrupted"),
];

/// The error-code vocabulary (contracts §5.1), surfaced in `capabilities.errorCodes`.
/// Every `error.code` the binary emits MUST be a member of this map (static test, Phase 1).
pub fn error_code_dictionary() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        ("unknown_flag", "an unrecognized flag was passed"),
        (
            "unknown_subcommand",
            "an unrecognized subcommand was passed",
        ),
        (
            "invalid_value",
            "a flag value failed validation or enum membership",
        ),
        (
            "missing_required_argument",
            "a required argument was omitted",
        ),
        (
            "placeholder_argument",
            "a literal placeholder (<id>, $VAR, YOUR_*) was passed as a value",
        ),
        (
            "not_authenticated",
            "no credential resolved from any ladder rung",
        ),
        (
            "reauth_required",
            "a credential was sent but upstream rejected it",
        ),
        (
            "key_scope_mismatch",
            "an api key was used where a service key is required (or vice versa)",
        ),
        (
            "no_input",
            "required stdin/@file input absent or a TTY would block",
        ),
        (
            "not_implemented",
            "the command is recognized but not yet wired in this build",
        ),
    ])
}
