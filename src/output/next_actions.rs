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
    let Some(data) = envelope.get("data").cloned() else {
        return Ok(());
    };
    match operation.command().as_str() {
        "batches create" => {
            if let Some(id) = data.get("id").and_then(serde_json::Value::as_str) {
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
            if let Some(url) = data
                .get("resultsUrl")
                .and_then(serde_json::Value::as_str)
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
            if let Some(id) = data.get("id").and_then(serde_json::Value::as_str) {
                let base = operation.command();
                let base = base.strip_suffix(" create").expect("create command");
                let Some(path) = envelope["operation"]["path"].as_str() else {
                    return Ok(());
                };
                let Some(mut ids) = route_ids(operation.api_path, path) else {
                    return Ok(());
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
                    push_resource_next_action(
                        envelope,
                        "Follow run events",
                        "agent runs events --stream",
                        &ids,
                        globals,
                    );
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
        }
        "websets imports create" => {
            if let Some(upload_url) = data.get("uploadUrl").and_then(serde_json::Value::as_str) {
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
                let import_id = data.get("id").and_then(serde_json::Value::as_str);
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
            if let Some(export_id) = data.get("id").and_then(serde_json::Value::as_str) {
                envelope["nextActions"]
                    .as_array_mut()
                    .expect("response envelopes initialize nextActions as an array")
                    .push(serde_json::json!({
                        "description": "Poll export status",
                        "command": format!(
                            "exa-agent websets exports get {} {}",
                            shell_quote(webset_id),
                            shell_quote(export_id)
                        ),
                    }));
            }
        }
        _ => {}
    }
    Ok(())
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
    if matches!(op.pagination, registry::Pagination::None) {
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
                return;
            }
        };
        args.push(format!("--{flag}={value}"));
    }
    args.push(format!("--cursor={cursor}"));
    args.push("--json".to_string());
    append_followup_context(&mut args, globals);
    if !ids.is_empty() {
        args.push("--".to_string());
        args.extend(ids);
    }
    push_next_action(
        envelope,
        "Read the next page (same filters)",
        args.iter()
            .map(|arg| shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" "),
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
    append_followup_context(&mut args, globals);
    args.push("--".to_string());
    args.extend_from_slice(ids);
    push_next_action(
        envelope,
        description,
        args.iter()
            .map(|arg| shell_quote(arg))
            .collect::<Vec<_>>()
            .join(" "),
    );
}

fn append_followup_context(args: &mut Vec<String>, globals: &GlobalArgs) {
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
    // Caller headers passed managed-secret validation before dispatch; reuse only non-secrets.
    if let Ok(headers) = parse_user_headers(&globals.headers) {
        args.extend(
            headers
                .iter()
                .filter(|(name, _)| !redaction::is_secret_name(name))
                .map(|(name, value)| format!("--header={name}: {value}")),
        );
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
