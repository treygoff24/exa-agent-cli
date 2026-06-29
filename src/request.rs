//! Request assembly: turn a parsed command + registry metadata into an upstream request
//! body (arch §4). Precedence: registry defaults < named flags < --body < --set.

use std::io::{self, IsTerminal, Read};
use std::path::Path;

use serde_json::{Map, Value};

use crate::error::{CliError, Diag};
use crate::registry::{FieldKind, OperationDef};

const MAX_SET_ARRAY_INDEX: usize = 10_000;

/// A resolved, ready-to-preview request.
#[derive(Debug)]
pub struct RequestSpec {
    pub op: &'static OperationDef,
    pub body: Value,
}

/// Where `--body` content comes from (inline JSON, `@file`, or stdin `-`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodySource<'a> {
    Inline(&'a str),
    File(&'a str),
    Stdin,
}

/// Optional overrides applied after named flags (arch §4).
#[derive(Default)]
pub struct RequestOverrides<'a> {
    pub body: Option<BodySource<'a>>,
    pub sets: &'a [String],
}

/// Classify a raw `--body` flag value into its source kind.
pub fn parse_body_source(raw: &str) -> Result<BodySource<'_>, CliError> {
    if raw == "-" {
        return Ok(BodySource::Stdin);
    }
    if let Some(path) = raw.strip_prefix('@') {
        if path.is_empty() {
            return Err(usage("invalid_value", "`--body @` requires a file path"));
        }
        return Ok(BodySource::File(path));
    }
    Ok(BodySource::Inline(raw))
}

/// Build the merged request body and metadata.
pub fn build_request(
    op: &'static OperationDef,
    flag_values: &[(&str, Option<String>)],
    overrides: RequestOverrides<'_>,
) -> Result<RequestSpec, CliError> {
    let mut body = build_flag_body(op, flag_values)?;

    if let Some(source) = overrides.body {
        let overlay = read_body_source(source)?;
        if !overlay.is_object() {
            return Err(usage(
                "invalid_value",
                "`--body` must be a JSON object when merging with named flags",
            ));
        }
        deep_merge(&mut body, overlay);
    }

    for entry in overrides.sets {
        let (path, value) = parse_set(entry)?;
        set_at_path(&mut body, &path, value)?;
    }

    Ok(RequestSpec { op, body })
}

/// Build the request body from named flag values only (backward-compatible spine entry).
pub fn build_body(
    op: &'static OperationDef,
    flag_values: &[(&str, Option<String>)],
) -> Result<RequestSpec, CliError> {
    build_request(op, flag_values, RequestOverrides::default())
}

/// Encode repeated typed parser values for a `FieldKind::StrArray` registry field.
///
/// `build_request` intentionally accepts stringly flag values because it is shared
/// with generic overlay metadata. Encoding arrays as JSON avoids comma-splitting
/// URLs or ids that happen to contain commas, while remaining local to request
/// construction and not changing any CLI surface.
pub fn encode_str_array(values: &[String]) -> String {
    serde_json::to_string(values).expect("Vec<String> always serializes")
}

fn build_flag_body(
    op: &'static OperationDef,
    flag_values: &[(&str, Option<String>)],
) -> Result<Value, CliError> {
    let mut body = Value::Object(Map::new());
    for field in op.fields {
        let raw = flag_values
            .iter()
            .find(|(flag, _)| *flag == field.flag)
            .and_then(|(_, v)| v.clone());

        match raw {
            Some(s) => {
                set_at_path(&mut body, field.body_path, coerce(field.kind, &s)?)?;
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
    Ok(body)
}

/// Read and parse a `--body` source into JSON.
pub fn read_body_source(source: BodySource<'_>) -> Result<Value, CliError> {
    let inline = matches!(source, BodySource::Inline(_));
    let text = match source {
        BodySource::Inline(raw) => raw.to_string(),
        BodySource::File(path) => read_file(path)?,
        BodySource::Stdin => read_stdin()?,
    };
    serde_json::from_str(&text).map_err(|_| {
        usage(
            "invalid_value",
            if inline {
                "`--body` is not valid JSON"
            } else {
                "body input is not valid JSON"
            },
        )
    })
}

fn read_file(path: &str) -> Result<String, CliError> {
    if !Path::new(path).is_file() {
        return Err(no_input(format!("body file not found: `{path}`")));
    }
    std::fs::read_to_string(path).map_err(|e| no_input(format!("failed to read `{path}`: {e}")))
}

fn read_stdin() -> Result<String, CliError> {
    if io::stdin().is_terminal() {
        return Err(no_input(
            "`--body -` requires piped stdin (refusing to read an interactive TTY)",
        ));
    }
    let mut buf = String::new();
    io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| no_input(format!("failed to read stdin: {e}")))?;
    if buf.is_empty() {
        return Err(no_input("stdin is empty"));
    }
    Ok(buf)
}

/// Parse `--set path=value` into a dotted path and JSON value (strings when not valid JSON).
pub fn parse_set(entry: &str) -> Result<(String, Value), CliError> {
    let Some((path, raw_value)) = entry.split_once('=') else {
        return Err(usage(
            "invalid_value",
            format!("`--set` must be `path=value`, got `{entry}`"),
        ));
    };
    if path.is_empty() {
        return Err(usage(
            "invalid_value",
            format!("`--set` path must not be empty, got `{entry}`"),
        ));
    }
    let value = parse_set_value(raw_value);
    Ok((path.to_string(), value))
}

fn parse_set_value(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.to_string()))
}

/// Deep-merge `overlay` into `base` (object keys recurse; scalars/arrays replace).
pub fn deep_merge(base: &mut Value, overlay: Value) {
    match (base, overlay) {
        (Value::Object(base_map), Value::Object(overlay_map)) => {
            for (key, overlay_val) in overlay_map {
                match base_map.get_mut(&key) {
                    Some(existing) if existing.is_object() && overlay_val.is_object() => {
                        deep_merge(existing, overlay_val);
                    }
                    Some(existing) => {
                        *existing = overlay_val;
                    }
                    None => {
                        base_map.insert(key, overlay_val);
                    }
                }
            }
        }
        (base_slot, overlay_val) => *base_slot = overlay_val,
    }
}

/// Assign `value` at a dotted path, creating intermediate objects/arrays as needed.
pub fn set_at_path(body: &mut Value, path: &str, value: Value) -> Result<(), CliError> {
    if path.is_empty() || path.split('.').any(str::is_empty) {
        return Err(usage(
            "invalid_value",
            format!("`--set` path must not contain empty segments: `{path}`"),
        ));
    }
    let segments: Vec<&str> = path.split('.').collect();
    set_at_segments(body, &segments, value)
}

fn set_at_segments(body: &mut Value, segments: &[&str], value: Value) -> Result<(), CliError> {
    if segments.len() == 1 {
        assign_leaf(body, segments[0], value)?;
        return Ok(());
    }

    let head = segments[0];
    let tail = &segments[1..];

    if is_array_index(head) {
        let idx = parse_array_index(head)?;
        ensure_array_len(body, checked_array_len(idx)?)?;
        let Some(arr) = body.as_array_mut() else {
            return Err(usage("invalid_value", "failed to create array path"));
        };
        set_at_segments(&mut arr[idx], tail, value)?;
    } else {
        if !body.is_object() {
            *body = Value::Object(Map::new());
        }
        let Some(obj) = body.as_object_mut() else {
            return Err(usage("invalid_value", "failed to create object path"));
        };
        let entry = obj
            .entry(head.to_string())
            .or_insert_with(|| default_container_for_segment(tail[0]));
        set_at_segments(entry, tail, value)?;
    }
    Ok(())
}

fn assign_leaf(body: &mut Value, segment: &str, value: Value) -> Result<(), CliError> {
    if is_array_index(segment) {
        let idx = parse_array_index(segment)?;
        ensure_array_len(body, checked_array_len(idx)?)?;
        let Some(arr) = body.as_array_mut() else {
            return Err(usage("invalid_value", "failed to create array path"));
        };
        arr[idx] = value;
    } else if body.is_object() {
        let Some(obj) = body.as_object_mut() else {
            return Err(usage("invalid_value", "failed to create object path"));
        };
        obj.insert(segment.to_string(), value);
    } else {
        let mut obj = Map::new();
        obj.insert(segment.to_string(), value);
        *body = Value::Object(obj);
    }
    Ok(())
}

fn default_container_for_segment(next: &str) -> Value {
    if is_array_index(next) {
        Value::Array(vec![])
    } else {
        Value::Object(Map::new())
    }
}

fn ensure_array_len(body: &mut Value, len: usize) -> Result<(), CliError> {
    if !body.is_array() {
        *body = Value::Array(vec![]);
    }
    let Some(arr) = body.as_array_mut() else {
        return Err(usage("invalid_value", "failed to create array path"));
    };
    while arr.len() < len {
        arr.push(Value::Null);
    }
    Ok(())
}

fn is_array_index(segment: &str) -> bool {
    !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_digit())
}

fn parse_array_index(segment: &str) -> Result<usize, CliError> {
    let idx = segment.parse::<usize>().map_err(|_| {
        usage(
            "invalid_value",
            format!("invalid array index `{segment}` in path"),
        )
    })?;
    if idx > MAX_SET_ARRAY_INDEX {
        return Err(usage(
            "invalid_value",
            format!("array index `{segment}` exceeds --set limit of {MAX_SET_ARRAY_INDEX}"),
        ));
    }
    Ok(idx)
}

fn checked_array_len(idx: usize) -> Result<usize, CliError> {
    idx.checked_add(1)
        .ok_or_else(|| usage("invalid_value", format!("array index `{idx}` is too large")))
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
        FieldKind::StrArray => coerce_str_array(s)?,
        FieldKind::Json => serde_json::from_str(s).map_err(|_| bad("JSON"))?,
    })
}

fn coerce_str_array(s: &str) -> Result<Value, CliError> {
    if s.trim_start().starts_with('[') {
        let parsed: Vec<String> = serde_json::from_str(s).map_err(|_| {
            CliError::Usage(Diag::new(
                "invalid_value",
                format!("`{s}` is not a valid string array"),
            ))
        })?;
        return Ok(Value::Array(
            parsed.into_iter().map(Value::String).collect(),
        ));
    }

    Ok(Value::Array(
        s.split(',')
            .map(|x| Value::String(x.trim().to_string()))
            .collect(),
    ))
}

fn usage(code: &str, message: impl Into<String>) -> CliError {
    CliError::Usage(Diag::new(code, message))
}

fn no_input(message: impl Into<String>) -> CliError {
    CliError::NoInput(Diag::new("no_input", message))
}

impl RequestSpec {
    /// The `--print-request`/`--dry-run` preview (arch §4): method, path, body.
    pub fn preview(&self) -> Value {
        self.preview_with_redactor(|body| body.clone())
    }

    /// Same as [`preview`](Self::preview) but runs `body` through a redaction hook first.
    pub fn preview_with_redactor(&self, redact_body: impl Fn(&Value) -> Value) -> Value {
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
                "body": redact_body(&self.body),
            },
            "dryRun": true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{Method, Namespace, Pagination};

    static NESTED_FIELDS: &[crate::registry::FieldDef] = &[
        crate::registry::FieldDef {
            flag: "query",
            body_path: "query",
            kind: FieldKind::Str,
            required: true,
        },
        crate::registry::FieldDef {
            flag: "text",
            body_path: "contents.text",
            kind: FieldKind::Bool,
            required: false,
        },
    ];

    static NESTED_OP: OperationDef = OperationDef {
        cli_path: &["search"],
        operation_id: "search-test",
        method: Method::Post,
        api_path: "/search",
        read_only: true,
        streaming: false,
        pagination: Pagination::None,
        dangerous: false,
        namespace: Namespace::Api,
        idempotency_sensitive: false,
        deprecated: false,
        source: "test",
        source_version: "0",
        fields: NESTED_FIELDS,
    };

    #[test]
    fn nested_flag_body_path_creates_objects() {
        let spec = build_body(
            &NESTED_OP,
            &[("query", Some("q".into())), ("text", Some("true".into()))],
        )
        .unwrap();
        assert_eq!(spec.body["query"], "q");
        assert_eq!(spec.body["contents"]["text"], true);
    }
}
