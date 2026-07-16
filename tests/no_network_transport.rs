//! Process-isolated transport guard tests. This binary mutates the environment, so it stays
//! separate from the parallel transport seam tests.

use clap::Parser;
use exa_agent_cli::auth::{self, NoopKeyring};
use exa_agent_cli::cli::Cli;
use exa_agent_cli::error::{CliError, Diag};
use exa_agent_cli::transport::{
    ensure_network_allowed, execute_raw_stream_with_request_id, send_with_retry, FakeTransport,
    HttpRequest, RawExecuteParams, SendOptions, StreamItem, StreamOutcome, Transport,
    UreqTransport,
};
use std::cell::Cell;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct NeverReachedTransport {
    send_calls: Cell<u32>,
    sse_calls: Cell<u32>,
}

impl Transport for NeverReachedTransport {
    fn send(&self, _req: &HttpRequest) -> Result<exa_agent_cli::transport::HttpResponse, CliError> {
        self.send_calls.set(self.send_calls.get() + 1);
        Err(CliError::Network(Diag::new("test", "send reached")))
    }

    fn send_sse<F>(
        &self,
        _req: &HttpRequest,
        _options: &SendOptions,
        _on_item: &mut F,
    ) -> Result<(StreamOutcome, u32), CliError>
    where
        F: FnMut(StreamItem<'_>) -> Result<(), CliError>,
    {
        self.sse_calls.set(self.sse_calls.get() + 1);
        Err(CliError::Network(Diag::new("test", "send_sse reached")))
    }
}

#[test]
fn no_network_guard_stops_before_fake_and_live_transport_send_for_any_present_value() {
    let _lock = ENV_LOCK.lock().unwrap();
    let previous = std::env::var("EXA_AGENT_NO_NETWORK").ok();
    for value in ["1", "true", "TRUE", "yes", "01", ""] {
        unsafe { std::env::set_var("EXA_AGENT_NO_NETWORK", value) };
        let fake = FakeTransport::default();
        fake.push_ok_json(200, "{}");
        let request = HttpRequest {
            method: "GET".to_string(),
            url: "http://127.0.0.1:1/never-sent".to_string(),
            headers: Vec::new(),
            body: None,
        };
        let options = SendOptions {
            retry: 0,
            retry_after: false,
            idempotency_key: None,
        };
        let result = send_with_retry(&fake, &request, &options).unwrap_err();
        assert_eq!(result.diag().code, "usage_error", "value {value:?}");
        assert!(fake.recorded_requests().is_empty(), "value {value:?}");
        let result = UreqTransport::with_defaults().send(&request).unwrap_err();
        assert_eq!(result.diag().code, "usage_error", "value {value:?}");
    }
    unsafe { std::env::remove_var("EXA_AGENT_NO_NETWORK") };
    assert!(
        ensure_network_allowed().is_ok(),
        "absence is the only off state"
    );

    match previous {
        Some(value) => unsafe { std::env::set_var("EXA_AGENT_NO_NETWORK", value) },
        None => unsafe { std::env::remove_var("EXA_AGENT_NO_NETWORK") },
    }
}

#[test]
fn no_network_guard_stops_before_custom_stream_transport_override() {
    let _lock = ENV_LOCK.lock().unwrap();
    let previous = std::env::var("EXA_AGENT_NO_NETWORK").ok();
    unsafe { std::env::set_var("EXA_AGENT_NO_NETWORK", "1") };

    let cli = Cli::try_parse_from([
        "exa-agent",
        "--api-key",
        "test-key-abcdef12",
        "answer",
        "hi",
    ])
    .unwrap();
    let credential = auth::resolve_api_credential(
        &auth::CredentialInput {
            explicit: Some("test-key-abcdef12".into()),
            ..Default::default()
        },
        &NoopKeyring,
    )
    .unwrap();
    let transport = NeverReachedTransport {
        send_calls: Cell::new(0),
        sse_calls: Cell::new(0),
    };
    let params = RawExecuteParams {
        method: "POST",
        path: "/answer",
        query_raw: &[],
        body: serde_json::json!({"query": "never sent"}),
        globals: &cli.globals,
        credential: &credential,
        request_id: "req_guard".to_string(),
    };
    let mut on_item = |_item: StreamItem<'_>| Ok(());
    let error = execute_raw_stream_with_request_id(&transport, params, &mut on_item).unwrap_err();
    assert_eq!(error.diag().code, "usage_error");
    assert_eq!(transport.send_calls.get(), 0);
    assert_eq!(transport.sse_calls.get(), 0);

    match previous {
        Some(value) => unsafe { std::env::set_var("EXA_AGENT_NO_NETWORK", value) },
        None => unsafe { std::env::remove_var("EXA_AGENT_NO_NETWORK") },
    }
}
