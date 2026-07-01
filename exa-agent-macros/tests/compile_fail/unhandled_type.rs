use exa_agent_macros::IntoFlagValues;

#[derive(IntoFlagValues)]
struct Args {
    count: Option<u32>,
}

fn main() {}
