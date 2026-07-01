//! Request assembly: turn a parsed command + registry metadata into an upstream request
//! body (arch §4). Precedence: fields/co_fields/item_template < body_builder < --body < --set.

use std::io::{self, IsTerminal, Read};
use std::path::Path;

use serde_json::{Map, Value};

use crate::error::{CliError, Diag};
use crate::registry::{BuilderId, ConstValue, FieldDef, FieldKind, OperationDef};

const MAX_SET_ARRAY_INDEX: usize = 10_000;
// `set_at_segments` recurses once per dotted segment; cap depth so a hostile
// `--set a.a.a.…=1` returns a clean error instead of overflowing the stack.
// Real request bodies nest only a few levels deep.
const MAX_SET_PATH_DEPTH: usize = 64;

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

    if let Some(id) = op.body_builder {
        deep_merge(&mut body, run_builder(id, flag_values));
    }

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

/// Dispatch a pure body-builder by id.
///
/// Purity is a convention + review rule + this test hook, NOT statically enforced:
/// a body_builder must be a pure fn of its flags — no I/O, auth, transport, or redaction.
pub fn run_builder(id: BuilderId, flags: &[(&str, Option<String>)]) -> Value {
    match id {
        BuilderId::Sentinel => {
            let mut fields = Map::new();
            for (flag, value) in flags {
                if let Some(value) = value {
                    fields.insert((*flag).to_string(), Value::String(value.clone()));
                }
            }
            let mut body = Map::new();
            body.insert("sentinel".to_string(), Value::Object(fields));
            Value::Object(body)
        }
    }
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
                set_at_path(&mut body, field.body_path, coerce_field(field, &s)?)?;
                // co_fields are written in field iteration order; later fields win on collisions.
                for (path, value) in field.co_fields {
                    set_at_path(&mut body, path, const_value_to_json(*value))?;
                }
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

fn const_value_to_json(value: ConstValue) -> Value {
    match value {
        ConstValue::Str(s) => Value::String(s.to_string()),
        ConstValue::Bool(b) => Value::Bool(b),
        ConstValue::Int(i) => Value::from(i),
    }
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

/// Read a JSON-valued flag from inline JSON or `@file`.
pub fn read_json_value_arg(raw: &str, flag: &str) -> Result<Value, CliError> {
    let (text, inline) = if let Some(path) = raw.strip_prefix('@') {
        if path.is_empty() {
            return Err(usage(
                "invalid_value",
                format!("`--{flag} @` requires a file path"),
            ));
        }
        (read_named_file(path, flag)?, false)
    } else {
        (raw.to_string(), true)
    };
    serde_json::from_str(&text).map_err(|_| {
        usage(
            "invalid_value",
            if inline {
                format!("`--{flag}` is not valid JSON")
            } else {
                format!("`--{flag}` file is not valid JSON")
            },
        )
    })
}

fn read_file(path: &str) -> Result<String, CliError> {
    read_named_file(path, "body")
}

fn read_named_file(path: &str, label: &str) -> Result<String, CliError> {
    if !Path::new(path).is_file() {
        return Err(no_input(format!("{label} file not found: `{path}`")));
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
    if segments.len() > MAX_SET_PATH_DEPTH {
        return Err(usage(
            "invalid_value",
            format!(
                "`--set` path is {} segments deep, exceeding the limit of {MAX_SET_PATH_DEPTH}: `{path}`",
                segments.len()
            ),
        ));
    }
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

fn coerce_field(field: &FieldDef, s: &str) -> Result<Value, CliError> {
    if field.kind == FieldKind::StrArray {
        return coerce_str_array(s, field.item_template);
    }
    coerce(field.kind, s)
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
        FieldKind::StrArray => coerce_str_array(s, None)?,
        FieldKind::Json => serde_json::from_str(s).map_err(|_| bad("JSON"))?,
    })
}

fn coerce_str_array(s: &str, item_template: Option<&str>) -> Result<Value, CliError> {
    let values = parse_str_array(s)?;
    Ok(Value::Array(match item_template {
        Some(key) => values
            .into_iter()
            .map(|value| {
                let mut item = Map::new();
                item.insert(key.to_string(), Value::String(value));
                Value::Object(item)
            })
            .collect(),
        None => values.into_iter().map(Value::String).collect(),
    }))
}

fn parse_str_array(s: &str) -> Result<Vec<String>, CliError> {
    if s.trim_start().starts_with('[') {
        return serde_json::from_str(s).map_err(|_| {
            CliError::Usage(Diag::new(
                "invalid_value",
                format!("`{s}` is not a valid string array"),
            ))
        });
    }

    Ok(s.split(',')
        .map(|x| x.trim().to_string())
        .collect::<Vec<_>>())
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
    use crate::registry::{BuilderId, ConstValue, FieldDef, Method, Namespace, Pagination};

    static NESTED_FIELDS: &[crate::registry::FieldDef] = &[
        crate::registry::FieldDef {
            flag: "query",
            body_path: "query",
            kind: FieldKind::Str,
            required: true,
            co_fields: &[],
            item_template: None,
            enum_values: &[],
            range: None,
        },
        crate::registry::FieldDef {
            flag: "text",
            body_path: "contents.text",
            kind: FieldKind::Bool,
            required: false,
            co_fields: &[],
            item_template: None,
            enum_values: &[],
            range: None,
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
        capabilities: &[],
        body_builder: None,
        validators: &[],
        mixed_status_exit: false,
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

    static CO_FIELD_SIBLINGS: &[(&str, ConstValue)] = &[
        ("trigger.type", ConstValue::Str("interval")),
        ("trigger.active", ConstValue::Bool(true)),
    ];

    static CO_FIELDS: &[FieldDef] = &[FieldDef {
        flag: "schedule",
        body_path: "trigger.schedule",
        kind: FieldKind::Str,
        required: false,
        co_fields: CO_FIELD_SIBLINGS,
        item_template: None,
        enum_values: &[],
        range: None,
    }];

    static CO_OP: OperationDef = OperationDef {
        cli_path: &["synthetic", "co-fields"],
        operation_id: "synthetic-co-fields",
        method: Method::Post,
        api_path: "/synthetic/co-fields",
        read_only: false,
        streaming: false,
        pagination: Pagination::None,
        dangerous: false,
        namespace: Namespace::Api,
        idempotency_sensitive: false,
        deprecated: false,
        source: "test",
        source_version: "0",
        fields: CO_FIELDS,
        capabilities: &[],
        body_builder: None,
        validators: &[],
        mixed_status_exit: false,
    };

    #[test]
    fn co_fields_apply_only_when_source_field_is_present() {
        let flags = &[("schedule", Some("daily".into()))];
        let first = build_body(&CO_OP, flags).unwrap().body;
        let second = build_body(&CO_OP, flags).unwrap().body;

        assert_eq!(first, second);
        assert_eq!(
            first,
            serde_json::json!({
                "trigger": {
                    "schedule": "daily",
                    "type": "interval",
                    "active": true
                }
            })
        );
        assert_eq!(build_body(&CO_OP, &[]).unwrap().body, serde_json::json!({}));
    }

    static ITEM_TEMPLATE_FIELDS: &[FieldDef] = &[FieldDef {
        flag: "descriptions",
        body_path: "items",
        kind: FieldKind::StrArray,
        required: false,
        co_fields: &[],
        item_template: Some("description"),
        enum_values: &[],
        range: None,
    }];

    static PLAIN_ARRAY_FIELDS: &[FieldDef] = &[FieldDef {
        flag: "descriptions",
        body_path: "items",
        kind: FieldKind::StrArray,
        required: false,
        co_fields: &[],
        item_template: None,
        enum_values: &[],
        range: None,
    }];

    static ITEM_TEMPLATE_OP: OperationDef = OperationDef {
        cli_path: &["synthetic", "item-template"],
        operation_id: "synthetic-item-template",
        method: Method::Post,
        api_path: "/synthetic/item-template",
        read_only: false,
        streaming: false,
        pagination: Pagination::None,
        dangerous: false,
        namespace: Namespace::Api,
        idempotency_sensitive: false,
        deprecated: false,
        source: "test",
        source_version: "0",
        fields: ITEM_TEMPLATE_FIELDS,
        capabilities: &[],
        body_builder: None,
        validators: &[],
        mixed_status_exit: false,
    };

    static PLAIN_ARRAY_OP: OperationDef = OperationDef {
        fields: PLAIN_ARRAY_FIELDS,
        operation_id: "synthetic-plain-array",
        cli_path: &["synthetic", "plain-array"],
        api_path: "/synthetic/plain-array",
        ..ITEM_TEMPLATE_OP
    };

    #[test]
    fn item_template_wraps_str_array_items_only_when_configured() {
        let values = vec!["a".to_string(), "b".to_string()];
        let body = build_body(
            &ITEM_TEMPLATE_OP,
            &[("descriptions", Some(encode_str_array(&values)))],
        )
        .unwrap()
        .body;
        assert_eq!(
            body,
            serde_json::json!({
                "items": [
                    { "description": "a" },
                    { "description": "b" }
                ]
            })
        );

        let empty = build_body(
            &ITEM_TEMPLATE_OP,
            &[("descriptions", Some(encode_str_array(&[])))],
        )
        .unwrap()
        .body;
        assert_eq!(empty, serde_json::json!({ "items": [] }));
        assert_eq!(
            build_body(&ITEM_TEMPLATE_OP, &[]).unwrap().body,
            serde_json::json!({})
        );

        let plain = build_body(
            &PLAIN_ARRAY_OP,
            &[("descriptions", Some(encode_str_array(&values)))],
        )
        .unwrap()
        .body;
        assert_eq!(plain, serde_json::json!({ "items": ["a", "b"] }));
    }

    static BUILDER_FIELDS: &[FieldDef] = &[FieldDef {
        flag: "modeled",
        body_path: "layers.field",
        kind: FieldKind::Str,
        required: false,
        co_fields: &[],
        item_template: None,
        enum_values: &[],
        range: None,
    }];

    static BUILDER_OP: OperationDef = OperationDef {
        cli_path: &["synthetic", "builder"],
        operation_id: "synthetic-builder",
        method: Method::Post,
        api_path: "/synthetic/builder",
        read_only: false,
        streaming: false,
        pagination: Pagination::None,
        dangerous: false,
        namespace: Namespace::Api,
        idempotency_sensitive: false,
        deprecated: false,
        source: "test",
        source_version: "0",
        fields: BUILDER_FIELDS,
        capabilities: &[],
        body_builder: Some(BuilderId::Sentinel),
        validators: &[],
        mixed_status_exit: false,
    };

    #[test]
    fn sentinel_builder_layers_between_fields_body_and_set() {
        let flags = &[
            ("modeled", Some("field-layer".into())),
            ("builder-flag", Some("builder-layer".into())),
            ("ignored", None),
        ];
        let sets = ["layers.set=set-layer".into()];
        let first = build_request(
            &BUILDER_OP,
            flags,
            RequestOverrides {
                body: Some(BodySource::Inline(r#"{"layers":{"body":"body-layer"}}"#)),
                sets: &sets,
            },
        )
        .unwrap()
        .body;
        let second = build_request(
            &BUILDER_OP,
            flags,
            RequestOverrides {
                body: Some(BodySource::Inline(r#"{"layers":{"body":"body-layer"}}"#)),
                sets: &sets,
            },
        )
        .unwrap()
        .body;

        assert_eq!(first, second);
        assert_eq!(
            first,
            serde_json::json!({
                "layers": {
                    "field": "field-layer",
                    "body": "body-layer",
                    "set": "set-layer"
                },
                "sentinel": {
                    "modeled": "field-layer",
                    "builder-flag": "builder-layer"
                }
            })
        );
    }

    #[test]
    fn sentinel_builder_dispatch_is_total_for_canary_variant() {
        // Budget canary: target <= ~16 real builders + Sentinel, hard max 20.
        assert_eq!(
            run_builder(BuilderId::Sentinel, &[]),
            serde_json::json!({ "sentinel": {} })
        );
    }
}
