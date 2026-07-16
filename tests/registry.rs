//! Registry-consistency invariants (arch §3). These pin the generated table against the
//! contracts so the hand-written surface and the codegen can't silently drift.

use clap::CommandFactory;
use exa_agent_cli::cli::Cli;
use exa_agent_cli::error::error_code_dictionary;
use exa_agent_cli::output::envelope::capabilities;
use exa_agent_cli::registry::{self, Method};

/// The `idempotency_sensitive` set equals the contracts §7 create-POST list, exactly (D7).
#[test]
fn registry_idempotency_matches_contract_create_list() {
    let expected = [
        "create-api-key",
        "createAgentRun",
        "createMonitor",
        "ResearchController_createResearch",
        "imports-create",
        "monitors-create",
        "webhooks-create",
        "websets-create",
        "websets-enrichments-create",
        "websets-searches-create",
    ];
    let mut expected: Vec<&str> = expected.to_vec();
    expected.sort_unstable();
    assert_eq!(registry::idempotency_sensitive_ids(), expected);
}

/// Every operation has a non-empty CLI path that resolves back to itself.
#[test]
fn registry_cli_paths_resolve() {
    for op in registry::REGISTRY {
        assert!(
            !op.cli_path.is_empty(),
            "{} has empty cli_path",
            op.operation_id
        );
        let resolved = registry::lookup_by_command(&op.command())
            .unwrap_or_else(|| panic!("{} did not resolve", op.command()));
        assert_eq!(resolved.operation_id, op.operation_id);
    }
}

/// `destructive` ⊇ every DELETE and every `dangerous` op (the D27 blast-radius triad).
#[test]
fn registry_destructive_covers_deletes_and_dangerous() {
    for op in registry::REGISTRY {
        if op.method == Method::Delete || op.dangerous {
            assert!(op.destructive(), "{} should be destructive", op.command());
        }
    }
}

/// `capabilities --json` enumerates every registry operation.
#[test]
fn capabilities_covers_every_operation() {
    let caps = capabilities();
    assert_eq!(
        caps["commandCount"].as_u64().unwrap() as usize,
        registry::REGISTRY.len()
    );
    assert_eq!(caps["spec"]["title"], "Exa Public API");
    assert_eq!(caps["spec"]["version"], "2.0.0");
    assert!(caps.get("env").is_none(), "env is not a capabilities field");
    assert_eq!(caps["errorCodes"]["not_authenticated"]["category"], "auth");
    assert_eq!(caps["errorCodes"]["not_authenticated"]["exit"], 2);
    assert_eq!(caps["errorCodes"]["not_authenticated"]["retryable"], false);
    assert_eq!(caps["errorCodes"]["partial_batch"]["category"], "partial");
    assert_eq!(caps["errorCodes"]["upstream_malformed"]["exit"], 5);
    assert_eq!(caps["errorCodes"]["concurrency_limit"]["exit"], 6);
    assert_eq!(caps["errorCodes"]["idempotency_conflict"]["exit"], 8);
    assert!(caps["errorCodes"].get("partial_success").is_none());
    assert_eq!(caps["doctor"]["exitCodes"]["1"], "findings");
}

/// Admin operations live in the service namespace and nowhere else (D4).
#[test]
fn admin_ops_are_service_namespace() {
    for op in registry::REGISTRY {
        let is_admin = op.cli_path.first() == Some(&"admin");
        let is_service = op.namespace == registry::Namespace::Service;
        assert_eq!(
            is_admin,
            is_service,
            "{} namespace/admin mismatch",
            op.command()
        );
    }
}

/// The error-code dictionary is non-empty and every entry is a stable snake_case string
/// (the published §5.1 vocabulary that agents branch on).
#[test]
fn error_code_dictionary_is_well_formed() {
    let dict = error_code_dictionary();
    assert!(!dict.is_empty());
    for expected in [
        "upstream_malformed",
        "concurrency_limit",
        "idempotency_conflict",
        "partial_batch",
    ] {
        assert!(dict.contains_key(expected), "missing {expected}");
    }
    for code in dict.keys() {
        assert!(
            code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "error code {code:?} is not snake_case"
        );
    }
}

fn clap_leaf<'a>(command: &'a clap::Command, path: &[&str]) -> &'a clap::Command {
    path.iter().fold(command, |command, segment| {
        command
            .find_subcommand(segment)
            .unwrap_or_else(|| panic!("missing clap command {segment}"))
    })
}

fn assert_registry_inputs_match_clap(command_path: &str) {
    let op = registry::lookup_by_command(command_path).expect("registry entry");
    let command = Cli::command();
    let segments: Vec<_> = command_path.split(' ').collect();
    let leaf = clap_leaf(&command, &segments);
    for field in op.fields {
        let input_kind = field.input_kind.unwrap_or_else(|| {
            panic!("{} field {} lacks input metadata", op.command(), field.flag)
        });
        let arg = match input_kind {
            registry::InputKind::Flag => {
                let arg = leaf
                    .get_arguments()
                    .find(|arg| arg.get_long() == Some(field.flag));
                let long = arg.and_then(|arg| arg.get_long()).expect("clap long flag");
                assert!(flag_input_name_matches_clap_long(
                    field.input_name.expect("flag input name"),
                    long
                ));
                arg
            }
            registry::InputKind::Argument => leaf.get_arguments().find(|arg| {
                arg.get_long().is_none()
                    && arg
                        .get_value_names()
                        .is_some_and(|names| names == [field.input_name.expect("input name")])
            }),
        }
        .unwrap_or_else(|| panic!("{} missing clap input for {}", op.command(), field.flag));

        if let Some(arity) = field.arity {
            let clap_arity = arg
                .get_num_args()
                .unwrap_or(clap::builder::ValueRange::SINGLE);
            assert_eq!(clap_arity.min_values(), arity.min, "{}", field.flag);
            assert_eq!(
                clap_arity.max_values(),
                arity.max.unwrap_or(usize::MAX),
                "{}",
                field.flag
            );
        }
        if let Some(value_name) = field.value_name.or_else(|| {
            (input_kind == registry::InputKind::Argument)
                .then_some(field.input_name.expect("argument input name"))
        }) {
            assert_eq!(
                arg.get_value_names().expect("clap value name"),
                [value_name],
                "{}",
                field.flag
            );
        }
    }
}

#[test]
fn contents_registry_input_metadata_matches_clap() {
    let op = registry::lookup_by_command("contents").expect("contents registry entry");
    assert!(op.fields.iter().all(|field| field.input_kind.is_some()));
    assert!(op.fields.iter().all(|field| field.input_name.is_some()));
    assert!(op.fields.iter().all(|field| field.arity.is_some()));
    assert_eq!(
        op.fields
            .iter()
            .find(|field| field.flag == "text")
            .and_then(|field| field.input_range),
        Some((1, 10_000))
    );
    assert_registry_inputs_match_clap("contents");
}

#[test]
fn modeled_core_inputs_are_non_vacuously_parity_checked_against_clap() {
    for command in ["search", "answer", "context", "similar", "contents"] {
        assert_registry_inputs_match_clap(command);
    }
}

fn flag_input_name_matches_clap_long(input_name: &str, long: &str) -> bool {
    input_name == format!("--{long}")
}

#[test]
fn flag_input_name_parity_detects_deliberate_registry_drift() {
    let search = registry::lookup_by_command("search").expect("search registry entry");
    let text = search
        .fields
        .iter()
        .find(|field| field.flag == "text")
        .expect("search text field");
    assert!(flag_input_name_matches_clap_long(
        text.input_name.expect("generator flag name"),
        "text"
    ));
    assert!(!flag_input_name_matches_clap_long("--wrong", "text"));
}
