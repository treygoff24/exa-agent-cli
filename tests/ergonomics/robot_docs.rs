use std::collections::BTreeSet;

use clap::Parser;
use exa_agent_cli::cli::Cli;
use exa_agent_cli::registry;

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

fn split_shell_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    for ch in input.chars() {
        match ch {
            '\'' if !in_double => {
                in_single = !in_single;
            }
            '"' if !in_single => {
                in_double = !in_double;
            }
            c if c.is_whitespace() && !in_single && !in_double => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn registry_api_paths() -> BTreeSet<String> {
    registry::REGISTRY
        .iter()
        .map(|op| op.api_path.to_string())
        .collect()
}

#[test]
fn robot_docs_examples_parse_and_build_offline() {
    let docs = ok_json(&["robot-docs", "examples", "--task", "search", "--compact"]);
    assert_eq!(docs["section"], "examples");
    let api_paths = registry_api_paths();
    for example in docs["examples"].as_array().expect("examples array") {
        let line = example.as_str().expect("example string");
        assert!(
            line.starts_with("exa-agent "),
            "example must start with exa-agent: {line}"
        );
        let args = split_shell_words(&line["exa-agent ".len()..]);
        let argv: Vec<String> = std::iter::once("exa-agent".to_string())
            .chain(args.clone())
            .collect();
        Cli::try_parse_from(&argv)
            .unwrap_or_else(|e| panic!("example failed to parse `{line}`: {e}"));

        if args.first().map(String::as_str) == Some("raw") {
            let method = args.get(1).map(String::as_str).expect("raw method");
            let path = args.get(2).map(String::as_str).expect("raw path");
            assert!(
                api_paths.contains(path),
                "raw example path `{path}` ({method}) is not in the registry"
            );
        }

        let run_args: Vec<&str> = args.iter().map(String::as_str).collect();
        let run = super::harness::run_cli(&run_args);
        assert_eq!(
            run.exit_code, 0,
            "example failed to run offline `{line}`\nstdout:\n{}\nstderr:\n{}",
            run.stdout, run.stderr
        );
    }
}
