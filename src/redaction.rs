//! Shared redaction helpers for CLI previews, errors, and traces.

use serde_json::Value;

pub const REDACTED: &str = "<redacted>";

pub fn is_secret_name(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    if matches!(n.as_str(), "tokensnum" | "tokens_num" | "tokens-num") {
        return false;
    }
    n.contains("authorization")
        || n.contains("api-key")
        || n.contains("api_key")
        || n.contains("apikey")
        || n.contains("service-key")
        || n.contains("service_key")
        || n.contains("servicekey")
        || n.contains("access-key")
        || n.contains("access_key")
        || n.contains("accesskey")
        || n.contains("token")
        || n.contains("secret")
        || n.contains("password")
}

pub fn redact_header(header: &str) -> String {
    let name = header
        .split_once(':')
        .map(|(name, _)| name)
        .unwrap_or(header);
    if is_secret_name(name) {
        format!("{}: {REDACTED}", name.trim())
    } else if let Some((name, value)) = header.split_once(':') {
        format!("{}:{}", name, scrub_text(value))
    } else {
        scrub_text(header)
    }
}

pub fn redact_set_value(value: &str) -> String {
    let key = value.split_once('=').map(|(key, _)| key).unwrap_or(value);
    if is_secret_name(key) {
        format!("{key}={REDACTED}")
    } else {
        value.to_string()
    }
}

pub fn redact_json_value(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if is_secret_name(key) {
                    *value = Value::String(REDACTED.to_string());
                } else {
                    redact_json_value(value);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_json_value(item);
            }
        }
        Value::String(s) => {
            *s = scrub_text(s);
        }
        _ => {}
    }
}

pub fn scrub_json_value(value: &mut Value) {
    scrub_json_value_for_key(None, value);
}

fn scrub_json_value_for_key(parent_key: Option<&str>, value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if is_secret_name(key) {
                    *value = Value::String(REDACTED.to_string());
                } else {
                    scrub_json_value_for_key(Some(key), value);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                scrub_json_value_for_key(parent_key, item);
            }
        }
        Value::String(s) if !is_structural_string_key(parent_key) => {
            *s = scrub_text(s);
        }
        Value::String(_) => {}
        _ => {}
    }
}

fn is_structural_string_key(key: Option<&str>) -> bool {
    let Some(key) = key else {
        return false;
    };
    matches!(key, "schema" | "id" | "code" | "category" | "status" | "ok")
}

pub fn scrub_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut token = String::new();
    for ch in input.chars() {
        if is_token_char(ch) {
            token.push(ch);
        } else {
            flush_token(&mut out, &mut token);
            out.push(ch);
        }
    }
    flush_token(&mut out, &mut token);
    out
}

fn flush_token(out: &mut String, token: &mut String) {
    if token.is_empty() {
        return;
    }
    if looks_like_secret_value(token) {
        out.push_str(REDACTED);
    } else {
        out.push_str(token);
    }
    token.clear();
}

fn is_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.')
}

fn looks_like_secret_value(token: &str) -> bool {
    let t = token
        .trim_matches(|c: char| !is_token_char(c))
        .to_ascii_lowercase();
    if t.is_empty() {
        return false;
    }
    if t == "exa-agent" {
        return false;
    }
    t.starts_with("exa-")
        || t.starts_with("sk-exa")
        || t.starts_with("sk_exa")
        || t.starts_with("svc-")
        || t.starts_with("service-")
        || (t.contains("secret") && t.len() >= 10)
        || is_uuid_like(&t)
}

fn is_uuid_like(token: &str) -> bool {
    let parts: Vec<&str> = token.split('-').collect();
    let lens = [8, 4, 4, 4, 12];
    parts.len() == lens.len()
        && parts
            .iter()
            .zip(lens)
            .all(|(part, len)| part.len() == len && part.chars().all(|c| c.is_ascii_hexdigit()))
}
