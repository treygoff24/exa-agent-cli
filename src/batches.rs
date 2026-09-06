//! Batch API request validation.
//!
//! Batch item bodies intentionally remain open: `/search` and `/agent/runs` evolve faster than
//! this wrapper, so only the Batch API's own invariants are enforced here.

use crate::error::{CliError, Diag};
use serde_json::Value;
use std::collections::HashSet;

pub(crate) const BETA_TOKEN: &str = "batches-2026-06-06";

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
