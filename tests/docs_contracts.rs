#[test]
fn commands_doc_matches_contents_mixed_outcome_exit_contract() {
    let commands = include_str!("../docs/v2/commands.md");
    assert!(commands.contains("batch with mixed outcomes exits 0"));
    assert!(!commands.contains("batch with mixed outcomes exits 10"));
}

#[test]
fn architecture_doc_rejects_boolean_values_for_optional_text() {
    let architecture = include_str!("../docs/v2/architecture.md");
    assert!(architecture.contains("`--text false`, `--text true`, and `--text 0` reject"));
    assert!(!architecture
        .contains("`--text[=N|full]` normalizes to `text.maxCharacters`, `true`, or `false`"));
}
