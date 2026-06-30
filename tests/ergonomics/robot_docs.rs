use std::collections::BTreeSet;

use super::harness::ok_json;

fn commands(value: &serde_json::Value) -> BTreeSet<String> {
    value["commands"]
        .as_array()
        .expect("commands array")
        .iter()
        .map(|entry| {
            entry["command"]
                .as_str()
                .or_else(|| entry["path"].as_str())
                .expect("command/path")
                .to_string()
        })
        .collect()
}

fn error_codes(value: &serde_json::Value) -> BTreeSet<String> {
    value["errorCodes"]
        .as_object()
        .expect("errorCodes object")
        .keys()
        .cloned()
        .collect()
}

#[test]
fn robot_docs_commands_match_capabilities() {
    let caps = ok_json(&["capabilities", "--compact"]);
    let docs = ok_json(&["robot-docs", "commands", "--compact"]);
    assert_eq!(docs["schema"], "exa.cli.robot_docs.v1");
    assert_eq!(commands(&docs), commands(&caps));
}

#[test]
fn robot_docs_errors_match_capabilities() {
    let caps = ok_json(&["capabilities", "--compact"]);
    let docs = ok_json(&["robot-docs", "errors", "--compact"]);
    assert_eq!(docs["schema"], "exa.cli.robot_docs.v1");
    assert_eq!(error_codes(&docs), error_codes(&caps));
}

#[test]
fn robot_docs_guide_mentions_core_agent_surfaces() {
    let guide = ok_json(&["robot-docs", "guide", "--compact"]);
    assert_eq!(guide["section"], "guide");
    let text = guide["guidance"].to_string();
    for needle in [
        "suggestedCommand",
        "--dry-run",
        "--print-request",
        "--num-results",
        "robot-docs errors",
    ] {
        assert!(text.contains(needle), "guide missing {needle}: {text}");
    }
}
