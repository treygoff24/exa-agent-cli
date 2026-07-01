use exa_agent_macros::IntoFlagValues;

#[derive(IntoFlagValues)]
struct Args {
    #[flag(skip, with = "format_value")]
    value: String,
}

fn format_value(value: &String) -> Option<String> {
    Some(value.clone())
}

fn main() {}
