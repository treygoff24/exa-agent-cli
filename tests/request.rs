//! Request-builder merge semantics (arch §4 / D39).

use exa_agent_cli::error::CliError;
use exa_agent_cli::registry;
use exa_agent_cli::request::{self, BodySource, RequestOverrides};
use serde_json::json;

fn search_op() -> &'static registry::OperationDef {
    registry::lookup_by_segments(&["search"]).expect("search op")
}

#[test]
fn body_deep_merges_over_named_flags() {
    let spec = request::build_request(
        search_op(),
        &[
            ("query", Some("hello".into())),
            ("num-results", Some("5".into())),
            ("category", Some("news".into())),
        ],
        RequestOverrides {
            body: Some(BodySource::Inline(
                r#"{"numResults":10,"contents":{"text":true}}"#,
            )),
            sets: &[],
        },
    )
    .unwrap();

    assert_eq!(spec.body["query"], "hello");
    assert_eq!(spec.body["numResults"], 10);
    assert_eq!(spec.body["category"], "news");
    assert_eq!(spec.body["contents"]["text"], true);
}

#[test]
fn set_applies_last_and_supports_nested_paths() {
    let spec = request::build_request(
        search_op(),
        &[("query", Some("hello".into()))],
        RequestOverrides {
            body: None,
            sets: &[
                "contents.text=true".into(),
                "contents.text.maxCharacters=1000".into(),
                "category=research".into(),
            ],
        },
    )
    .unwrap();

    assert_eq!(spec.body["contents"]["text"]["maxCharacters"], 1000);
    assert_eq!(spec.body["category"], "research");
}

#[test]
fn set_array_index_last_writer_wins() {
    let spec = request::build_request(
        search_op(),
        &[("query", Some("q".into()))],
        RequestOverrides {
            body: Some(BodySource::Inline(
                r#"{"users":[{"name":"a"},{"name":"b"}]}"#,
            )),
            sets: &["users.1.name=beta".into(), "users.0.name=alpha".into()],
        },
    )
    .unwrap();

    assert_eq!(spec.body["users"][0]["name"], "alpha");
    assert_eq!(spec.body["users"][1]["name"], "beta");
}

#[test]
fn invalid_inline_body_is_usage() {
    let err = request::build_request(
        search_op(),
        &[("query", Some("q".into()))],
        RequestOverrides {
            body: Some(BodySource::Inline("{not json")),
            sets: &[],
        },
    )
    .unwrap_err();
    assert!(matches!(err, CliError::Usage(_)));
    assert_eq!(err.diag().code, "invalid_value");
}

#[test]
fn missing_body_file_is_no_input() {
    let err = request::read_body_source(BodySource::File("/no/such/body.json")).unwrap_err();
    assert!(matches!(err, CliError::NoInput(_)));
    assert_eq!(err.diag().code, "no_input");
}

#[test]
fn malformed_set_is_usage() {
    let err = request::parse_set("no-equals-sign").unwrap_err();
    assert!(matches!(err, CliError::Usage(_)));
    assert_eq!(err.diag().code, "invalid_value");
}

#[test]
fn body_at_prefix_empty_path_is_usage() {
    let err = request::parse_body_source("@").unwrap_err();
    assert!(matches!(err, CliError::Usage(_)));
    assert_eq!(err.diag().code, "invalid_value");
}

#[test]
fn preview_redaction_hook_scrubs_body() {
    let spec = request::build_body(search_op(), &[("query", Some("secret-query".into()))]).unwrap();

    let preview = spec.preview_with_redactor(|body| {
        json!({
            "redacted": true,
            "keys": body.as_object().map(|m| m.keys().cloned().collect::<Vec<_>>()),
        })
    });

    assert_eq!(preview["request"]["body"]["redacted"], true);
    assert_eq!(preview["request"]["body"]["keys"], json!(["query"]));
    assert!(preview["request"]["body"].get("query").is_none());
}

#[test]
fn non_object_body_merge_is_usage() {
    let err = request::build_request(
        search_op(),
        &[("query", Some("q".into()))],
        RequestOverrides {
            body: Some(BodySource::Inline("[1,2,3]")),
            sets: &[],
        },
    )
    .unwrap_err();
    assert!(matches!(err, CliError::Usage(_)));
    assert_eq!(err.diag().code, "invalid_value");
}

#[test]
fn empty_dotted_set_path_segment_is_usage() {
    let mut body = json!({});
    let err = request::set_at_path(&mut body, "contents..text", json!(true)).unwrap_err();
    assert!(matches!(err, CliError::Usage(_)));
    assert_eq!(err.diag().code, "invalid_value");
}

#[test]
fn huge_array_index_is_rejected_without_allocating() {
    let mut body = json!({});
    let err = request::set_at_path(&mut body, "users.1000000000.name", json!("x")).unwrap_err();
    assert!(matches!(err, CliError::Usage(_)));
    assert_eq!(err.diag().code, "invalid_value");
}

#[test]
fn overflowing_array_index_is_rejected_without_panicking() {
    let mut body = json!({});
    let err =
        request::set_at_path(&mut body, "users.18446744073709551615.name", json!("x")).unwrap_err();
    assert!(matches!(err, CliError::Usage(_)));
    assert_eq!(err.diag().code, "invalid_value");
}
