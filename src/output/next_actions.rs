use crate::cli::GlobalArgs;
use crate::error::{CliError, Diag};
use crate::registry;
use crate::transport::parse_user_headers;
use crate::{has_more, next_cursor, redaction, shell_quote, transport};

pub(crate) fn append_operation_next_actions(
    envelope: &mut serde_json::Value,
    operation: &registry::OperationDef,
    webset_id: Option<&str>,
    globals: &GlobalArgs,
) -> Result<(), CliError> {
    let Some(data) = envelope.get("data") else {
        return Ok(());
    };
    // Follow-ups need only these handles, not a second copy of potentially large page text.
    let id = data
        .get("id")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let upload_url = data
        .get("uploadUrl")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    let results_url = data
        .get("resultsUrl")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    match operation.command().as_str() {
        "batches create" => {
            if let Some(id) = id.as_deref() {
                push_resource_next_action(
                    envelope,
                    "Poll batch status and refresh its short-lived results URL",
                    "batches get",
                    &[id.to_string()],
                    globals,
                );
            }
        }
        "batches get" => {
            if let Some(url) = results_url
                .as_deref()
                .filter(|url| url.starts_with("https://"))
            {
                push_next_action(envelope,
                    "Download JSONL from this short-lived bearer URL; run batches get again after it expires. Redirect stdout to a new file to keep all rows.",
                    format!("curl --fail --location --proto '=https' -- {}", shell_quote(url)),
                );
            }
        }
        "agent runs create"
        | "websets create"
        | "monitor create"
        | "websets monitors create"
        | "websets webhooks create"
        | "websets searches create"
        | "websets enrichments create"
        | "admin keys create" => {
            if let Some(id) = id.as_deref() {
                push_create_followups(envelope, operation, id, globals, RunState::Started);
            }
        }
        "websets imports create" => {
            if let Some(upload_url) = upload_url.as_deref() {
                envelope["nextActions"]
                    .as_array_mut()
                    .expect("response envelopes initialize nextActions as an array")
                    .push(serde_json::json!({
                        "description": "Upload your CSV file",
                        "command": format!(
                            "curl -X PUT --data-binary @your-file.csv -H 'Content-Type: text/csv' {}",
                            shell_quote(upload_url)
                        ),
                    }));
            } else {
                // Without `uploadUrl` the create half-succeeded: there is nothing to upload to,
                // so the caller must not read exit 0 as "import ready". `upstream_malformed` is
                // an exit-5 error in the dictionary, never a warning on a success envelope.
                let import_id = id.as_deref();
                let mut details = serde_json::json!({
                    "field": "uploadUrl",
                    "command": "websets imports create",
                });
                if let Some(import_id) = import_id {
                    details["importId"] = serde_json::Value::String(import_id.to_string());
                }
                return Err(CliError::Upstream(
                    Diag::new(
                        "upstream_malformed",
                        "websets imports create response did not include string `uploadUrl`; the upload step could not be prepared",
                    )
                    .with_details(details)
                    .with_suggestion(format!(
                        "exa-agent websets imports get {}",
                        shell_quote(import_id.unwrap_or("<id>"))
                    )),
                ));
            }
        }
        "websets exports create" => {
            let Some(webset_id) = webset_id else {
                return Ok(());
            };
            if let Some(export_id) = id.as_deref() {
                push_resource_next_action(
                    envelope,
                    "Poll export status",
                    "websets exports get",
                    &[webset_id.to_string(), export_id.to_string()],
                    globals,
                );
            }
        }
        _ => {}
    }
    Ok(())
}

/// Whether the created run is still producing events. A run that already reached its terminal
/// event cannot be followed, so the stream follow-up is offered only to callers who created a
/// run without `--stream`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RunState {
    Started,
    Completed,
}

/// Follow-ups for a `… create` response, taking the new resource id directly so the streaming
/// terminal path (whose `data` is the event list, not the resource) can reuse it.
pub(crate) fn push_create_followups(
    envelope: &mut serde_json::Value,
    operation: &registry::OperationDef,
    id: &str,
    globals: &GlobalArgs,
    state: RunState,
) {
    let command = operation.command();
    let base = command.strip_suffix(" create").expect("create command");
    let Some(path) = envelope["operation"]["path"].as_str() else {
        return;
    };
    let Some(mut ids) = route_ids(operation.api_path, path) else {
        return;
    };
    ids.push(id.to_string());
    push_resource_next_action(
        envelope,
        "Inspect the created resource",
        &format!("{base} get"),
        &ids,
        globals,
    );
    if base == "agent runs" {
        if state == RunState::Started {
            push_resource_next_action(
                envelope,
                "Follow run events",
                "agent runs events --stream",
                &ids,
                globals,
            );
        }
    } else if base == "websets" {
        push_resource_next_action(
            envelope,
            "Read all webset items",
            "websets items list --all",
            &ids,
            globals,
        );
    }
}

fn push_next_action(envelope: &mut serde_json::Value, description: &str, command: String) {
    envelope["nextActions"]
        .as_array_mut()
        .expect("response envelopes initialize nextActions")
        .push(serde_json::json!({ "description": description, "command": command }));
}

pub(crate) fn append_pagination_next_action(
    envelope: &mut serde_json::Value,
    op: &registry::OperationDef,
    path: &str,
    query: &[(String, String)],
    globals: &GlobalArgs,
) {
    // The pagination driver may stop an unsafe cursor even when upstream still says more.
    if matches!(op.pagination, registry::Pagination::None)
        || envelope["pagination"]["hasMore"].as_bool() == Some(false)
    {
        return;
    }
    let data = &envelope["data"];
    let Some(cursor) = next_cursor(data) else {
        return;
    };
    if !has_more(data, Some(&cursor))
        || query
            .iter()
            .any(|(key, value)| key == "cursor" && value == &cursor)
    {
        return;
    }
    let Some(ids) = route_ids(op.api_path, path) else {
        return;
    };
    let mut args = vec!["exa-agent".to_string()];
    args.extend(op.cli_path.iter().map(|part| part.to_string()));
    for (key, value) in query {
        if redaction::is_secret_name(key) {
            warn_followup_context(envelope);
            return;
        }
        let flag = match key.as_str() {
            "cursor" => continue,
            "limit" | "status" | "name" | "search" | "successful" => key.as_str(),
            "sourceId" => "source-id",
            "websetId" => "webset-id",
            "types" => "type",
            "createdBefore" => "created-before",
            "createdAfter" => "created-after",
            "eventType" => "event-type",
            _ => {
                if let Some(key) = key
                    .strip_prefix("metadata[")
                    .and_then(|key| key.strip_suffix(']'))
                {
                    args.push(format!("--metadata={key}={value}"));
                    continue;
                }
                // Do not manufacture a continuation that silently loses an unknown filter.
                warn_followup_context(envelope);
                return;
            }
        };
        args.push(format!("--{flag}={value}"));
    }
    args.push(format!("--cursor={cursor}"));
    args.push("--json".to_string());
    if !append_followup_context(&mut args, globals) {
        warn_followup_context(envelope);
        return;
    }
    if !ids.is_empty() {
        args.push("--".to_string());
        args.extend(ids);
    }
    push_next_action(
        envelope,
        "Read the next page (same filters)",
        crate::shell_join_readable(&args),
    );
}

fn push_resource_next_action(
    envelope: &mut serde_json::Value,
    description: &str,
    command: &str,
    ids: &[String],
    globals: &GlobalArgs,
) {
    let mut args = vec!["exa-agent".to_string()];
    args.extend(command.split_whitespace().map(str::to_string));
    args.push("--json".to_string());
    if !append_followup_context(&mut args, globals) {
        warn_followup_context(envelope);
        return;
    }
    args.push("--".to_string());
    args.extend_from_slice(ids);
    push_next_action(envelope, description, crate::shell_join_readable(&args));
}

fn append_followup_context(args: &mut Vec<String>, globals: &GlobalArgs) -> bool {
    // Arbitrary caller headers can be proxy credentials even without a secret-looking name.
    // Only public media-negotiation headers are safe to omit; each follow-up chooses its own
    // media mode. In particular this covers the Accept header added by our SSE path.
    let Ok(headers) = parse_user_headers(&globals.headers) else {
        return false;
    };
    if headers.iter().any(|(name, value)| {
        !(name.eq_ignore_ascii_case("Accept")
            && matches!(value.as_str(), "application/json" | "text/event-stream"))
    }) {
        return false;
    }
    if globals
        .base_url
        .as_deref()
        .is_some_and(|base| !transport::is_safe_suggestion_base_url_origin(base))
    {
        return false;
    }
    if let Some(profile) = &globals.profile {
        args.push(format!("--profile={profile}"));
    }
    if let Some(base) = globals
        .base_url
        .as_deref()
        .filter(|base| transport::is_safe_suggestion_base_url_origin(base))
    {
        args.push(format!("--base-url={base}"));
    }
    if let Some(beta) = &globals.beta {
        args.push(format!("--beta={beta}"));
    }
    true
}

/// Scope an error's recovery command without exposing caller credentials or changing hosts.
pub(crate) fn scoped_recovery_command(base: &str, globals: &GlobalArgs) -> Option<String> {
    let mut context = Vec::new();
    if !append_followup_context(&mut context, globals) {
        return None;
    }
    if context.is_empty() {
        return Some(base.to_string());
    }
    Some(format!(
        "exa-agent {} {}",
        crate::shell_join(&context),
        base.strip_prefix("exa-agent ")?
    ))
}

fn warn_followup_context(envelope: &mut serde_json::Value) {
    if let Some(warnings) = envelope["warnings"].as_array_mut() {
        if !warnings
            .iter()
            .any(|warning| warning["code"] == "followup_context_required")
        {
            warnings.push(serde_json::json!({
                "code": "followup_context_required",
                "message": "A follow-up requires routing, headers, or filters that cannot safely be echoed. Preserve the original request context when resuming with the returned resource ID or cursor.",
            }));
        }
    }
}

/// Reverse our path-segment encoding, not a URL form decoder (`+` is a literal plus).
fn route_ids(template: &str, path: &str) -> Option<Vec<String>> {
    let template: Vec<_> = template.split('/').collect();
    let path: Vec<_> = path.split('/').collect();
    if template.len() != path.len() {
        return None;
    }
    template
        .iter()
        .zip(path)
        .filter(|(segment, _)| segment.starts_with('{') && segment.ends_with('}'))
        .map(|(_, value)| {
            let mut bytes = Vec::new();
            let mut input = value.bytes();
            while let Some(byte) = input.next() {
                bytes.push(if byte == b'%' {
                    let high = (input.next()? as char).to_digit(16)?;
                    let low = (input.next()? as char).to_digit(16)?;
                    (high * 16 + low) as u8
                } else {
                    byte
                });
            }
            String::from_utf8(bytes).ok()
        })
        .collect()
}
