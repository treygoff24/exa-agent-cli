//! The operation registry: a build-time-generated static table describing every Exa
//! operation the CLI knows (D9/D17). Command routing, `capabilities`, `schema`, and
//! local validation all read from here. See `build.rs` for codegen.

mod generated;

pub use generated::{
    ADMIN_SPEC_TITLE, ADMIN_SPEC_VERSION, BUILD_DATE, EMBEDDED_SPEC_SHA256, GIT_SHA, REGISTRY,
    SPEC_TITLE, SPEC_URL, SPEC_VERSION, TARGET,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
        }
    }
}

/// Credential + host selection (D4). `Api` => EXA_API_KEY + api host; `Service` => EXA_SERVICE_KEY + admin host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Namespace {
    Api,
    Service,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pagination {
    None,
    /// Cursor-paginated list; the field carries the upstream nextCursor key (contracts §10).
    Cursor(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Str,
    Int,
    Bool,
    Num,
    StrArray,
    Json,
}

/// One named flag ⇄ request-body-field mapping (arch §4).
#[derive(Debug, Clone, Copy)]
pub struct FieldDef {
    pub flag: &'static str,
    pub body_path: &'static str,
    pub kind: FieldKind,
    pub required: bool,
}

/// One Exa operation. Carries exactly what the contracts surface plus the internal
/// metadata commands route on (arch §3).
#[derive(Debug, Clone, Copy)]
pub struct OperationDef {
    pub cli_path: &'static [&'static str],
    pub operation_id: &'static str,
    pub method: Method,
    pub api_path: &'static str,
    pub read_only: bool,
    pub streaming: bool,
    pub pagination: Pagination,
    pub dangerous: bool,
    pub namespace: Namespace,
    pub idempotency_sensitive: bool,
    pub deprecated: bool,
    pub source: &'static str,
    pub source_version: &'static str,
    pub fields: &'static [FieldDef],
}

impl OperationDef {
    /// `destructive` per the blast-radius triad (D27): dangerous flag OR a DELETE.
    pub fn destructive(&self) -> bool {
        self.dangerous || self.method == Method::Delete
    }

    /// Space-joined command path, e.g. `agent runs create`.
    pub fn command(&self) -> String {
        self.cli_path.join(" ")
    }
}

/// Resolve an operation by its space-joined CLI path (`"agent runs create"`).
pub fn lookup_by_command(path: &str) -> Option<&'static OperationDef> {
    REGISTRY.iter().find(|op| op.command() == path)
}

/// Resolve an operation by its leading path segments.
pub fn lookup_by_segments(segments: &[&str]) -> Option<&'static OperationDef> {
    REGISTRY.iter().find(|op| op.cli_path == segments)
}

/// Every operationId subject to the no-auto-retry-on-create rule (D7/contracts §7).
pub fn idempotency_sensitive_ids() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = REGISTRY
        .iter()
        .filter(|op| op.idempotency_sensitive)
        .map(|op| op.operation_id)
        .collect();
    v.sort_unstable();
    v
}
