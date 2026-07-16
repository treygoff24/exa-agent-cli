//! Portable in-repo ergonomics harness (Wave 6).
//!
//! Run with `cargo test --test ergonomics` or `cargo xtask ergonomics`.

#[path = "ergonomics/harness.rs"]
mod harness;
#[path = "ergonomics/intent_mistakes.rs"]
mod intent_mistakes;
#[path = "ergonomics/robot_docs.rs"]
mod robot_docs;
#[path = "ergonomics/scoring.rs"]
mod scoring;
#[path = "ergonomics/skill.rs"]
mod skill;
