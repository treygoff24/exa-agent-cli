//! Blocking HTTP transport (contracts §7, arch §5). All live upstream calls go through
//! [`Transport::send`]; ureq + rustls is the default impl, with an in-memory fake for tests.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::auth::{ResolvedCredential, Secret};
use crate::cli::GlobalArgs;
use crate::config::Config;
use crate::error::{CliError, Diag};
use crate::redaction;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// A fully-resolved outbound HTTP call (after auth/header validation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// Upstream response bytes + metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Result of a successful raw command execution (before output formatting).
#[derive(Debug, Clone)]
pub struct RawExecuteResult {
    pub request_id: String,
    pub method: String,
    pub path: String,
    pub profile: String,
    pub correlation_id: Option<String>,
    pub response: HttpResponse,
    pub retries: u32,
}

pub struct RawExecuteParams<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub query_raw: &'a [String],
    pub body: Value,
    pub globals: &'a GlobalArgs,
    pub credential: &'a ResolvedCredential,
    pub request_id: String,
}

pub trait Transport {
    fn send(&self, req: &HttpRequest) -> Result<HttpResponse, CliError>;
}

/// Live transport backed by ureq + rustls (D14).
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl UreqTransport {
    pub fn new(timeout: Duration) -> Self {
        let config = ureq::config::Config::builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build();
        Self {
            agent: config.into(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_TIMEOUT)
    }
}

impl Transport for UreqTransport {
    fn send(&self, req: &HttpRequest) -> Result<HttpResponse, CliError> {
        let response = if let Some(body) = &req.body {
            macro_rules! send_body {
                ($builder:expr) => {{
                    let mut builder = $builder;
                    for (name, value) in &req.headers {
                        builder = builder.header(name.as_str(), value.as_str());
                    }
                    if !has_header(&req.headers, "content-type") {
                        builder = builder.header("Content-Type", "application/json");
                    }
                    builder.send(body.as_slice())
                }};
            }
            match req.method.as_str() {
                "GET" => send_body!(self.agent.get(&req.url).force_send_body()),
                "POST" => send_body!(self.agent.post(&req.url)),
                "PUT" => send_body!(self.agent.put(&req.url)),
                "PATCH" => send_body!(self.agent.patch(&req.url)),
                "DELETE" => send_body!(self.agent.delete(&req.url).force_send_body()),
                "OPTIONS" => send_body!(self.agent.options(&req.url).force_send_body()),
                other => {
                    return Err(CliError::Usage(Diag::new(
                        "invalid_value",
                        format!("unsupported HTTP method `{other}` with body"),
                    )));
                }
            }
        } else {
            let mut builder = match req.method.as_str() {
                "GET" => self.agent.get(&req.url),
                "DELETE" => self.agent.delete(&req.url),
                "HEAD" => self.agent.head(&req.url),
                "OPTIONS" => self.agent.options(&req.url),
                "POST" | "PUT" | "PATCH" => {
                    return Err(CliError::Usage(Diag::new(
                        "invalid_value",
                        format!(
                            "{} requires a JSON body (use `--body` or `--set`)",
                            req.method
                        ),
                    )));
                }
                other => {
                    return Err(CliError::Usage(Diag::new(
                        "invalid_value",
                        format!("unsupported HTTP method `{other}`"),
                    )));
                }
            };
            for (name, value) in &req.headers {
                builder = builder.header(name.as_str(), value.as_str());
            }
            builder.call()
        }
        .map_err(map_ureq_error)?;

        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| {
                value
                    .to_str()
                    .ok()
                    .map(|v| (name.as_str().to_string(), v.to_string()))
            })
            .collect();
        let body = response.into_body().read_to_vec().map_err(map_ureq_error)?;
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

/// In-memory transport for unit/integration tests (no network).
pub struct FakeTransport {
    responses: RefCell<VecDeque<Result<HttpResponse, CliError>>>,
    recorded: RefCell<Vec<HttpRequest>>,
}

impl Default for FakeTransport {
    fn default() -> Self {
        Self {
            responses: RefCell::new(VecDeque::new()),
            recorded: RefCell::new(Vec::new()),
        }
    }
}

impl FakeTransport {
    pub fn ok_json(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: body.as_bytes().to_vec(),
        }
    }

    pub fn push_ok_json(&self, status: u16, body: &str) {
        self.responses
            .borrow_mut()
            .push_back(Ok(Self::ok_json(status, body)));
    }

    pub fn push_err(&self, err: CliError) {
        self.responses.borrow_mut().push_back(Err(err));
    }

    pub fn recorded_requests(&self) -> Vec<HttpRequest> {
        self.recorded.borrow().clone()
    }
}

impl Transport for FakeTransport {
    fn send(&self, req: &HttpRequest) -> Result<HttpResponse, CliError> {
        self.recorded.borrow_mut().push(req.clone());
        self.responses.borrow_mut().pop_front().unwrap_or_else(|| {
            Err(CliError::Network(Diag::new(
                "network_error",
                "FakeTransport: no canned response",
            )))
        })
    }
}

/// Refuse user-supplied auth/secret headers (contracts §12 / D18).
pub fn parse_user_headers(raw: &[String]) -> Result<Vec<(String, String)>, CliError> {
    let mut out = Vec::new();
    for item in raw {
        let (name, value) = item.split_once(':').ok_or_else(|| {
            CliError::Usage(Diag::new(
                "invalid_value",
                "`--header` must be `Name: value`",
            ))
        })?;
        let name = name.trim();
        let value = value.trim();
        if name.is_empty() {
            return Err(CliError::Usage(Diag::new(
                "invalid_value",
                "`--header` name must not be empty",
            )));
        }
        if is_forbidden_header(name) {
            return Err(CliError::Usage(
                Diag::new(
                    "invalid_flag_combination",
                    format!("`--header` cannot override managed header `{name}`"),
                )
                .with_suggestion(
                    "use --api-key / EXA_API_KEY; auth headers are injected by the CLI",
                ),
            ));
        }
        out.push((name.to_string(), value.to_string()));
    }
    Ok(out)
}

fn is_forbidden_header(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    redaction::is_secret_name(&n) || n == "x-api-key" || n == "idempotency-key"
}

fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers
        .iter()
        .any(|(header, _)| header.eq_ignore_ascii_case(name))
}

pub fn build_url(base: &str, path: &str, query: &[(String, String)]) -> Result<String, CliError> {
    let base = base.trim_end_matches('/');
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let mut url = format!("{base}{path}");
    if !query.is_empty() {
        let qs = query
            .iter()
            .map(|(k, v)| format!("{}={}", encode_component(k), encode_component(v)))
            .collect::<Vec<_>>()
            .join("&");
        url.push('?');
        url.push_str(&qs);
    }
    Ok(url)
}

fn encode_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub fn parse_raw_query(raw: &[String]) -> Result<Vec<(String, String)>, CliError> {
    raw.iter()
        .map(|item| {
            let (name, value) = item.split_once('=').ok_or_else(|| {
                CliError::Usage(Diag::new(
                    "invalid_value",
                    "raw --query expects `key=value`",
                ))
            })?;
            if name.is_empty() {
                return Err(CliError::Usage(Diag::new(
                    "invalid_value",
                    "raw --query expects a non-empty key",
                )));
            }
            Ok((name.to_string(), value.to_string()))
        })
        .collect()
}

pub fn resolve_timeout(globals: &GlobalArgs, cfg: &Config) -> Duration {
    let raw = globals
        .timeout
        .as_deref()
        .or(cfg.timeout.as_deref())
        .unwrap_or(crate::config::DEFAULT_TIMEOUT);
    parse_duration(raw).unwrap_or(DEFAULT_TIMEOUT)
}

fn parse_duration(raw: &str) -> Option<Duration> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some(secs) = raw.strip_suffix('s') {
        return secs.parse::<u64>().ok().map(Duration::from_secs);
    }
    if let Some(ms) = raw.strip_suffix("ms") {
        return ms.parse::<u64>().ok().map(Duration::from_millis);
    }
    raw.parse::<u64>().ok().map(Duration::from_secs)
}

pub fn resolve_base_url(globals: &GlobalArgs, cfg: &Config) -> String {
    globals
        .base_url
        .clone()
        .unwrap_or_else(|| cfg.effective_base_url().to_string())
}

fn inject_auth_headers(headers: &mut Vec<(String, String)>, secret: &Secret) {
    headers.push(("x-api-key".to_string(), secret.expose().to_string()));
}

pub fn new_request_id() -> String {
    let epoch = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0)
        });
    format!("req_local_{epoch:016x}")
}

fn request_is_idempotent(method: &str) -> bool {
    matches!(method, "GET" | "HEAD" | "OPTIONS")
}

fn should_retry(
    method: &str,
    idempotency_key: Option<&str>,
    err: &CliError,
    attempt: u32,
    max_retries: u32,
) -> bool {
    if attempt >= max_retries {
        return false;
    }
    if !request_is_idempotent(method) && idempotency_key.is_none() {
        return false;
    }
    match err {
        CliError::Network(d) => d.retryable,
        CliError::RateLimit(d) => d.retryable,
        CliError::Upstream(d) => d.retryable,
        _ => false,
    }
}

fn retry_delay_ms(response: Option<&HttpResponse>, retry_after: bool) -> u64 {
    if !retry_after {
        return 0;
    }
    response
        .and_then(|r| {
            r.headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("retry-after"))
        })
        .and_then(|(_, v)| v.parse::<u64>().ok())
        .map(|secs| secs.saturating_mul(1000))
        .unwrap_or(0)
}

pub fn send_with_retry<T: Transport>(
    transport: &T,
    req: &HttpRequest,
    options: &SendOptions,
) -> Result<(HttpResponse, u32), CliError> {
    let max_retries = options.retry;
    let mut attempt = 0u32;
    loop {
        match transport.send(req) {
            Ok(resp) if (200..300).contains(&resp.status) => {
                return Ok((resp, attempt));
            }
            Ok(resp) => {
                let delay = retry_delay_ms(Some(&resp), options.retry_after);
                let err = classify_http_status(resp.status, &resp.body, &resp.headers);
                if should_retry(
                    &req.method,
                    options.idempotency_key.as_deref(),
                    &err,
                    attempt,
                    max_retries,
                ) {
                    attempt += 1;
                    if delay > 0 {
                        std::thread::sleep(Duration::from_millis(delay));
                    }
                    continue;
                }
                return Err(err);
            }
            Err(err) => {
                if should_retry(
                    &req.method,
                    options.idempotency_key.as_deref(),
                    &err,
                    attempt,
                    max_retries,
                ) {
                    attempt += 1;
                    std::thread::sleep(Duration::from_millis(100 * u64::from(attempt)));
                    continue;
                }
                return Err(err);
            }
        }
    }
}

/// Retry/idempotency knobs shared by transport sends.
#[derive(Debug, Clone)]
pub struct SendOptions {
    pub retry: u32,
    pub retry_after: bool,
    pub idempotency_key: Option<String>,
}

pub fn classify_http_status(status: u16, body: &[u8], headers: &[(String, String)]) -> CliError {
    let snippet = String::from_utf8_lossy(body);
    let message = first_line(&snippet);
    match status {
        401 | 403 => {
            let mut diag = Diag::new("reauth_required", message);
            diag.http_status = Some(status);
            diag.retryable = false;
            CliError::Auth(diag)
        }
        404 => {
            let mut diag = Diag::new("not_found", message);
            diag.http_status = Some(status);
            diag.retryable = false;
            CliError::NotFound(diag)
        }
        409 => {
            let code = if snippet.to_ascii_lowercase().contains("idempotenc") {
                "idempotency_conflict"
            } else {
                "conflict"
            };
            let mut diag = Diag::new(code, message);
            diag.http_status = Some(status);
            diag.retryable = false;
            CliError::Conflict(diag)
        }
        429 => {
            let retry_after_ms = headers
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("retry-after"))
                .and_then(|(_, v)| v.parse::<u64>().ok())
                .map(|secs| secs.saturating_mul(1000));
            let mut diag = Diag::new("rate_limited", message);
            diag.http_status = Some(status);
            diag.retryable = true;
            if let Some(ms) = retry_after_ms {
                diag = diag.with_details(serde_json::json!({ "retryAfterMs": ms }));
            }
            CliError::RateLimit(diag)
        }
        500..=599 => {
            let mut diag = Diag::new("upstream_error", message);
            diag.http_status = Some(status);
            diag.retryable = true;
            CliError::Upstream(diag)
        }
        400..=499 => {
            let mut diag = Diag::new("invalid_value", message);
            diag.http_status = Some(status);
            diag.retryable = false;
            CliError::Usage(diag)
        }
        _ => {
            let mut diag = Diag::new("upstream_malformed", message);
            diag.http_status = Some(status);
            diag.retryable = false;
            CliError::Upstream(diag)
        }
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or(s).chars().take(240).collect()
}

fn map_ureq_error(err: ureq::Error) -> CliError {
    let mut diag = Diag::new("network_error", err.to_string());
    diag.retryable = true;
    CliError::Network(diag)
}

pub fn parse_response_data(body: &[u8]) -> Value {
    if body.is_empty() {
        return Value::Null;
    }
    serde_json::from_slice(body)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(body).into_owned()))
}

pub fn data_hash(data: &Value) -> Option<String> {
    let bytes = serde_json::to_vec(data).ok()?;
    let digest = Sha256::digest(bytes);
    Some(format!("sha256:{digest:x}"))
}

pub fn primary_count(data: &Value) -> Option<u64> {
    if let Some(items) = data.as_array() {
        return Some(items.len() as u64);
    }
    for key in ["results", "items", "data", "runs", "websets"] {
        if let Some(items) = data.get(key).and_then(Value::as_array) {
            return Some(items.len() as u64);
        }
    }
    None
}

/// Execute a live `raw` command through the supplied transport.
pub fn execute_raw<T: Transport>(
    transport: &T,
    method: &str,
    path: &str,
    query_raw: &[String],
    body: Value,
    globals: &GlobalArgs,
    credential: &ResolvedCredential,
) -> Result<RawExecuteResult, CliError> {
    execute_raw_with_request_id(
        transport,
        RawExecuteParams {
            method,
            path,
            query_raw,
            body,
            globals,
            credential,
            request_id: new_request_id(),
        },
    )
}

/// Execute a live `raw` command through the supplied transport with a caller-provided request id.
pub fn execute_raw_with_request_id<T: Transport>(
    transport: &T,
    params: RawExecuteParams<'_>,
) -> Result<RawExecuteResult, CliError> {
    let cfg = Config::load()?;
    let method = params.method.to_ascii_uppercase();
    let query = parse_raw_query(params.query_raw)?;
    let base_url = resolve_base_url(params.globals, &cfg);
    let url = build_url(&base_url, params.path, &query)?;

    let mut headers = parse_user_headers(&params.globals.headers)?;
    if let Some(key) = &params.globals.idempotency_key {
        headers.push(("Idempotency-Key".to_string(), key.clone()));
    }
    if let Some(beta) = &params.globals.beta {
        headers.push(("x-exa-beta".to_string(), beta.clone()));
    }
    inject_auth_headers(&mut headers, &params.credential.secret);

    let body_bytes = if params.body.is_null() {
        None
    } else {
        Some(serde_json::to_vec(&params.body).map_err(|e| {
            CliError::Usage(Diag::new(
                "invalid_value",
                format!("request body is not serializable JSON: {e}"),
            ))
        })?)
    };

    let req = HttpRequest {
        method: method.clone(),
        url,
        headers,
        body: body_bytes,
    };

    let send_opts = SendOptions {
        retry: params.globals.retry,
        retry_after: params.globals.retry_after,
        idempotency_key: params.globals.idempotency_key.clone(),
    };
    let (response, retries) = send_with_retry(transport, &req, &send_opts)?;

    Ok(RawExecuteResult {
        request_id: params.request_id,
        method,
        path: params.path.to_string(),
        profile: params.credential.profile.clone(),
        correlation_id: params.globals.correlation_id.clone(),
        response,
        retries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{self, NoopKeyring};
    use clap::Parser;

    #[test]
    fn build_url_joins_base_path_and_query() {
        let url = build_url(
            "https://api.exa.ai",
            "/search",
            &[("limit".into(), "10".into())],
        )
        .unwrap();
        assert_eq!(url, "https://api.exa.ai/search?limit=10");
    }

    #[test]
    fn refuses_managed_auth_header() {
        let err = parse_user_headers(&["Authorization: Bearer leak".into()]).unwrap_err();
        assert_eq!(err.diag().code, "invalid_flag_combination");
        let err = parse_user_headers(&["x-api-key: leak".into()]).unwrap_err();
        assert_eq!(err.diag().code, "invalid_flag_combination");
    }

    #[test]
    fn classify_status_maps_auth_and_rate_limit() {
        let auth = classify_http_status(401, b"unauthorized", &[]);
        assert!(matches!(auth, CliError::Auth(_)));
        let rl = classify_http_status(429, b"too many", &[("Retry-After".into(), "2".into())]);
        assert!(matches!(rl, CliError::RateLimit(_)));
        assert_eq!(rl.diag().details.as_ref().unwrap()["retryAfterMs"], 2000);
    }

    #[test]
    fn fake_transport_records_request_and_returns_canned_body() {
        let fake = FakeTransport::default();
        fake.push_ok_json(200, r#"{"ok":true}"#);
        let req = HttpRequest {
            method: "GET".to_string(),
            url: "https://example.test/health".to_string(),
            headers: vec![],
            body: None,
        };
        let resp = fake.send(&req).unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(fake.recorded_requests()[0].url, req.url);
    }

    #[test]
    fn execute_raw_posts_json_with_injected_auth() {
        let fake = FakeTransport::default();
        fake.push_ok_json(200, r#"{"results":[]}"#);
        let cli = crate::cli::Cli::try_parse_from([
            "exa-agent",
            "--api-key",
            "test-key-12345678",
            "--header",
            "X-Trace: abc",
            "raw",
            "POST",
            "/search",
        ])
        .unwrap();
        let cred = auth::resolve_api_credential(
            &auth::CredentialInput {
                explicit: Some("test-key-12345678".into()),
                ..Default::default()
            },
            &NoopKeyring,
        )
        .unwrap();
        let result = execute_raw(
            &fake,
            "POST",
            "/search",
            &[],
            serde_json::json!({"query":"hi"}),
            &cli.globals,
            &cred,
        )
        .unwrap();
        assert_eq!(result.response.status, 200);
        let recorded = &fake.recorded_requests()[0];
        assert!(recorded.url.ends_with("/search"));
        assert!(recorded.headers.iter().any(|(k, _)| k == "x-api-key"));
        assert!(!recorded.headers.iter().any(|(k, _)| k == "Authorization"));
        assert!(recorded
            .body
            .as_ref()
            .unwrap()
            .windows(5)
            .any(|w| w == b"query"));
    }

    #[test]
    fn post_without_idempotency_key_is_not_retried_on_503() {
        let fake = FakeTransport::default();
        fake.push_ok_json(503, "down");
        fake.push_ok_json(200, r#"{"ok":true}"#);
        let req = HttpRequest {
            method: "POST".to_string(),
            url: "https://example.test/search".to_string(),
            headers: vec![],
            body: Some(br#"{"q":"x"}"#.to_vec()),
        };
        let opts = SendOptions {
            retry: 2,
            retry_after: false,
            idempotency_key: None,
        };
        let err = send_with_retry(&fake, &req, &opts).unwrap_err();
        assert!(matches!(err, CliError::Upstream(_)));
        assert_eq!(fake.recorded_requests().len(), 1);
    }

    #[test]
    fn get_retries_on_503() {
        let fake = FakeTransport::default();
        fake.push_ok_json(503, "down");
        fake.push_ok_json(200, r#"{"ok":true}"#);
        let req = HttpRequest {
            method: "GET".to_string(),
            url: "https://example.test/health".to_string(),
            headers: vec![],
            body: None,
        };
        let opts = SendOptions {
            retry: 2,
            retry_after: false,
            idempotency_key: None,
        };
        let (resp, retries) = send_with_retry(&fake, &req, &opts).unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(retries, 1);
        assert_eq!(fake.recorded_requests().len(), 2);
    }
}
