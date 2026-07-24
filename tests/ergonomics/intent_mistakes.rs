use super::harness::{error_json, ok_json};

#[test]
fn search_text_maps_to_nested_contents_text() {
    let json = ok_json(&[
        "search",
        "rust async",
        "--text",
        "--dry-run",
        "--print-request",
        "--compact",
    ]);
    let body = &json["data"]["request"]["body"];
    assert_eq!(body["query"], "rust async");
    assert_eq!(body["contents"]["text"]["maxCharacters"], 1500);
}

#[test]
fn search_rejects_limit_with_num_results_suggestion() {
    let json = error_json(&[
        "search",
        "rust async",
        "--limit",
        "10",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["operation"]["path"], "/search");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results 10"));
}

#[test]
fn search_rejects_zero_limit_with_teaching_suggestion() {
    let json = error_json(&[
        "search",
        "rust async",
        "--limit",
        "0",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["operation"]["path"], "/search");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results 1"));
}

#[test]
fn search_rejects_negative_limit_with_operation_context() {
    let json = error_json(&[
        "search",
        "rust async",
        "--limit",
        "-1",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["operation"]["path"], "/search");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results 1"));
}

#[test]
fn search_rejects_bare_limit_with_operation_context() {
    let json = error_json(&["search", "rust async", "--limit", "--dry-run", "--compact"]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["operation"]["path"], "/search");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results 1"));
}

#[test]
fn search_rejects_count_with_num_results_suggestion() {
    let json = error_json(&[
        "search",
        "rust async",
        "--count",
        "8",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results 8"));
}

#[test]
fn search_rejects_zero_count_with_teaching_suggestion() {
    let json = error_json(&[
        "search",
        "rust async",
        "--count",
        "0",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["operation"]["path"], "/search");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results 1"));
}

#[test]
fn search_rejects_bare_count_with_operation_context() {
    let json = error_json(&["search", "rust async", "--count", "--dry-run", "--compact"]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["operation"]["path"], "/search");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results 1"));
}

#[test]
fn search_rejects_invalid_num_results_with_operation_context() {
    let json = error_json(&[
        "search",
        "rust async",
        "--num-results",
        "0",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_value");
    assert_eq!(json["operation"]["path"], "/search");
    assert_eq!(json["error"]["details"]["received"], "0");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results 1"));
}

#[test]
fn search_rejects_negative_num_results_with_operation_context() {
    let json = error_json(&[
        "search",
        "rust async",
        "--num-results",
        "-1",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_value");
    assert_eq!(json["operation"]["path"], "/search");
    assert_eq!(json["error"]["details"]["received"], "-1");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results 1"));
}

#[test]
fn search_rejects_set_num_results_out_of_range_with_operation_context() {
    let json = error_json(&[
        "search",
        "rust async",
        "--set",
        "numResults=0",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_value");
    assert_eq!(json["operation"]["path"], "/search");
    assert_eq!(json["error"]["details"]["received"], 0);
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results 1"));
}

#[test]
fn search_rejects_body_num_results_out_of_range_with_operation_context() {
    let json = error_json(&[
        "search",
        "rust async",
        "--body",
        r#"{"query":"rust async","numResults":101}"#,
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_value");
    assert_eq!(json["operation"]["path"], "/search");
    assert_eq!(json["error"]["details"]["received"], 101);
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results 100"));
}

#[test]
fn search_rejects_negative_count_with_operation_context() {
    let json = error_json(&[
        "search",
        "rust async",
        "--count",
        "-1",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["operation"]["path"], "/search");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results 1"));
}

#[test]
fn search_rejects_bare_num_results_with_operation_context() {
    let json = error_json(&[
        "search",
        "rust async",
        "--num-results",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_value");
    assert_eq!(json["operation"]["path"], "/search");
    assert_eq!(json["error"]["details"]["received"], "");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results 1"));
}

#[test]
fn search_rejects_all_with_num_results_suggestion() {
    let json = error_json(&["search", "rust async", "--all", "--dry-run", "--compact"]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results 100"));
}

#[test]
fn search_rejects_filter_with_typed_filter_suggestion() {
    let json = error_json(&[
        "search",
        "rust async",
        "--filter",
        "category=news",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    let suggestion = json["error"]["suggestedCommand"].as_str().unwrap();
    assert!(suggestion.contains("--category"));
    assert!(suggestion.contains("news"));
}

#[test]
fn search_filter_custom_category_suggests_category_flag() {
    let json = error_json(&[
        "search",
        "rust async",
        "--filter",
        "category=person",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    let suggestion = json["error"]["suggestedCommand"].as_str().unwrap();
    assert!(suggestion.contains("--category"));
    assert!(suggestion.contains("person"));
}

#[test]
fn search_filter_without_key_value_suggests_schema_discovery() {
    let json = error_json(&[
        "search",
        "rust async",
        "--filter",
        "news",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(
        json["error"]["suggestedCommand"],
        "exa-agent schema show search --compact"
    );
}

#[test]
fn search_filter_suggests_include_domain_flag() {
    let json = error_json(&[
        "search",
        "rust async",
        "--filter",
        "includeDomains=linkedin.com",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    let suggestion = json["error"]["suggestedCommand"].as_str().unwrap();
    assert!(suggestion.contains("--include-domain"));
    assert!(suggestion.contains("linkedin.com"));
    assert!(!suggestion.contains("--set"));
}

#[test]
fn search_filter_domain_shorthand_suggests_include_domain_flag() {
    let json = error_json(&[
        "search",
        "rust async",
        "--filter",
        "domain=example.com",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    let suggestion = json["error"]["suggestedCommand"].as_str().unwrap();
    assert!(suggestion.contains("--include-domain"));
    assert!(suggestion.contains("example.com"));
    assert!(!suggestion.contains("--set"));
}

#[test]
fn search_filter_suggests_published_date_flag() {
    let json = error_json(&[
        "search",
        "rust async",
        "--filter",
        "startPublishedDate=2026-01-01",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    let suggestion = json["error"]["suggestedCommand"].as_str().unwrap();
    assert!(suggestion.contains("--start-published-date"));
    assert!(suggestion.contains("2026-01-01"));
    assert!(!suggestion.contains("--set"));
}

#[test]
fn search_filter_suggests_exclude_domain_flag() {
    let json = error_json(&[
        "search",
        "rust async",
        "--filter",
        "excludeDomains=example.com",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    let suggestion = json["error"]["suggestedCommand"].as_str().unwrap();
    assert!(suggestion.contains("--exclude-domain"));
    assert!(suggestion.contains("example.com"));
    assert!(!suggestion.contains("--set"));
}

#[test]
fn search_filter_suggests_end_published_date_flag() {
    let json = error_json(&[
        "search",
        "rust async",
        "--filter",
        "endPublishedDate=2026-12-31",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    let suggestion = json["error"]["suggestedCommand"].as_str().unwrap();
    assert!(suggestion.contains("--end-published-date"));
    assert!(suggestion.contains("2026-12-31"));
    assert!(!suggestion.contains("--set"));
}

#[test]
fn search_rejects_retired_research_paper_category_with_did_you_mean() {
    let json = error_json(&[
        "search",
        "rust async",
        "--category",
        "research-paper",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_value");
    assert_eq!(json["error"]["details"]["didYouMean"], "publication");
    let suggestion = json["error"]["suggestedCommand"].as_str().unwrap();
    assert!(suggestion.contains("--category"));
    assert!(suggestion.contains("publication"));
}

#[test]
fn search_accepts_singular_person_as_custom_category_hint() {
    let json = ok_json(&[
        "search",
        "rust async",
        "--category",
        "person",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["data"]["request"]["body"]["category"], "person");
}

#[test]
fn search_accepts_unknown_category_without_misleading_default() {
    let json = ok_json(&[
        "search",
        "rust async",
        "--category",
        "pdf",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["data"]["request"]["body"]["category"], "pdf");
}

#[test]
fn search_accepts_and_canonicalizes_case_insensitive_category() {
    let json = ok_json(&[
        "search",
        "rust async",
        "--category",
        "Company",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["data"]["request"]["body"]["category"], "company");
}

#[test]
fn search_rejects_company_exclude_domain_combo() {
    let json = error_json(&[
        "search",
        "rust async",
        "--category",
        "company",
        "--exclude-domain",
        "example.com",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["error"]["details"]["category"], "company");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--category company"));
}

#[test]
fn search_rejects_people_published_date_combo() {
    let json = error_json(&[
        "search",
        "rust async",
        "--category",
        "people",
        "--start-published-date",
        "2026-01-01",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["error"]["details"]["category"], "people");
}

#[test]
fn search_accepts_people_include_domain_without_linkedin_restriction() {
    let ok = ok_json(&[
        "search",
        "rust async",
        "--category",
        "people",
        "--include-domain",
        "example.com",
        "--dry-run",
        "--compact",
    ]);
    let body = &ok["data"]["request"]["body"];
    assert_eq!(body["category"], "people");
    assert_eq!(body["includeDomains"], serde_json::json!(["example.com"]));
}

#[test]
fn contents_rejects_search_nested_contents_set() {
    let json = error_json(&[
        "contents",
        "https://exa.ai",
        "--set",
        "contents.text=true",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["operation"]["path"], "/contents");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--text"));
}

#[test]
fn contents_rejects_search_nested_contents_body() {
    let json = error_json(&[
        "contents",
        "https://exa.ai",
        "--body",
        r#"{"contents":{"text":true}}"#,
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["operation"]["path"], "/contents");
}

#[test]
fn websets_create_rejects_num_results_with_count_suggestion() {
    let json = error_json(&[
        "websets",
        "create",
        "--query",
        "AI startups",
        "--num-results",
        "10",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["operation"]["path"], "/websets/v0/websets");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--count 10"));
}

#[test]
fn websets_create_rejects_zero_num_results_with_count_suggestion() {
    let json = error_json(&[
        "websets",
        "create",
        "--query",
        "AI startups",
        "--num-results",
        "0",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["operation"]["path"], "/websets/v0/websets");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--count 1"));
}

#[test]
fn websets_create_rejects_negative_num_results_with_operation_context() {
    let json = error_json(&[
        "websets",
        "create",
        "--query",
        "AI startups",
        "--num-results",
        "-1",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["operation"]["path"], "/websets/v0/websets");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--count 1"));
}

#[test]
fn websets_create_rejects_bare_num_results_with_operation_context() {
    let json = error_json(&[
        "websets",
        "create",
        "--query",
        "AI startups",
        "--num-results",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["operation"]["path"], "/websets/v0/websets");
    assert!(json["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--count 1"));
}
