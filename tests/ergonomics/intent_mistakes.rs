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
fn search_plural_include_domains_points_to_the_canonical_flag_without_echoing_credentials() {
    let json = error_json(&[
        "--api-key",
        "user-secret-value",
        "search",
        "rust async",
        "--include-domains",
        "example.com",
        "--json",
    ]);
    assert_eq!(json["error"]["code"], "unknown_flag");
    assert_eq!(
        json["error"]["suggestedCommand"],
        "exa-agent search 'rust async' --include-domain example.com --json"
    );
    assert!(!json.to_string().contains("user-secret-value"));
}

#[test]
fn corrected_commands_omit_secret_bearing_raw_inputs() {
    for (args, canary, omitted_flag) in [
        (
            vec![
                "--header",
                "Authorization: Bearer HEADER_SECRET_CANARY",
                "search",
                "--query",
                "rust",
                "--json",
            ],
            "HEADER_SECRET_CANARY",
            "--header",
        ),
        (
            vec![
                "--header=x-api-key: EQUALS_HEADER_SECRET_CANARY",
                "search",
                "--query",
                "rust",
                "--json",
            ],
            "EQUALS_HEADER_SECRET_CANARY",
            "--header",
        ),
        (
            vec![
                "search",
                "--query",
                "rust",
                "--set",
                "contents.apiKey=SET_SECRET_CANARY",
                "--json",
            ],
            "SET_SECRET_CANARY",
            "--set",
        ),
        (
            vec![
                "search",
                "--query",
                "rust",
                "--body",
                r#"{"apiKey":"BODY_SECRET_CANARY"}"#,
                "--json",
            ],
            "BODY_SECRET_CANARY",
            "--body",
        ),
        (
            vec![
                "--base-url",
                "https://user:BASE_URL_SECRET_CANARY@example.com",
                "search",
                "--query",
                "rust",
                "--json",
            ],
            "BASE_URL_SECRET_CANARY",
            "--base-url",
        ),
        (
            vec![
                "--base-url=https://user:EQUALS_BASE_URL_SECRET_CANARY@example.com",
                "search",
                "--query",
                "rust",
                "--json",
            ],
            "EQUALS_BASE_URL_SECRET_CANARY",
            "--base-url",
        ),
        (
            vec![
                "--base-url",
                "https://example.com/not-an-origin",
                "search",
                "--query",
                "rust",
                "--json",
            ],
            "not-an-origin",
            "--base-url",
        ),
        (
            vec![
                "--base-url=https://example.com?debug=BASE_URL_QUERY_CANARY",
                "search",
                "--query",
                "rust",
                "--json",
            ],
            "BASE_URL_QUERY_CANARY",
            "--base-url",
        ),
        (
            vec![
                "--base-url=https://example.com#BASE_URL_FRAGMENT_CANARY",
                "search",
                "--query",
                "rust",
                "--json",
            ],
            "BASE_URL_FRAGMENT_CANARY",
            "--base-url",
        ),
        (
            vec![
                "--base-url",
                "http://example.com",
                "search",
                "--query",
                "rust",
                "--json",
            ],
            "http://example.com",
            "--base-url",
        ),
        (
            vec![
                "--base-url=https://example.com%2FBASE_URL_PERCENT_CANARY",
                "search",
                "--query",
                "rust",
                "--json",
            ],
            "BASE_URL_PERCENT_CANARY",
            "--base-url",
        ),
        (
            vec![
                "--base-url",
                r"https://example.com\BASE_URL_BACKSLASH_CANARY",
                "search",
                "--query",
                "rust",
                "--json",
            ],
            "BASE_URL_BACKSLASH_CANARY",
            "--base-url",
        ),
        (
            vec![
                "--base-url",
                "https://example.com BASE_URL_SPACE_CANARY",
                "search",
                "--query",
                "rust",
                "--json",
            ],
            "BASE_URL_SPACE_CANARY",
            "--base-url",
        ),
        (
            vec![
                "--base-url=https://example.com:99999",
                "search",
                "--query",
                "rust",
                "--json",
            ],
            "99999",
            "--base-url",
        ),
    ] {
        let json = error_json(&args);
        assert_eq!(
            json["error"]["suggestedCommand"],
            "exa-agent search rust --json"
        );
        assert_eq!(
            json["error"]["details"]["omittedFlags"],
            serde_json::json!([omitted_flag])
        );
        assert!(!json.to_string().contains(canary));
    }
}

#[test]
fn corrected_commands_preserve_safe_base_url() {
    for (args, expected) in [
        (
            vec![
                "--base-url",
                "http://127.0.0.1:9",
                "search",
                "--query",
                "rust",
                "--json",
            ],
            "exa-agent --base-url http://127.0.0.1:9 search rust --json",
        ),
        (
            vec![
                "--base-url=https://example.com",
                "search",
                "--query",
                "rust",
                "--json",
            ],
            "exa-agent --base-url=https://example.com search rust --json",
        ),
    ] {
        let json = error_json(&args);
        assert_eq!(json["error"]["suggestedCommand"], expected);
        assert!(json["error"]["details"].get("omittedFlags").is_none());
    }
}

#[test]
fn corrected_commands_report_all_omitted_semantic_flags_without_values() {
    let json = error_json(&[
        "--api-key",
        "API_SECRET_CANARY",
        "--header",
        "x-extra: HEADER_SECRET_CANARY",
        "search",
        "--query",
        "rust",
        "--set",
        "contents.token=SET_SECRET_CANARY",
        "--body",
        r#"{"token":"BODY_SECRET_CANARY"}"#,
        "--json",
    ]);
    assert_eq!(
        json["error"]["details"]["omittedFlags"],
        serde_json::json!(["--api-key", "--header", "--set", "--body"])
    );
    let rendered = json.to_string();
    for canary in [
        "API_SECRET_CANARY",
        "HEADER_SECRET_CANARY",
        "SET_SECRET_CANARY",
        "BODY_SECRET_CANARY",
    ] {
        assert!(!rendered.contains(canary));
    }
}

#[test]
fn non_recovery_clap_errors_do_not_report_omitted_flags() {
    let json = error_json(&[
        "--api-key",
        "API_SECRET_CANARY",
        "search",
        "rust",
        "--category",
        "companys",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_value");
    assert!(json["error"]["details"].get("omittedFlags").is_none());
    assert!(!json.to_string().contains("API_SECRET_CANARY"));
}

#[test]
fn search_query_flag_points_to_the_positional_query() {
    let json = error_json(&["search", "--query", "rust async", "--json"]);
    assert_eq!(json["error"]["code"], "unknown_flag");
    assert_eq!(
        json["error"]["suggestedCommand"],
        "exa-agent search 'rust async' --json"
    );
    assert!(json["error"]["details"].get("didYouMean").is_none());
}

#[test]
fn search_query_flag_joins_shell_split_words() {
    let json = error_json(&["search", "--query", "rust", "async", "--json"]);
    assert_eq!(
        json["error"]["suggestedCommand"],
        "exa-agent search 'rust async' --json"
    );
}

#[test]
fn search_query_with_an_existing_positional_falls_back_to_help() {
    let json = error_json(&["search", "existing", "--query", "rust", "async", "--json"]);
    assert_eq!(json["error"]["suggestedCommand"], "exa-agent search --help");
}

#[test]
fn search_query_without_a_value_falls_back_to_help() {
    let json = error_json(&["search", "--query", "--json"]);
    assert_eq!(json["error"]["code"], "unknown_flag");
    assert_eq!(json["error"]["suggestedCommand"], "exa-agent search --help");
}

#[test]
fn search_contents_numeric_points_to_text() {
    let json = error_json(&["search", "rust async", "--contents", "1200", "--json"]);
    assert_eq!(json["error"]["code"], "unknown_flag");
    assert_eq!(
        json["error"]["suggestedCommand"],
        "exa-agent search 'rust async' --text 1200 --json"
    );
    assert!(json["error"]["details"].get("didYouMean").is_none());
}

#[test]
fn search_contents_numeric_joins_a_shell_split_query() {
    let json = error_json(&["search", "rust", "async", "--contents", "1200", "--json"]);
    assert_eq!(
        json["error"]["suggestedCommand"],
        "exa-agent search 'rust async' --text 1200 --json"
    );
}

#[test]
fn search_contents_split_query_preserves_global_option_ordering() {
    for args in [
        vec!["--json", "search", "rust", "async", "--contents", "1200"],
        vec!["search", "--json", "rust", "async", "--contents", "1200"],
    ] {
        let json = error_json(&args);
        assert_eq!(
            json["error"]["suggestedCommand"],
            if args[0] == "--json" {
                "exa-agent --json search 'rust async' --text 1200"
            } else {
                "exa-agent search --json 'rust async' --text 1200"
            }
        );
    }
}

#[test]
fn search_contents_split_query_preserves_preceding_typed_flag_values() {
    let json = error_json(&[
        "search",
        "--include-domain",
        "exa.ai",
        "rust",
        "async",
        "--contents",
        "1200",
        "--json",
    ]);
    assert_eq!(
        json["error"]["suggestedCommand"],
        "exa-agent search --include-domain exa.ai 'rust async' --text 1200 --json"
    );
}

#[test]
fn search_plural_domains_joins_a_shell_split_query() {
    let json = error_json(&[
        "search",
        "rust",
        "async",
        "--include-domains",
        "example.com",
        "--json",
    ]);
    assert_eq!(
        json["error"]["suggestedCommand"],
        "exa-agent search 'rust async' --include-domain example.com --json"
    );
}

#[test]
fn search_plural_domains_keeps_filter_values_out_of_the_query() {
    let json = error_json(&[
        "search",
        "--include-domains",
        "exa.ai",
        "rust",
        "async",
        "--json",
    ]);
    assert_eq!(
        json["error"]["suggestedCommand"],
        "exa-agent search --include-domain exa.ai 'rust async' --json"
    );
}

#[test]
fn search_contents_highlights_points_to_highlights() {
    let json = error_json(&["search", "rust async", "--contents", "highlights", "--json"]);
    assert_eq!(json["error"]["code"], "unknown_flag");
    assert_eq!(
        json["error"]["suggestedCommand"],
        "exa-agent search 'rust async' --highlights --json"
    );
    assert!(json["error"]["details"].get("didYouMean").is_none());
}

#[test]
fn search_contents_negative_value_falls_back_to_help() {
    let json = error_json(&["search", "rust async", "--contents", "-1", "--json"]);
    assert_eq!(json["error"]["code"], "unknown_flag");
    assert_eq!(json["error"]["suggestedCommand"], "exa-agent search --help");
}

#[test]
fn search_content_size_points_to_current_content_controls() {
    let json = error_json(&["search", "rust async", "--content-size", "medium", "--json"]);
    assert_eq!(json["error"]["code"], "unknown_flag");
    assert_eq!(json["error"]["suggestedCommand"], "exa-agent search --help");
    let message = json["error"]["message"].as_str().unwrap();
    assert!(message.contains("--text"), "message: {message}");
    assert!(message.contains("--highlights"), "message: {message}");
    assert!(json["error"]["details"].get("didYouMean").is_none());
}

#[test]
fn contents_no_highlights_points_to_omitting_the_opt_in_flag() {
    let json = error_json(&["contents", "https://exa.ai", "--no-highlights", "--json"]);
    assert_eq!(json["error"]["code"], "unknown_flag");
    assert_eq!(
        json["error"]["suggestedCommand"],
        "exa-agent contents https://exa.ai --json"
    );
    assert!(json["error"]["message"]
        .as_str()
        .unwrap()
        .contains("opt-in"));
    assert!(json["error"]["details"].get("didYouMean").is_none());
}

#[test]
fn contents_no_highlights_discards_a_stale_boolean_value() {
    let json = error_json(&[
        "contents",
        "https://exa.ai",
        "--no-highlights",
        "true",
        "--json",
    ]);
    assert_eq!(
        json["error"]["suggestedCommand"],
        "exa-agent contents https://exa.ai --json"
    );
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
fn search_filter_category_typo_suggests_canonical_category() {
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
    assert!(suggestion.contains("people"));
    assert!(!suggestion.contains("person"));
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
fn search_rejects_bad_category_with_did_you_mean() {
    let json = error_json(&[
        "search",
        "rust async",
        "--category",
        "companys",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_value");
    assert_eq!(json["error"]["details"]["didYouMean"], "company");
    let suggestion = json["error"]["suggestedCommand"].as_str().unwrap();
    assert!(suggestion.contains("--category"));
    assert!(suggestion.contains("company"));
}

#[test]
fn search_rejects_singular_person_category_with_people_hint() {
    let json = error_json(&[
        "search",
        "rust async",
        "--category",
        "person",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_value");
    assert_eq!(json["error"]["details"]["didYouMean"], "people");
    let suggestion = json["error"]["suggestedCommand"].as_str().unwrap();
    assert!(suggestion.contains("--category"));
    assert!(suggestion.contains("people"));
}

#[test]
fn search_rejects_peoples_category_with_people_hint() {
    let json = error_json(&[
        "search",
        "rust async",
        "--category",
        "peoples",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_value");
    assert_eq!(json["error"]["details"]["didYouMean"], "people");
    let suggestion = json["error"]["suggestedCommand"].as_str().unwrap();
    assert!(suggestion.contains("--category"));
    assert!(suggestion.contains("people"));
}

#[test]
fn search_rejects_unknown_category_without_misleading_default() {
    let json = error_json(&[
        "search",
        "rust async",
        "--category",
        "pdf",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_value");
    assert!(json["error"]["details"].get("didYouMean").is_none());
    assert_eq!(
        json["error"]["suggestedCommand"],
        "exa-agent schema show search --compact"
    );
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
fn search_rejects_people_include_domain_unless_linkedin() {
    let json = error_json(&[
        "search",
        "rust async",
        "--category",
        "people",
        "--include-domain",
        "example.com",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["error"]["code"], "invalid_flag_combination");
    assert_eq!(json["error"]["details"]["invalidDomain"], "example.com");

    let ok = ok_json(&[
        "search",
        "rust async",
        "--category",
        "people",
        "--include-domain",
        "www.linkedin.com",
        "--dry-run",
        "--compact",
    ]);
    let body = &ok["data"]["request"]["body"];
    assert_eq!(body["category"], "people");
    assert_eq!(
        body["includeDomains"],
        serde_json::json!(["www.linkedin.com"])
    );
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
