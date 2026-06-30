//! Blocking HTTP transport (contracts §7, arch §5). All live upstream calls go through
//! [`Transport::send`]; ureq + rustls is the default impl, with an in-memory fake for tests.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::Read;
use std::time::Duration;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::auth::{CredentialNamespace, ResolvedCredential, Secret};
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

    fn send_sse<F>(
        &self,
        req: &HttpRequest,
        options: &SendOptions,
        on_item: &mut F,
    ) -> Result<(StreamOutcome, u32), CliError>
    where
        F: FnMut(StreamItem<'_>) -> Result<(), CliError>,
        Self: Sized,
    {
        let (response, retries) = send_with_retry(self, req, options)?;
        let frames = parse_sse(&response.body);
        let mut last_event_id = None;
        on_item(StreamItem::Bytes(&response.body))
            .map_err(|err| stream_callback_error(err, last_event_id.as_deref()))?;
        for frame in frames {
            let frame_id = frame.id.clone();
            on_item(StreamItem::Frame(frame))
                .map_err(|err| stream_callback_error(err, last_event_id.as_deref()))?;
            if frame_id.is_some() {
                last_event_id = frame_id;
            }
        }
        Ok((StreamOutcome { last_event_id }, retries))
    }
}

#[derive(Debug, Clone, Default)]
pub struct StreamOutcome {
    pub last_event_id: Option<String>,
}

pub enum StreamItem<'a> {
    Bytes(&'a [u8]),
    Frame(SseFrame),
}

/// Live transport backed by ureq + rustls (D14).
pub struct UreqTransport {
    agent: ureq::Agent,
    sse_agent: ureq::Agent,
}

impl UreqTransport {
    pub fn new(timeout: Duration) -> Self {
        let config = ureq::config::Config::builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build();
        let sse_config = ureq::config::Config::builder()
            .timeout_global(Some(timeout))
            .timeout_recv_body(Some(crate::stream::SSE_READ_TIMEOUT))
            .http_status_as_error(false)
            .build();
        Self {
            agent: config.into(),
            sse_agent: sse_config.into(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_TIMEOUT)
    }
}

impl Transport for UreqTransport {
    fn send(&self, req: &HttpRequest) -> Result<HttpResponse, CliError> {
        let response = send_ureq_request(&self.agent, req)?;

        let status = response.status().as_u16();
        let headers = response_headers(&response);
        let body = response.into_body().read_to_vec().map_err(map_ureq_error)?;
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }

    fn send_sse<F>(
        &self,
        req: &HttpRequest,
        options: &SendOptions,
        on_item: &mut F,
    ) -> Result<(StreamOutcome, u32), CliError>
    where
        F: FnMut(StreamItem<'_>) -> Result<(), CliError>,
    {
        crate::stream::install_sigint_handler()?;
        crate::stream::reset_interrupt();
        let max_retries = options.retry;
        let mut attempt = 0u32;
        loop {
            match self.send_sse_once(req, on_item) {
                Ok(outcome) => return Ok((outcome, attempt)),
                Err(err) => {
                    if should_retry(
                        &req.method,
                        options.idempotency_key.as_deref(),
                        &err,
                        attempt,
                        max_retries,
                    ) {
                        attempt += 1;
                        if let Some(ms) = retry_delay_ms_from_error(&err, options.retry_after) {
                            std::thread::sleep(Duration::from_millis(ms));
                        } else {
                            std::thread::sleep(Duration::from_millis(100 * u64::from(attempt)));
                        }
                        continue;
                    }
                    return Err(err);
                }
            }
        }
    }
}

impl UreqTransport {
    fn send_sse_once<F>(
        &self,
        req: &HttpRequest,
        on_item: &mut F,
    ) -> Result<StreamOutcome, CliError>
    where
        F: FnMut(StreamItem<'_>) -> Result<(), CliError>,
    {
        let mut response = send_ureq_request(&self.sse_agent, req)?;
        let status = response.status().as_u16();
        let headers = response_headers(&response);
        if !(200..300).contains(&status) {
            let body = response.body_mut().read_to_vec().map_err(map_ureq_error)?;
            return Err(classify_http_status(status, &body, &headers));
        }

        let mut decoder = crate::stream::SseDecoder::new();
        let mut last_emitted_event_id: Option<String> = None;
        let mut buf = [0u8; 8192];
        let mut saw_body = false;
        let mut reader = response.body_mut().as_reader();
        loop {
            if crate::stream::interrupted() {
                return Err(crate::stream::interrupted_stream_error(
                    last_emitted_event_id.as_deref(),
                ));
            }
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    saw_body = true;
                    let chunk = &buf[..n];
                    on_item(StreamItem::Bytes(chunk)).map_err(|err| {
                        stream_callback_error(err, last_emitted_event_id.as_deref())
                    })?;
                    for frame in decoder.push(chunk) {
                        let frame_id = frame.id.clone();
                        on_item(StreamItem::Frame(frame)).map_err(|err| {
                            stream_callback_error(err, last_emitted_event_id.as_deref())
                        })?;
                        if frame_id.is_some() {
                            last_emitted_event_id = frame_id;
                        }
                    }
                }
                Err(err) if crate::stream::is_poll_timeout(&err) => continue,
                Err(err) => {
                    if saw_body {
                        return Err(crate::stream::interrupted_stream_error(
                            last_emitted_event_id.as_deref(),
                        ));
                    }
                    let mut diag = Diag::new("network_error", err.to_string());
                    diag.retryable = true;
                    return Err(CliError::Network(diag));
                }
            }
        }
        for frame in decoder.finish() {
            let frame_id = frame.id.clone();
            on_item(StreamItem::Frame(frame))
                .map_err(|err| stream_callback_error(err, last_emitted_event_id.as_deref()))?;
            if frame_id.is_some() {
                last_emitted_event_id = frame_id;
            }
        }
        Ok(StreamOutcome {
            last_event_id: last_emitted_event_id,
        })
    }
}

fn stream_callback_error(err: CliError, last_event_id: Option<&str>) -> CliError {
    let Some(last_event_id) = last_event_id else {
        return err;
    };
    match err {
        CliError::Interrupted(mut diag) => {
            let mut details = diag
                .details
                .take()
                .map(|value| *value)
                .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
            match &mut details {
                serde_json::Value::Object(map) => {
                    map.entry("lastEventId".to_string())
                        .or_insert_with(|| serde_json::Value::String(last_event_id.to_string()));
                }
                other => {
                    let cause = std::mem::take(other);
                    *other = serde_json::json!({
                        "lastEventId": last_event_id,
                        "cause": cause,
                    });
                }
            }
            diag.details = Some(Box::new(details));
            CliError::Interrupted(diag)
        }
        other => other,
    }
}

fn send_ureq_request(
    agent: &ureq::Agent,
    req: &HttpRequest,
) -> Result<ureq::http::Response<ureq::Body>, CliError> {
    if let Some(body) = &req.body {
        macro_rules! send_body {
            ($builder:expr) => {{
                let mut builder = $builder;
                for (name, value) in &req.headers {
                    builder = builder.header(name.as_str(), value.as_str());
                }
                if !has_header(&req.headers, "content-type") {
                    builder = builder.header("Content-Type", "application/json");
                }
                builder.send(body.as_slice()).map_err(map_ureq_error)
            }};
        }
        match req.method.as_str() {
            "GET" => send_body!(agent.get(&req.url).force_send_body()),
            "POST" => send_body!(agent.post(&req.url)),
            "PUT" => send_body!(agent.put(&req.url)),
            "PATCH" => send_body!(agent.patch(&req.url)),
            "DELETE" => send_body!(agent.delete(&req.url).force_send_body()),
            "OPTIONS" => send_body!(agent.options(&req.url).force_send_body()),
            other => Err(CliError::Usage(Diag::new(
                "invalid_value",
                format!("unsupported HTTP method `{other}` with body"),
            ))),
        }
    } else {
        let mut builder = match req.method.as_str() {
            "GET" => agent.get(&req.url),
            "DELETE" => agent.delete(&req.url),
            "HEAD" => agent.head(&req.url),
            "OPTIONS" => agent.options(&req.url),
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
        builder.call().map_err(map_ureq_error)
    }
}

fn response_headers(response: &ureq::http::Response<ureq::Body>) -> Vec<(String, String)> {
    response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_string(), v.to_string()))
        })
        .collect()
}

fn retry_delay_ms_from_error(err: &CliError, retry_after: bool) -> Option<u64> {
    if !retry_after {
        return None;
    }
    match err {
        CliError::RateLimit(diag) => diag
            .details
            .as_deref()
            .and_then(|value| value.get("retryAfterMs"))
            .and_then(serde_json::Value::as_u64),
        _ => None,
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

pub fn encode_path_segment(s: &str) -> String {
    encode_component(s)
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

pub fn resolve_timeout(globals: &GlobalArgs, cfg: &Config) -> Result<Duration, CliError> {
    let raw = globals
        .timeout
        .as_deref()
        .or(cfg.timeout.as_deref())
        .unwrap_or(crate::config::DEFAULT_TIMEOUT);
    parse_duration(raw).ok_or_else(|| {
        CliError::Usage(
            Diag::new(
                "invalid_value",
                format!("invalid timeout `{raw}` (use e.g. `30s` or `250ms`)"),
            )
            .with_suggestion("exa-agent <command> --timeout 30s"),
        )
    })
}

fn parse_duration(raw: &str) -> Option<Duration> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    // `ms` must be tested before `s` — every `…ms` also ends in `s`.
    if let Some(ms) = raw.strip_suffix("ms") {
        return ms.trim().parse::<u64>().ok().map(Duration::from_millis);
    }
    if let Some(secs) = raw.strip_suffix('s') {
        return secs.trim().parse::<u64>().ok().map(Duration::from_secs);
    }
    raw.parse::<u64>().ok().map(Duration::from_secs)
}

pub fn resolve_base_url_for_namespace(
    globals: &GlobalArgs,
    cfg: &Config,
    namespace: CredentialNamespace,
) -> Result<String, CliError> {
    let url = match namespace {
        CredentialNamespace::Api => globals.base_url.clone().unwrap_or_else(|| {
            cfg.effective_base_url_for_profile(globals.profile.as_deref())
                .to_string()
        }),
        CredentialNamespace::Service => std::env::var("EXA_ADMIN_BASE_URL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| {
                cfg.effective_admin_base_url_for_profile(globals.profile.as_deref())
                    .to_string()
            }),
    };
    validate_base_url(&url)?;
    Ok(url)
}

/// Refuse to attach the managed key to a base URL that would leak it in cleartext
/// to a non-local host. `https` is always allowed; plain `http` only for loopback
/// (local dev/test servers, which never leave the machine). This is the egress
/// chokepoint — every live request resolves its base URL here before auth headers
/// are attached — so a `--base-url`/`EXA_ADMIN_BASE_URL` override pointed at an
/// attacker host (e.g. via prompt injection) cannot exfiltrate the credential.
fn validate_base_url(url: &str) -> Result<(), CliError> {
    if crate::config::is_valid_https_url(url) || is_loopback_http_url(url) {
        return Ok(());
    }
    Err(CliError::Usage(
        Diag::new(
            "invalid_value",
            format!(
                "refusing to send credentials to `{url}`: base URL must be https (plain http is allowed only for localhost)"
            ),
        )
        .with_suggestion("use an https base URL, e.g. --base-url https://api.exa.ai"),
    ))
}

fn is_loopback_http_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    if rest
        .chars()
        .any(|ch| ch.is_ascii_whitespace() || ch.is_control())
    {
        return false;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.contains('@') {
        return false;
    }
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        // `[ipv6]` or `[ipv6]:port`
        match stripped.split_once(']') {
            Some((host, _)) => host,
            None => return false,
        }
    } else {
        authority
            .rsplit_once(':')
            .map(|(host, _)| host)
            .unwrap_or(authority)
    };
    // Loopback literals only — parse as an IP so `127.0.0.1.evil.com` (a remote
    // host that merely starts with `127.`) is NOT treated as local.
    host == "localhost"
        || host
            .parse::<std::net::IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
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
    for key in ["results", "items", "data", "runs", "websets", "statuses"] {
        if let Some(items) = data.get(key).and_then(Value::as_array) {
            return Some(items.len() as u64);
        }
    }
    None
}

/// `/contents` may return HTTP 200 with per-item failures in `statuses[]` (contracts §11).
/// Mixed success/error batches exit 10 after the success envelope is emitted.
pub fn contents_mixed_status_exit_code(data: &Value) -> i32 {
    let Some(statuses) = data.get("statuses").and_then(Value::as_array) else {
        return 0;
    };
    let mut saw_success = false;
    let mut saw_error = false;
    for entry in statuses {
        match entry.get("status").and_then(Value::as_str) {
            Some("success") => saw_success = true,
            Some("error") => saw_error = true,
            _ => {}
        }
    }
    if saw_success && saw_error {
        10
    } else {
        0
    }
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

/// True when the merged request body opts into upstream SSE (`stream: true`).
pub fn body_wants_stream(body: &Value) -> bool {
    body.get("stream").and_then(Value::as_bool).unwrap_or(false)
}

/// Whether upstream returned an SSE payload (by header or recognizable framing).
pub fn response_is_sse(response: &HttpResponse) -> bool {
    if response.headers.iter().any(|(k, v)| {
        k.eq_ignore_ascii_case("content-type")
            && v.to_ascii_lowercase().contains("text/event-stream")
    }) {
        return true;
    }
    response.body.starts_with(b"data:") || response.body.starts_with(b"id:")
}

/// One SSE event block after blank-line framing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseFrame {
    pub id: Option<String>,
    pub data: Vec<String>,
}

/// Parse SSE bytes into framed events (`data:`, `id:`, `data: [DONE]`).
pub fn parse_sse(bytes: &[u8]) -> Vec<SseFrame> {
    let text = String::from_utf8_lossy(bytes);
    let mut frames = Vec::new();
    let mut id: Option<String> = None;
    let mut data = Vec::new();

    for line in text.lines() {
        if line.is_empty() {
            if id.is_some() || !data.is_empty() {
                frames.push(SseFrame {
                    id: id.take(),
                    data: std::mem::take(&mut data),
                });
            }
            continue;
        }
        if line.starts_with(':') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("id:") {
            id = Some(rest.trim_start().to_string());
            continue;
        }
        if let Some(rest) = line.strip_prefix("data:") {
            data.push(rest.trim_start().to_string());
        }
    }

    if id.is_some() || !data.is_empty() {
        frames.push(SseFrame { id, data });
    }
    frames
}

pub fn infer_stream_event_type(event: &Value) -> &'static str {
    if event.get("choices").is_some() {
        return "delta";
    }
    match event.get("type").and_then(Value::as_str) {
        Some("done") => "done",
        Some("error") => "error",
        _ if event.get("done").and_then(Value::as_bool) == Some(true) => "done",
        _ => "item",
    }
}

/// Accumulate parsed SSE JSON payloads into a single upstream-shaped `data` value.
pub fn accumulate_stream_data(frames: &[SseFrame]) -> Value {
    let mut events: Vec<_> = parsed_stream_events(frames)
        .map(|value| value.unwrap_or_else(Value::String))
        .collect();
    if events.len() == 1 {
        events.pop().unwrap_or(Value::Null)
    } else {
        Value::Array(events)
    }
}

/// Terminal response `data` for a stream: prefer final answer-like event, then concat deltas.
pub fn terminal_stream_data(frames: &[SseFrame]) -> Value {
    let mut fallback = Vec::new();
    let mut answer_like = None;
    let mut delta_text = String::new();

    for event in parsed_stream_events(frames) {
        match event {
            Ok(value) => {
                if value.get("answer").is_some() || value.get("citations").is_some() {
                    answer_like = Some(value.clone());
                }
                if let Some(content) = openai_delta_content(&value) {
                    delta_text.push_str(content);
                }
                fallback.push(value);
            }
            Err(raw) => fallback.push(Value::String(raw)),
        }
    }

    if let Some(value) = answer_like {
        return value;
    }
    if !delta_text.is_empty() {
        return serde_json::json!({ "answer": delta_text });
    }
    if fallback.len() == 1 {
        fallback.pop().unwrap_or(Value::Null)
    } else {
        Value::Array(fallback)
    }
}

fn parsed_stream_events(frames: &[SseFrame]) -> impl Iterator<Item = Result<Value, String>> + '_ {
    frames.iter().flat_map(|frame| {
        frame
            .data
            .iter()
            .filter(|chunk| chunk.as_str() != "[DONE]")
            .map(|chunk| serde_json::from_str::<Value>(chunk).map_err(|_| chunk.clone()))
    })
}

fn openai_delta_content(value: &Value) -> Option<&str> {
    value
        .get("choices")?
        .as_array()?
        .iter()
        .find_map(|choice| choice.get("delta")?.get("content")?.as_str())
}

/// Execute a live `raw` command through the supplied transport with a caller-provided request id.
pub fn execute_raw_with_request_id<T: Transport>(
    transport: &T,
    params: RawExecuteParams<'_>,
) -> Result<RawExecuteResult, CliError> {
    let prepared = prepare_raw_request(&params)?;
    let (response, retries) = send_with_retry(transport, &prepared.req, &prepared.send_opts)?;

    Ok(RawExecuteResult {
        request_id: prepared.request_id,
        method: prepared.method,
        path: prepared.path,
        profile: prepared.profile,
        correlation_id: prepared.correlation_id,
        response,
        retries,
    })
}

pub fn execute_raw_stream_with_request_id<T, F>(
    transport: &T,
    params: RawExecuteParams<'_>,
    on_item: &mut F,
) -> Result<(RawExecuteResult, Vec<SseFrame>), CliError>
where
    T: Transport,
    F: FnMut(StreamItem<'_>) -> Result<(), CliError>,
{
    let prepared = prepare_raw_request(&params)?;
    let mut body = Vec::new();
    let mut frames = Vec::new();
    let mut callback = |item: StreamItem<'_>| -> Result<(), CliError> {
        match item {
            StreamItem::Bytes(bytes) => {
                body.extend_from_slice(bytes);
                on_item(StreamItem::Bytes(bytes))
            }
            StreamItem::Frame(frame) => {
                frames.push(frame.clone());
                on_item(StreamItem::Frame(frame))
            }
        }
    };
    let (_outcome, retries) =
        transport.send_sse(&prepared.req, &prepared.send_opts, &mut callback)?;

    Ok((
        RawExecuteResult {
            request_id: prepared.request_id,
            method: prepared.method,
            path: prepared.path,
            profile: prepared.profile,
            correlation_id: prepared.correlation_id,
            response: HttpResponse {
                status: 200,
                headers: vec![("content-type".to_string(), "text/event-stream".to_string())],
                body,
            },
            retries,
        },
        frames,
    ))
}

struct PreparedRawRequest {
    req: HttpRequest,
    send_opts: SendOptions,
    request_id: String,
    method: String,
    path: String,
    profile: String,
    correlation_id: Option<String>,
}

fn prepare_raw_request(params: &RawExecuteParams<'_>) -> Result<PreparedRawRequest, CliError> {
    let cfg = Config::load()?;
    let method = params.method.to_ascii_uppercase();
    let query = parse_raw_query(params.query_raw)?;
    let base_url =
        resolve_base_url_for_namespace(params.globals, &cfg, params.credential.namespace)?;
    let url = build_url(&base_url, params.path, &query)?;

    let mut headers = parse_user_headers(&params.globals.headers)?;
    if body_wants_stream(&params.body) {
        headers.push(("Accept".to_string(), "text/event-stream".to_string()));
    }
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
    Ok(PreparedRawRequest {
        req,
        send_opts,
        request_id: params.request_id.clone(),
        method,
        path: params.path.to_string(),
        profile: params.credential.profile.clone(),
        correlation_id: params.globals.correlation_id.clone(),
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
    fn parse_duration_tries_ms_before_seconds() {
        assert_eq!(parse_duration("250ms"), Some(Duration::from_millis(250)));
        assert_eq!(parse_duration("30s"), Some(Duration::from_secs(30)));
        assert_eq!(parse_duration("5"), Some(Duration::from_secs(5)));
        // Unparseable values are rejected, not silently defaulted.
        assert_eq!(parse_duration("bogus"), None);
        assert_eq!(parse_duration("ms"), None);
        assert_eq!(parse_duration("12x"), None);
    }

    #[test]
    fn resolve_timeout_rejects_unparseable_value() {
        let cli =
            crate::cli::Cli::try_parse_from(["exa-agent", "--timeout", "bogus", "capabilities"])
                .unwrap();
        let err = resolve_timeout(&cli.globals, &Config::default()).unwrap_err();
        assert_eq!(err.diag().code, "invalid_value");
    }

    #[test]
    fn base_url_refuses_remote_cleartext_allows_https_and_loopback() {
        // https to anywhere, and http only to loopback, are accepted.
        assert!(validate_base_url("https://api.exa.ai").is_ok());
        assert!(validate_base_url("https://gateway.internal.corp/exa").is_ok());
        assert!(validate_base_url("http://127.0.0.1:8731").is_ok());
        assert!(validate_base_url("http://localhost:3000/x").is_ok());
        assert!(validate_base_url("http://[::1]:9000").is_ok());
        // Cleartext to a non-local host would exfiltrate the key — refused.
        assert_eq!(
            validate_base_url("http://collector.evil")
                .unwrap_err()
                .diag()
                .code,
            "invalid_value"
        );
        // A remote host that merely starts with `127.` is not loopback.
        assert!(validate_base_url("http://127.0.0.1.evil.com").is_err());
        assert!(validate_base_url("ftp://example.com").is_err());
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

    #[test]
    fn execute_contents_posts_urls_body() {
        let fake = FakeTransport::default();
        fake.push_ok_json(
            200,
            r#"{"results":[],"statuses":[{"id":"https://example.test","status":"success"}]}"#,
        );
        let cli = crate::cli::Cli::try_parse_from([
            "exa-agent",
            "--api-key",
            "test-key-12345678",
            "raw",
            "POST",
            "/contents",
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
            "/contents",
            &[],
            serde_json::json!({"urls": ["https://example.test"]}),
            &cli.globals,
            &cred,
        )
        .unwrap();
        assert_eq!(result.response.status, 200);
        let recorded = &fake.recorded_requests()[0];
        assert!(recorded.url.ends_with("/contents"));
        assert_eq!(recorded.method, "POST");
    }

    #[test]
    fn contents_mixed_statuses_exit_partial() {
        let mixed = serde_json::json!({
            "statuses": [
                { "id": "https://a.test", "status": "success" },
                { "id": "https://b.test", "status": "error" }
            ]
        });
        assert_eq!(contents_mixed_status_exit_code(&mixed), 10);

        let all_ok = serde_json::json!({
            "statuses": [{ "id": "https://a.test", "status": "success" }]
        });
        assert_eq!(contents_mixed_status_exit_code(&all_ok), 0);

        let all_err = serde_json::json!({
            "statuses": [{ "id": "https://a.test", "status": "error" }]
        });
        assert_eq!(contents_mixed_status_exit_code(&all_err), 0);
    }

    #[test]
    fn parse_sse_frames_data_id_and_done() {
        let bytes =
            b"id: evt-1\ndata: {\"seq\":1}\n\nid: evt-2\ndata: {\"seq\":2}\n\ndata: [DONE]\n\n";
        let frames = parse_sse(bytes);
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].id.as_deref(), Some("evt-1"));
        assert_eq!(frames[0].data, vec!["{\"seq\":1}".to_string()]);
        assert_eq!(frames[1].id.as_deref(), Some("evt-2"));
        assert_eq!(frames[2].data, vec!["[DONE]".to_string()]);
    }

    #[test]
    fn accumulate_stream_data_skips_done_marker() {
        let frames = parse_sse(b"data: {\"answer\":\"hi\"}\n\ndata: [DONE]\n\n");
        let data = accumulate_stream_data(&frames);
        assert_eq!(data["answer"], "hi");
    }

    #[test]
    fn body_wants_stream_reads_boolean_field() {
        assert!(!body_wants_stream(&serde_json::json!({})));
        assert!(body_wants_stream(&serde_json::json!({"stream": true})));
    }

    #[test]
    fn execute_raw_adds_sse_accept_when_stream_true() {
        let fake = FakeTransport::default();
        fake.push_ok_json(200, "data: {}\n\n");
        let cli = crate::cli::Cli::try_parse_from([
            "exa-agent",
            "--api-key",
            "test-key-12345678",
            "raw",
            "POST",
            "/answer",
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
        execute_raw(
            &fake,
            "POST",
            "/answer",
            &[],
            serde_json::json!({"query":"q","stream": true}),
            &cli.globals,
            &cred,
        )
        .unwrap();
        let recorded = &fake.recorded_requests()[0];
        assert!(recorded
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("accept") && v == "text/event-stream"));
    }

    #[test]
    fn send_sse_callback_error_reports_previous_emitted_event_id() {
        let fake = FakeTransport::default();
        fake.push_ok_json(
            200,
            "id: evt-1\ndata: {\"seq\":1}\n\nid: evt-2\ndata: {\"seq\":2}\n\n",
        );
        let req = HttpRequest {
            method: "GET".into(),
            url: "https://example.test/events".into(),
            headers: vec![],
            body: None,
        };
        let opts = SendOptions {
            retry: 0,
            retry_after: false,
            idempotency_key: None,
        };
        let mut callback = |item: StreamItem<'_>| -> Result<(), CliError> {
            if let StreamItem::Frame(frame) = item {
                if frame.id.as_deref() == Some("evt-2") {
                    return Err(CliError::Interrupted(Diag::new(
                        "interrupted",
                        "stdout closed",
                    )));
                }
            }
            Ok(())
        };

        let err = fake.send_sse(&req, &opts, &mut callback).unwrap_err();
        assert_eq!(err.category(), 12);
        assert_eq!(err.diag().details.as_ref().unwrap()["lastEventId"], "evt-1");
    }
}
