//! Batch API commands and request validation.
//!
//! Batch item bodies intentionally remain open: `/search` and `/agent/runs` evolve faster than
//! this wrapper, so only the Batch API's own invariants are enforced here.

use crate::cli::{BatchesCmd, BatchesCreateArgs, BatchesListArgs, GlobalArgs};
use crate::error::{CliError, Diag};
use serde_json::Value;
use std::collections::HashSet;

pub(crate) const BETA_TOKEN: &str = "batches-2026-06-06";

pub(crate) fn dispatch(
    sub: &BatchesCmd,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let globals = super::globals_with_required_beta(globals, BETA_TOKEN);
    match sub {
        BatchesCmd::Create(args) => dispatch_create(args, &globals, pretty),
        BatchesCmd::List(args) => dispatch_list(args, &globals, pretty),
        BatchesCmd::Get { id } => dispatch_id_command("get", id, &globals, pretty),
        BatchesCmd::Cancel { id } => dispatch_id_command("cancel", id, &globals, pretty),
        BatchesCmd::Delete { id } => dispatch_id_command("delete", id, &globals, pretty),
    }
}

fn dispatch_create(
    args: &BatchesCreateArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = crate::registry::lookup_by_segments(&["batches", "create"]).expect("batches create");
    super::with_typed_error_context(op, globals, || {
        let requests = args
            .requests
            .as_deref()
            .map(|raw| crate::request::read_json_value_arg(raw, "requests"))
            .transpose()?;
        let requests = requests
            .or(if args.requests.is_none() {
                globals
                    .preset
                    .as_deref()
                    .map(|name| crate::presets::get_preset(name, "batches create"))
                    .transpose()?
                    .and_then(|preset| preset.body.get("requests").cloned())
            } else {
                None
            })
            .unwrap_or_else(|| Value::Array(Vec::new()))
            .to_string();
        let metadata = args
            .metadata
            .as_deref()
            .map(|raw| crate::request::read_json_value_arg(raw, "metadata"))
            .transpose()?
            .map(|value| value.to_string());
        let spec = super::build_typed_spec(
            op,
            &[("requests", Some(requests)), ("metadata", metadata)],
            globals,
        )?;
        super::dispatch_typed_command(spec, globals, pretty)
    })
}

fn dispatch_list(
    args: &BatchesListArgs,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = crate::registry::lookup_by_segments(&["batches", "list"]).expect("batches list");
    super::with_typed_error_context(op, globals, || {
        super::validate_cursor_pagination(&args.pagination)?;
        if args.pagination.limit == Some(0) {
            return Err(CliError::Usage(
                Diag::new("invalid_value", "batches list --limit must be at least 1")
                    .with_details(serde_json::json!({ "field": "limit", "min": 1, "received": 0 }))
                    .with_suggestion("exa-agent batches list --limit 100"),
            ));
        }
        let spec = super::build_typed_spec(op, &[], globals)?;
        let static_query = args
            .status
            .map(|status| vec![("status".to_string(), status.as_str().to_string())])
            .unwrap_or_default();
        let query = super::merge_static_and_pagination_query(&static_query, &args.pagination);
        if args.pagination.all && !(globals.print_request || globals.dry_run) {
            super::dispatch_paginated_typed_command(
                spec,
                globals,
                pretty,
                &args.pagination,
                None,
                &static_query,
            )
        } else {
            super::dispatch_typed_command_routed(spec, globals, pretty, None, &query, false, None)
        }
    })
}

fn dispatch_id_command(
    action: &str,
    id: &str,
    globals: &GlobalArgs,
    pretty: bool,
) -> Result<i32, CliError> {
    let op = crate::registry::lookup_by_segments(&["batches", action])
        .expect("batch id command is in the registry");
    super::with_typed_error_context(op, globals, || {
        let spec = super::build_typed_spec(op, &[], globals)?;
        let path = super::checked_substitute_path(op.api_path, &[("id", id)])?;
        super::dispatch_typed_command_routed(spec, globals, pretty, Some(&path), &[], false, None)
    })
}

pub(crate) fn validate_create_body(body: &Value) -> Result<(), CliError> {
    let Some(requests) = body.get("requests") else {
        return Err(batch_error(
            "missing_required_argument",
            "batches create requires a non-empty requests array via --requests, --body, or --set",
            serde_json::json!({ "field": "requests" }),
        ));
    };
    let Some(requests) = requests.as_array() else {
        return Err(batch_error(
            "invalid_field_type",
            "batch field `requests` must be an array",
            serde_json::json!({
                "field": "requests",
                "expected": "array",
                "received": requests,
            }),
        ));
    };
    if requests.is_empty() {
        return Err(batch_error(
            "invalid_value",
            "batch field `requests` must contain at least one request",
            serde_json::json!({ "field": "requests", "minItems": 1 }),
        ));
    }

    let mut custom_ids = HashSet::with_capacity(requests.len());
    for (index, request) in requests.iter().enumerate() {
        validate_request_item(request, index, &mut custom_ids)?;
    }

    if let Some(metadata) = body.get("metadata") {
        let Some(metadata) = metadata.as_object() else {
            return Err(batch_error(
                "invalid_field_type",
                "batch field `metadata` must be an object whose values are strings",
                serde_json::json!({
                    "field": "metadata",
                    "expected": "object with string values",
                    "received": metadata,
                }),
            ));
        };
        if let Some((key, value)) = metadata.iter().find(|(_, value)| !value.is_string()) {
            return Err(batch_error(
                "invalid_field_type",
                format!("batch metadata value `{key}` must be a string"),
                serde_json::json!({
                    "field": format!("metadata.{key}"),
                    "expected": "string",
                    "received": value,
                }),
            ));
        }
    }

    Ok(())
}

fn validate_request_item(
    request: &Value,
    index: usize,
    custom_ids: &mut HashSet<String>,
) -> Result<(), CliError> {
    let Some(request) = request.as_object() else {
        return Err(batch_item_error(
            "invalid_field_type",
            index,
            "batch request item must be an object",
            "item",
            "object",
            request,
        ));
    };

    let custom_id = required_string(request.get("customId"), index, "customId")?;
    let custom_id_len = custom_id.chars().count();
    if custom_id_len > 64 {
        return Err(batch_error(
            "invalid_value",
            format!("batch request item {index} customId must be at most 64 characters"),
            serde_json::json!({
                "field": format!("requests[{index}].customId"),
                "maxLength": 64,
                "receivedLength": custom_id_len,
            }),
        ));
    }
    if !custom_ids.insert(custom_id.to_string()) {
        return Err(batch_error(
            "invalid_value",
            format!("batch request customId `{custom_id}` is duplicated"),
            serde_json::json!({
                "field": format!("requests[{index}].customId"),
                "customId": custom_id,
                "reason": "duplicate",
            }),
        ));
    }

    let method = required_string(request.get("method"), index, "method")?;
    if method != "POST" {
        return Err(batch_error(
            "invalid_value",
            format!("batch request item {index} method must be `POST`"),
            serde_json::json!({
                "field": format!("requests[{index}].method"),
                "accepted": ["POST"],
                "received": method,
            }),
        ));
    }

    let url = required_string(request.get("url"), index, "url")?;
    if !matches!(url, "/search" | "/agent/runs") {
        return Err(batch_error(
            "invalid_value",
            format!("batch request item {index} url must be `/search` or `/agent/runs`"),
            serde_json::json!({
                "field": format!("requests[{index}].url"),
                "accepted": ["/search", "/agent/runs"],
                "received": url,
            }),
        ));
    }

    let Some(item_body) = request.get("body") else {
        return Err(batch_error(
            "missing_required_argument",
            format!("batch request item {index} is missing required field `body`"),
            serde_json::json!({ "field": format!("requests[{index}].body") }),
        ));
    };
    if !item_body.is_object() {
        return Err(batch_item_error(
            "invalid_field_type",
            index,
            "batch request item body must be an object",
            "body",
            "object",
            item_body,
        ));
    }
    if item_body.get("stream").and_then(Value::as_bool) == Some(true) {
        return Err(batch_error(
            "invalid_flag_combination",
            format!("batch request item {index} cannot set `stream: true`"),
            serde_json::json!({
                "field": format!("requests[{index}].body.stream"),
                "received": true,
            }),
        ));
    }

    Ok(())
}

fn required_string<'a>(
    value: Option<&'a Value>,
    index: usize,
    field: &str,
) -> Result<&'a str, CliError> {
    let Some(value) = value else {
        return Err(batch_error(
            "missing_required_argument",
            format!("batch request item {index} is missing required field `{field}`"),
            serde_json::json!({ "field": format!("requests[{index}].{field}") }),
        ));
    };
    let Some(value) = value.as_str() else {
        return Err(batch_item_error(
            "invalid_field_type",
            index,
            format!("batch request item field `{field}` must be a string"),
            field,
            "string",
            value,
        ));
    };
    if value.is_empty() {
        return Err(batch_error(
            "invalid_value",
            format!("batch request item {index} field `{field}` must not be empty"),
            serde_json::json!({
                "field": format!("requests[{index}].{field}"),
                "minLength": 1,
            }),
        ));
    }
    Ok(value)
}

fn batch_item_error(
    code: &'static str,
    index: usize,
    message: impl Into<String>,
    field: &str,
    expected: &str,
    received: &Value,
) -> CliError {
    batch_error(
        code,
        message,
        serde_json::json!({
            "field": format!("requests[{index}].{field}"),
            "expected": expected,
            "received": received,
        }),
    )
}

fn batch_error(code: &'static str, message: impl Into<String>, details: Value) -> CliError {
    CliError::Usage(
        Diag::new(code, message)
            .with_details(details)
            .with_suggestion("exa-agent batches create --help"),
    )
}
