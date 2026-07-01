use exa_agent_macros::IntoFlagValues;

#[derive(IntoFlagValues)]
struct Args {
    #[flag(bogus)]
    value: String,
}

fn main() {}
