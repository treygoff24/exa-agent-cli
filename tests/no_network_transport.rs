//! Process-isolated transport guard tests. This binary mutates the environment, so it stays
//! separate from the parallel transport seam tests.

use exa_agent_cli::transport::{
    send_with_retry, FakeTransport, HttpRequest, SendOptions, Transport, UreqTransport,
};

#[test]
fn no_network_guard_stops_before_fake_and_live_transport_send() {
    let previous = std::env::var("EXA_AGENT_NO_NETWORK").ok();
    unsafe { std::env::set_var("EXA_AGENT_NO_NETWORK", "1") };

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
    assert_eq!(result.diag().code, "usage_error");
    assert!(fake.recorded_requests().is_empty());

    let result = UreqTransport::with_defaults().send(&request).unwrap_err();
    assert_eq!(result.diag().code, "usage_error");

    match previous {
        Some(value) => unsafe { std::env::set_var("EXA_AGENT_NO_NETWORK", value) },
        None => unsafe { std::env::remove_var("EXA_AGENT_NO_NETWORK") },
    }
}
