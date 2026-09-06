//! Spec-drift guards for values the CLI hardcodes that the vendored OpenAPI document already
//! carries. Without these, an API refresh can change a beta token or an enum upstream while every
//! other test stays green and the CLI keeps sending the retired value.

use exa_agent_cli::{batches, registry, AGENT_MAX_EFFORT_BETA};
use serde_json::Value;
use std::fs;

const SPEC: &str = "openapi/exa-openapi.json";

fn spec() -> Value {
    let raw = fs::read_to_string(SPEC).unwrap_or_else(|err| panic!("read {SPEC}: {err}"));
    serde_json::from_str(&raw).unwrap_or_else(|err| panic!("parse {SPEC}: {err}"))
}

fn operation<'a>(spec: &'a Value, path: &str, method: &str) -> &'a Value {
    spec.pointer(&format!(
        "/paths/{}/{method}",
        path.replace('~', "~0").replace('/', "~1")
    ))
    .unwrap_or_else(|| panic!("{method} {path} is missing from {SPEC}"))
}

fn string_enum(spec: &Value, schema: &str, property: &str) -> Vec<String> {
    spec.pointer(&format!(
        "/components/schemas/{schema}/properties/{property}/enum"
    ))
    .and_then(Value::as_array)
    .unwrap_or_else(|| panic!("{schema}.{property} has no enum in {SPEC}"))
    .iter()
    .map(|value| {
        value
            .as_str()
            .unwrap_or_else(|| panic!("{schema}.{property} enum member is not a string"))
            .to_string()
    })
    .collect()
}

#[test]
fn batch_beta_token_matches_the_spec_flag() {
    let spec = spec();
    let flag = operation(&spec, "/batches", "post")["x-exa-beta-flag"]
        .as_str()
        .expect("POST /batches carries x-exa-beta-flag");
    assert_eq!(
        batches::BETA_TOKEN,
        flag,
        "batches::BETA_TOKEN drifted from x-exa-beta-flag on POST /batches"
    );
}

/// `POST /agent/runs/{id}/stop` carries no `x-exa-beta-flag`; the spec names its token in prose
/// only, so the assertion reads the description.
#[test]
fn agent_stop_beta_token_matches_the_spec_description() {
    let spec = spec();
    let description = operation(&spec, "/agent/runs/{id}/stop", "post")["description"]
        .as_str()
        .expect("stop operation has a description");
    let (_, after) = description
        .split_once("Exa-Beta: ")
        .expect("stop description names the Exa-Beta header");
    let token = after
        .split('`')
        .next()
        .expect("stop description terminates the token")
        .trim();
    assert_eq!(
        AGENT_MAX_EFFORT_BETA, token,
        "AGENT_MAX_EFFORT_BETA drifted from the token named in the stop description"
    );
}

#[test]
fn batch_item_urls_match_the_spec_enum() {
    let spec = spec();
    assert_eq!(
        batches::ALLOWED_ITEM_URLS.to_vec(),
        string_enum(&spec, "BatchRequestItem", "url"),
        "batches::ALLOWED_ITEM_URLS drifted from BatchRequestItem.url"
    );
}

#[test]
fn answer_model_enum_matches_the_spec_enum() {
    let spec = spec();
    let op = registry::lookup_by_command("answer").expect("answer is in the registry");
    let field = op
        .fields
        .iter()
        .find(|field| field.flag == "model")
        .expect("answer has a model field");
    assert_eq!(
        registry::field_enum_values(op, field).to_vec(),
        string_enum(&spec, "AnswerRequest", "model"),
        "the answer model enum drifted from AnswerRequest.model"
    );
}

/// The `--model` parser is built from the registry enum, so a spec refresh that adds a model
/// reaches Clap without a second edit. This binds the parser to the same list the test above pins.
#[test]
fn answer_model_flag_accepts_exactly_the_registry_enum() {
    let op = registry::lookup_by_command("answer").expect("answer is in the registry");
    let field = op
        .fields
        .iter()
        .find(|field| field.flag == "model")
        .expect("answer has a model field");
    let expected = registry::field_enum_values(op, field);
    assert!(!expected.is_empty(), "answer model enum must not be empty");

    let help = std::process::Command::new(env!("CARGO_BIN_EXE_exa-agent"))
        .args(["answer", "--help"])
        .env("EXA_AGENT_NO_NETWORK", "1")
        .output()
        .expect("run answer --help");
    let rendered = String::from_utf8_lossy(&help.stdout);
    // Clap wraps the possible-values list across lines, so read from the `--model` entry to the
    // closing bracket and normalize the whitespace it inserted.
    let after_flag = rendered
        .split_once("--model ")
        .unwrap_or_else(|| panic!("--model missing from help:\n{rendered}"))
        .1;
    let listed: Vec<String> = after_flag
        .split_once("[possible values:")
        .unwrap_or_else(|| panic!("--model has no possible values:\n{rendered}"))
        .1
        .split_once(']')
        .expect("possible values are closed")
        .0
        .split(',')
        .map(|value| value.split_whitespace().collect::<String>())
        .filter(|value| !value.is_empty())
        .collect();
    assert_eq!(listed, expected.to_vec(), "in help:\n{rendered}");
}
