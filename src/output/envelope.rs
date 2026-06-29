//! Envelope serializers. Field order is fixed by struct declaration order (contracts §4/§5/§13).

use serde::Serialize;

use crate::error::{error_code_specs, CliError, EXIT_CODES};
use crate::redaction;
use crate::registry::{self, OperationDef, Pagination};

/// `exa.cli.error.v1` (contracts §5). Rendered to stderr; stdout stays empty on error.
#[derive(Serialize)]
pub struct ErrorEnvelope {
    pub schema: &'static str,
    pub ok: bool,
    pub error: ErrorBody,
}

#[derive(Serialize)]
pub struct ErrorBody {
    pub code: String,
    pub category: &'static str,
    #[serde(rename = "exitCode")]
    pub exit_code: u8,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
    #[serde(rename = "httpStatus", skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(rename = "suggestedCommand", skip_serializing_if = "Option::is_none")]
    pub suggested_command: Option<String>,
}

impl ErrorEnvelope {
    pub fn from_error(err: &CliError) -> Self {
        let d = err.diag();
        let mut details = d.details.as_deref().cloned();
        if let Some(value) = &mut details {
            redaction::scrub_json_value(value);
        }
        ErrorEnvelope {
            schema: "exa.cli.error.v1",
            ok: false,
            error: ErrorBody {
                code: d.code.clone(),
                category: err.category_name(),
                exit_code: err.category(),
                message: redaction::scrub_text(&d.message),
                retryable: d.retryable,
                details,
                http_status: d.http_status,
                suggested_command: d.suggested_command.as_deref().map(redaction::scrub_text),
            },
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }
}

/// Build the `capabilities --json` payload (contracts §13) entirely from the registry.
/// Proves the registry spine + the per-command blast-radius triad (D27).
pub fn capabilities() -> serde_json::Value {
    let commands: Vec<serde_json::Value> = registry::REGISTRY.iter().map(command_entry).collect();

    let exit_codes: serde_json::Map<String, serde_json::Value> = EXIT_CODES
        .iter()
        .map(|(code, name, desc)| {
            (
                code.to_string(),
                serde_json::json!({ "name": name, "description": desc }),
            )
        })
        .collect();

    let error_codes: serde_json::Map<String, serde_json::Value> = error_code_specs()
        .into_iter()
        .map(|(code, spec)| {
            (
                code.to_string(),
                serde_json::json!({
                    "category": spec.category,
                    "exit": spec.exit,
                    "retryable": spec.retryable,
                    "description": spec.description,
                }),
            )
        })
        .collect();

    serde_json::json!({
        "schema": "exa.cli.capabilities.v1",
        "ok": true,
        "binary": "exa-agent",
        "build": {
            "version": env!("CARGO_PKG_VERSION"),
            "gitSha": registry::GIT_SHA,
            "buildDate": registry::BUILD_DATE,
            "target": registry::TARGET,
        },
        "spec": {
            "title": registry::SPEC_TITLE,
            "version": registry::SPEC_VERSION,
            "url": registry::SPEC_URL,
            "embeddedSpecSha256": registry::EMBEDDED_SPEC_SHA256,
            "adminTitle": registry::ADMIN_SPEC_TITLE,
            "adminVersion": registry::ADMIN_SPEC_VERSION,
        },
        "supportsRawBody": true,
        "supportsPrintRequest": true,
        "commandCount": commands.len(),
        "commands": commands,
        "exitCodes": exit_codes,
        "errorCodes": error_codes,
        "doctor": {
            "exitCodes": {
                "0": "healthy",
                "1": "findings",
                "4": "refused-unsafe",
            },
            "detectors": [
                "config.parse",
                "key.present",
                "service-key.scope",
                "base-url",
                "spec.hash",
                "binary.version",
                "tty.discipline",
                "auth.online",
                "connectivity",
            ],
        },
    })
}

fn command_entry(op: &OperationDef) -> serde_json::Value {
    let pagination = match op.pagination {
        Pagination::None => serde_json::Value::Null,
        Pagination::Cursor(field) => serde_json::json!({ "style": "cursor", "cursorField": field }),
    };
    serde_json::json!({
        "path": op.command(),
        "operationId": op.operation_id,
        "method": op.method.as_str(),
        "apiPath": op.api_path,
        "namespace": match op.namespace {
            registry::Namespace::Api => "api",
            registry::Namespace::Service => "service",
        },
        "readOnly": op.read_only,
        "destructive": op.destructive(),
        "idempotencySensitive": op.idempotency_sensitive,
        "requiresConfirm": op.dangerous,
        "streaming": op.streaming,
        "deprecated": op.deprecated,
        "pagination": pagination,
        "source": op.source,
        "sourceVersion": op.source_version,
    })
}
