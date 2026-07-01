use exa_agent_macros::IntoFlagValues;

mod request {
    pub fn encode_str_array(values: &[String]) -> String {
        format!("[{}]", values.join(","))
    }
}

fn format_count(value: &u32) -> Option<String> {
    Some(value.to_string())
}

#[test]
fn infers_supported_field_types() {
    #[derive(IntoFlagValues)]
    struct Args {
        text: String,
        maybe: Option<String>,
        enabled: bool,
        tags: Vec<String>,
    }

    assert_eq!(
        Args {
            text: "hello".into(),
            maybe: Some("world".into()),
            enabled: true,
            tags: vec!["a".into(), "b".into()],
        }
        .into_flag_values(),
        vec![
            ("text", Some("hello".into())),
            ("maybe", Some("world".into())),
            ("enabled", Some("true".into())),
            ("tags", Some("[a,b]".into())),
        ]
    );
}

#[test]
fn supports_with_and_clap_long_renames() {
    #[derive(IntoFlagValues)]
    struct Args {
        #[flag(with = "format_count")]
        count: u32,
        #[arg(long = "type")]
        types: Vec<String>,
        #[arg(long)]
        bare_long: String,
        #[flag(rename = "explicit")]
        #[arg(long = "ignored")]
        renamed: String,
    }

    assert_eq!(
        Args {
            count: 7,
            types: vec!["pdf".into()],
            bare_long: "kept".into(),
            renamed: "wins".into(),
        }
        .into_flag_values(),
        vec![
            ("count", Some("7".into())),
            ("type", Some("[pdf]".into())),
            ("bare-long", Some("kept".into())),
            ("explicit", Some("wins".into())),
        ]
    );
}
