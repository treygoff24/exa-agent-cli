use exa_agent_cli::registry;
use std::collections::BTreeSet;

#[test]
fn registry_builder_validator_variant_counts_stay_bounded() {
    let builders: BTreeSet<_> = registry::REGISTRY
        .iter()
        .filter_map(|op| op.body_builder.map(|builder| format!("{builder:?}")))
        .collect();
    let validators: BTreeSet<_> = registry::REGISTRY
        .iter()
        .flat_map(|op| {
            op.validators
                .iter()
                .map(|validator| format!("{validator:?}"))
        })
        .collect();

    // Target shape is 12 builders / 8 validators; this hard cap catches dispatch
    // refactors that recreate one-off variants. Future waves should pin a
    // grandfather allowlist when variants are intentionally introduced.
    assert!(
        builders.len() <= 20,
        "too many registry builder variants: {} {builders:?}",
        builders.len()
    );
    assert!(
        validators.len() <= 10,
        "too many registry validator variants: {} {validators:?}",
        validators.len()
    );
}
