//! Bridges the build-time-generated table into the local registry types.
//! `registry.rs` (emitted to `$OUT_DIR`) names `OperationDef`, `FieldDef`, `Method`,
//! `Pagination`, `Namespace`, `FieldKind` — brought into scope here via `super::*`.

use super::*;

include!(concat!(env!("OUT_DIR"), "/registry.rs"));
