use super::harness::ok_json;

const SKILL: &str = include_str!("../../work/generated/exa-agent-cli/SKILL.md");

#[test]
fn generated_skill_is_the_robot_docs_golden() {
    let guide = ok_json(&["robot-docs", "guide", "--compact"]);
    for line in guide["guidance"].as_array().expect("guidance array") {
        let line = line.as_str().expect("guidance string");
        assert!(SKILL.contains(line), "skill missing guidance: {line}");
    }

    let examples = ok_json(&["robot-docs", "examples", "--task", "search", "--compact"]);
    for line in examples["examples"].as_array().expect("examples array") {
        let line = line.as_str().expect("example string");
        assert!(SKILL.contains(line), "skill missing example: {line}");
    }
    assert!(SKILL.contains("https://docs.exa.ai"));
    assert!(SKILL.contains("no failures and no returned content"));
}
