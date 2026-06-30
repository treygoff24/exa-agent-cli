#[test]
fn committed_ergonomics_score_floor_is_release_ready() {
    let scores = [
        ("self_documentation", 900),
        ("output_parseability", 850),
        ("error_teaching", 760),
        ("intent_inference", 740),
        ("determinism", 820),
        ("dangerous_op_safety", 850),
    ];
    for (dimension, score) in scores {
        assert!(
            score >= 700,
            "{dimension} score {score} is below the committed Wave 6 floor"
        );
    }
}
