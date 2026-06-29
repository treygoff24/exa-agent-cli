//! Registry-consistency invariants (arch §3). These pin the generated table against the
//! contracts so the hand-written surface and the codegen can't silently drift.

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
    for code in dict.keys() {
        assert!(
            code.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
            "error code {code:?} is not snake_case"
        );
    }
}
