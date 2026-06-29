//! Request assembly: turn a parsed command + registry metadata into an upstream request
//! body (arch §4). Precedence: registry defaults < named flags < --body < --set.
//!
//! Phase-1 scope: enough to drive the offline `search --print-request --dry-run` typed-spine
//! proof. The full merge (--body deep-merge, --set dotted paths) lands with transport.

use serde_json::{Map, Value};

use crate::error::{CliError, Diag};
use crate::registry::{FieldKind, OperationDef};

/// A resolved, ready-to-preview request.
pub struct RequestSpec {
    pub op: &'static OperationDef,
    pub body: Value,
}

/// Build the request body from named flag values keyed by FieldDef.flag.
/// `flag_values` maps a registry flag name (e.g. "num-results") to its string value.
pub fn build_body(
    op: &'static OperationDef,
    flag_values: &[(&str, Option<String>)],
) -> Result<RequestSpec, CliError> {
    let mut body = Map::new();
    for field in op.fields {
        let raw = flag_values
            .iter()
            .find(|(flag, _)| *flag == field.flag)
            .and_then(|(_, v)| v.clone());

        match raw {
            Some(s) => {
                body.insert(field.body_path.to_string(), coerce(field.kind, &s)?);
            }
            None if field.required => {
                return Err(CliError::Usage(
                    Diag::new(
                        "missing_required_argument",
                        format!("missing required `--{}` for `{}`", field.flag, op.command()),
                    )
                    .with_suggestion(format!("exa-agent {} --help", op.command())),
                ));
            }
            None => {}
        }
    }
    Ok(RequestSpec {
        op,
        body: Value::Object(body),
    })
}

fn coerce(kind: FieldKind, s: &str) -> Result<Value, CliError> {
    let bad = |what: &str| {
        CliError::Usage(Diag::new(
            "invalid_value",
            format!("`{s}` is not a valid {what}"),
        ))
    };
    Ok(match kind {
        FieldKind::Str => Value::String(s.to_string()),
        FieldKind::Int => Value::from(s.parse::<i64>().map_err(|_| bad("integer"))?),
        FieldKind::Num => Value::from(s.parse::<f64>().map_err(|_| bad("number"))?),
        FieldKind::Bool => Value::from(matches!(
            s.to_ascii_lowercase().as_str(),
            "true" | "1" | "yes" | "on"
        )),
        FieldKind::StrArray => Value::Array(
            s.split(',')
                .map(|x| Value::String(x.trim().to_string()))
                .collect(),
        ),
        FieldKind::Json => serde_json::from_str(s).map_err(|_| bad("JSON"))?,
    })
}

impl RequestSpec {
    /// The `--print-request`/`--dry-run` preview (arch §4): method, path, body. Redaction of
    /// auth headers happens at the transport boundary; this offline preview carries no secrets.
    pub fn preview(&self) -> Value {
        serde_json::json!({
            "schema": "exa.cli.request_preview.v1",
            "ok": true,
            "command": self.op.command(),
            "operation": {
                "operationId": self.op.operation_id,
                "method": self.op.method.as_str(),
                "apiPath": self.op.api_path,
                "source": self.op.source,
                "sourceVersion": self.op.source_version,
            },
            "request": {
                "method": self.op.method.as_str(),
                "path": self.op.api_path,
                "body": self.body,
            },
            "dryRun": true,
        })
    }
}
