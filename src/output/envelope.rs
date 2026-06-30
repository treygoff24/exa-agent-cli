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
    pub operation: ErrorOperation,
    pub request: ErrorRequest,
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

#[derive(Serialize)]
pub struct ErrorOperation {
    pub method: Option<String>,
    pub path: Option<String>,
}

#[derive(Serialize)]
pub struct ErrorRequest {
    #[serde(rename = "requestId")]
    pub request_id: Option<String>,
    #[serde(rename = "upstreamRequestId")]
    pub upstream_request_id: Option<String>,
    #[serde(rename = "correlationId")]
    pub correlation_id: Option<String>,
    pub redacted: bool,
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
            operation: ErrorOperation {
                method: None,
                path: None,
            },
            request: ErrorRequest {
                request_id: None,
                upstream_request_id: None,
                correlation_id: None,
                redacted: true,
            },
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    pub fn with_context(
        mut self,
        method: impl Into<String>,
        path: impl Into<String>,
        request_id: impl Into<String>,
        correlation_id: Option<String>,
    ) -> Self {
        self.operation.method = Some(redaction::scrub_text(&method.into()));
        self.operation.path = Some(redaction::scrub_text(&path.into()));
        self.request.request_id = Some(request_id.into());
        self.request.correlation_id = correlation_id.map(|value| redaction::scrub_text(&value));
        self
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

    let error_codes = error_codes_json();

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

pub fn error_codes_json() -> serde_json::Map<String, serde_json::Value> {
    error_code_specs()
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
        .collect()
}

/// `exa.cli.response.v1` success envelope (contracts §4).
pub struct ResponseEnvelopeArgs<'a> {
    pub command: &'a str,
    pub method: &'a str,
    pub path: &'a str,
    pub operation: Option<&'a OperationDef>,
    pub request_id: &'a str,
    pub profile: &'a str,
    pub correlation_id: Option<&'a str>,
    pub data: serde_json::Value,
    pub count: Option<u64>,
    pub data_hash: Option<String>,
    pub retries: u32,
    pub warnings: &'a [serde_json::Value],
}

pub fn response_envelope(args: ResponseEnvelopeArgs<'_>) -> serde_json::Value {
    let operation_id = args.operation.map(|op| op.operation_id);
    let source = args.operation.map_or(registry::SPEC_URL, |op| op.source);
    let source_version = args
        .operation
        .map_or(registry::SPEC_VERSION, |op| op.source_version);
    serde_json::json!({
        "schema": "exa.cli.response.v1",
        "ok": true,
        "command": args.command,
        "operation": {
            "method": args.method,
            "path": args.path,
            "operationId": operation_id,
            "source": source,
            "sourceVersion": source_version,
        },
        "request": {
            "requestId": args.request_id,
            "upstreamRequestId": null,
            "correlationId": args.correlation_id,
            "profile": args.profile,
            "redacted": true,
        },
        "count": args.count,
        "data": args.data,
        "dataHash": args.data_hash,
        "pagination": null,
        "costDollars": { "total": 0.0 },
        "nextActions": [],
        "warnings": args.warnings,
        "diagnostics": { "durationMs": 0, "retries": args.retries },
        "dataTruncated": false,
        "dataPath": null,
        "bytes": null,
    })
}

pub struct EventEnvelopeArgs<'a> {
    pub event_type: &'a str,
    pub command: &'a str,
    pub seq: u64,
    pub event_id: Option<&'a str>,
    pub correlation_id: Option<&'a str>,
    pub event: serde_json::Value,
}

/// One NDJSON streaming record (`exa.cli.event.v1`, contracts §8).
pub fn event_envelope(args: EventEnvelopeArgs<'_>) -> serde_json::Value {
    serde_json::json!({
        "schema": "exa.cli.event.v1",
        "type": args.event_type,
        "command": args.command,
        "seq": args.seq,
        "eventId": args.event_id,
        "timestamp": stream_event_timestamp(),
        "correlationId": args.correlation_id,
        "event": args.event,
    })
}

fn stream_event_timestamp() -> String {
    let epoch = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    unix_epoch_to_rfc3339(epoch)
}

fn unix_epoch_to_rfc3339(epoch: u64) -> String {
    let days = (epoch / 86_400).min(i64::MAX as u64) as i64;
    let seconds_of_day = epoch % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

// Howard Hinnant's civil-from-days algorithm, with day 0 = 1970-01-01.
fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y, m as u32, d as u32)
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

#[cfg(test)]
mod tests {
    use super::unix_epoch_to_rfc3339;

    #[test]
    fn unix_epoch_timestamp_format_is_reproducible() {
        assert_eq!(unix_epoch_to_rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(unix_epoch_to_rfc3339(1_700_000_000), "2023-11-14T22:13:20Z");
    }
}
