#[test]
fn commands_doc_matches_contents_mixed_outcome_exit_contract() {
    let commands = include_str!("../docs/v2/commands.md");
    assert!(commands.contains("batch with mixed outcomes exits 0"));
    assert!(!commands.contains("batch with mixed outcomes exits 10"));
}

/// Keep the §5.1 error-code table and the published count synchronized with the binary.
#[test]
fn contracts_error_dictionary_matches_the_binary() {
    let contracts = include_str!("../docs/v2/contracts.md");
    let documented: std::collections::BTreeSet<String> = contracts
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|line| line.split_once("` | "))
        .filter(|(_, rest)| rest.contains(" (") && rest.contains(") | "))
        .map(|(code, _)| code.to_string())
        .filter(|code| code != "code")
        .collect();
    let implemented: std::collections::BTreeSet<String> = exa_agent_cli::error::error_code_specs()
        .keys()
        .map(|code| code.to_string())
        .collect();

    let missing: Vec<_> = implemented.difference(&documented).collect();
    let extra: Vec<_> = documented.difference(&implemented).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "contracts.md §5.1 is out of sync with error_code_specs(); missing from docs: {missing:?}; documented but not implemented: {extra:?}"
    );

    let agents = include_str!("../AGENTS.md");
    assert!(
        agents.contains(&format!("{} codes map onto", implemented.len())),
        "AGENTS.md publishes a stale error-code count; expected {}",
        implemented.len()
    );
}

#[test]
fn architecture_doc_rejects_boolean_values_for_optional_text() {
    let architecture = include_str!("../docs/v2/architecture.md");
    assert!(architecture.contains("`--text false`, `--text true`, and `--text 0` reject"));
    assert!(!architecture
        .contains("`--text[=N|full]` normalizes to `text.maxCharacters`, `true`, or `false`"));
}

#[test]
fn contracts_document_successful_raw_payment_receipt_metadata() {
    let contracts = include_str!("../docs/v2/contracts.md");
    assert!(contracts.contains(
        "`payment` is a top-level field only on successful signed raw payment responses"
    ));
    assert!(
        contracts.contains("It is inserted after `dataTruncated` and is never nested under `data`")
    );
    assert!(contracts.contains(
        "case-insensitively match `payment-response`, `payment-receipt`, `x-payment-response`, or `x-payment-receipt`"
    ));
    assert!(contracts.contains("with `name` casing preserved as received"));
    let redacted = exa_agent_cli::redaction::REDACTED;
    assert!(contracts.contains(&format!(
        "all non-payment raw bytes remain exact, while signed payment raw output is exact except exact submitted payment credential echoes are replaced with `{redacted}`"
    )));
    assert!(contracts.contains(&format!(
        "Under `--raw`, no envelope or `payment` metadata is added; output is exact except those echoes are replaced with `{redacted}`"
    )));
    assert!(contracts.contains(
        "Successful signed payment responses redact exact submitted payment credential echoes before any output"
    ));
}
