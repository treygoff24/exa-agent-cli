//! Request-builder merge semantics (arch §4 / D39).

use exa_agent_cli::error::CliError;
use exa_agent_cli::registry;
use exa_agent_cli::request::{self, BodySource, RequestOverrides};
use serde_json::json;
use std::fs;

fn search_op() -> &'static registry::OperationDef {
    registry::lookup_by_segments(&["search"]).expect("search op")
}

fn answer_op() -> &'static registry::OperationDef {
    registry::lookup_by_segments(&["answer"]).expect("answer op")
}

fn contents_op() -> &'static registry::OperationDef {
    registry::lookup_by_segments(&["contents"]).expect("contents op")
}

fn context_op() -> &'static registry::OperationDef {
    registry::lookup_by_segments(&["context"]).expect("context op")
}

fn similar_op() -> &'static registry::OperationDef {
    registry::lookup_by_segments(&["similar"]).expect("similar op")
}

#[test]
fn search_core_fields_map_and_overrides_keep_precedence() {
    let spec = request::build_request(
        search_op(),
        &[
            ("query", Some("typed query".into())),
            ("num-results", Some("5".into())),
            ("type", Some("fast".into())),
            ("category", Some("news".into())),
        ],
        RequestOverrides {
            body: Some(BodySource::Inline(r#"{"numResults":10,"type":"deep"}"#)),
            sets: &["category=research paper".into()],
        },
    )
    .unwrap();

    assert_eq!(spec.body["query"], "typed query");
    assert_eq!(spec.body["numResults"], 10);
    assert_eq!(spec.body["type"], "deep");
    assert_eq!(spec.body["category"], "research paper");
}

#[test]
fn answer_fields_map_schema_and_overrides_keep_precedence() {
    let schema = json!({"type":"object","properties":{"answer":{"type":"string"}}});
    let spec = request::build_request(
        answer_op(),
        &[
            ("question", Some("typed question".into())),
            ("text", Some("true".into())),
            ("stream", Some("true".into())),
            ("output-schema", Some(schema.to_string())),
        ],
        RequestOverrides {
            body: Some(BodySource::Inline(
                r#"{"text":false,"outputSchema":{"type":"string"}}"#,
            )),
            sets: &["stream=false".into()],
        },
    )
    .unwrap();

    assert_eq!(spec.body["query"], "typed question");
    assert_eq!(spec.body["text"], false);
    assert_eq!(spec.body["stream"], false);
    assert_eq!(
        spec.body["outputSchema"],
        json!({"type":"string","properties":{"answer":{"type":"string"}}})
    );
}

#[test]
fn output_schema_at_file_reads_json_for_typed_builders() {
    let path = std::env::temp_dir().join(format!(
        "exa-agent-output-schema-{}.json",
        std::process::id()
    ));
    fs::write(&path, r#"{"type":"object","required":["answer"]}"#).unwrap();

    let schema =
        request::read_json_value_arg(&format!("@{}", path.display()), "output-schema").unwrap();

    fs::remove_file(path).unwrap();
    assert_eq!(schema, json!({"type":"object","required":["answer"]}));
}

#[test]
fn context_fields_map_tokens_and_precedence() {
    let spec = request::build_request(
        context_op(),
        &[
            ("query", Some("rust async".into())),
            ("tokens", Some("1000".into())),
        ],
        RequestOverrides {
            body: Some(BodySource::Inline(r#"{"tokensNum":2000}"#)),
            sets: &["tokensNum=3000".into()],
        },
    )
    .unwrap();

    assert_eq!(spec.body["query"], "rust async");
    assert_eq!(spec.body["tokensNum"], 3000);
}

#[test]
fn similar_fields_map_core_flags() {
    let spec = request::build_body(
        similar_op(),
        &[
            ("url", Some("https://exa.ai".into())),
            ("num-results", Some("7".into())),
            ("exclude-source-domain", Some("true".into())),
            ("category", Some("company".into())),
        ],
    )
    .unwrap();

    assert_eq!(
        spec.body,
        json!({
            "url": "https://exa.ai",
            "numResults": 7,
            "excludeSourceDomain": true,
            "category": "company"
        })
    );
}

#[test]
fn body_deep_merges_over_named_flags() {
    let spec = request::build_request(
        search_op(),
        &[
            ("query", Some("hello".into())),
            ("num-results", Some("5".into())),
            ("type", Some("fast".into())),
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
    assert_eq!(spec.body["type"], "fast");
    assert_eq!(spec.body["category"], "news");
    assert_eq!(spec.body["contents"]["text"], true);
}

#[test]
fn contents_urls_build_body_from_registry_metadata() {
    let urls = vec![
        "https://exa.ai/docs".to_string(),
        "https://docs.exa.ai/reference/search".to_string(),
    ];
    let spec = request::build_body(
        contents_op(),
        &[("urls", Some(request::encode_str_array(&urls)))],
    )
    .unwrap();

    assert_eq!(
        spec.body,
        json!({
            "urls": [
                "https://exa.ai/docs",
                "https://docs.exa.ai/reference/search"
            ]
        })
    );
}

#[test]
fn contents_ids_build_body_from_registry_metadata() {
    let ids = vec!["doc_1".to_string(), "doc_2".to_string()];
    let spec = request::build_body(
        contents_op(),
        &[("ids", Some(request::encode_str_array(&ids)))],
    )
    .unwrap();

    assert_eq!(spec.body, json!({ "ids": ["doc_1", "doc_2"] }));
}

#[test]
fn contents_chunk_size_is_local_only_not_request_body() {
    assert!(contents_op()
        .fields
        .iter()
        .all(|field| field.flag != "chunk-size" && field.body_path != "chunkSize"));

    let urls = vec!["https://exa.ai/docs".to_string()];
    let spec = request::build_body(
        contents_op(),
        &[
            ("urls", Some(request::encode_str_array(&urls))),
            ("chunk-size", Some("25".into())),
        ],
    )
    .unwrap();

    assert_eq!(spec.body, json!({ "urls": ["https://exa.ai/docs"] }));
    assert!(spec.body.get("chunkSize").is_none());
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
