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

/// How a request field is supplied at the CLI boundary. `None` means the
/// older registry entry has API metadata only; the contents pilot uses this
/// to keep CLI-facing metadata with its request mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Flag,
    Argument,
}

impl InputKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Flag => "flag",
            Self::Argument => "argument",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputArity {
    pub min: usize,
    pub max: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConstValue {
    Str(&'static str),
    Bool(bool),
    Int(i64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmProtocol {
    Yes,
    EchoId,
    YesPlusEcho(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Capability {
    SecretCapture {
        response_field: &'static str,
        output_flag: &'static str,
        required: bool,
    },
    Chunked {
        input_fields: &'static [&'static str],
        max: u32,
    },
    Macro {
        expands_to: &'static str,
    },
    Confirm(ConfirmProtocol),
    QueryFromBody {
        rules: &'static [(&'static str, &'static str)],
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuilderId {
    /// Permanent self-test for body-builder merge dispatch. Intentionally never
    /// assigned to production operations; future waves add real variants.
    Sentinel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidatorId {}

/// One named flag ⇄ request-body-field mapping (arch §4).
#[derive(Debug, Clone, Copy)]
pub struct FieldDef {
    /// Legacy schema key; retain for one compatibility release.
    pub flag: &'static str,
    pub body_path: &'static str,
    pub kind: FieldKind,
    pub required: bool,
    pub co_fields: &'static [(&'static str, ConstValue)],
    pub item_template: Option<&'static str>,
    pub enum_values: &'static [&'static str],
    pub range: Option<(f64, f64)>,
    pub input_kind: Option<InputKind>,
    /// User-facing input spelling, e.g. `--text` or `URLS`.
    pub input_name: Option<&'static str>,
    pub value_name: Option<&'static str>,
    pub arity: Option<InputArity>,
    /// Numeric range for a CLI value. This is distinct from `range`, which
    /// validates a numeric JSON request-body field.
    pub input_range: Option<(u64, u64)>,
}

pub fn field_input_help(command: &str, flag: &str) -> Option<String> {
    let field = lookup_by_command(command)?
        .fields
        .iter()
        .find(|field| field.flag == flag)?;
    let name = field.input_name?;
    match field_range(field) {
        Some((min, max)) if flag == "text" => Some(format!(
            "Optional character cap: {name} accepts bare, `full`, or {min}..={max}."
        )),
        Some((min, max)) => Some(format!("{name} accepts {min}..={max}.")),
        None => Some(format!("Set the `{}` request field.", field.body_path)),
    }
}

pub fn field_value_name(command: &str, flag: &str) -> Option<&'static str> {
    let field = lookup_by_command(command)?
        .fields
        .iter()
        .find(|field| field.flag == flag)?;
    field.value_name.or(field.input_name)
}

pub fn field_range(field: &FieldDef) -> Option<(u64, u64)> {
    field.input_range.or_else(|| {
        field.range.and_then(|(min, max)| {
            (min >= 0.0 && min.fract() == 0.0 && max.fract() == 0.0)
                .then_some((min as u64, max as u64))
        })
    })
}

fn required_field_range(command: &str, flag: &str) -> (u64, u64) {
    lookup_by_command(command)
        .and_then(|op| op.fields.iter().find(|field| field.flag == flag))
        .and_then(field_range)
        .unwrap_or_else(|| panic!("{command} --{flag} requires registry range metadata"))
}

pub fn text_value_parser(
    command: &'static str,
) -> impl clap::builder::TypedValueParser<Value = String> {
    move |raw: &str| {
        if raw.is_empty() || raw.eq_ignore_ascii_case("full") {
            return Ok(raw.to_string());
        }
        let (min, max) = required_field_range(command, "text");
        raw.parse::<u64>()
            .ok()
            .filter(|value| (min..=max).contains(value))
            .map(|_| raw.to_string())
            .ok_or_else(|| {
                format!(
                    "--text accepts bare, `full`, or {min}..={max}; use --text full or --text {max}"
                )
            })
    }
}

pub fn optional_ranged_string_value_parser(
    command: &'static str,
    flag: &'static str,
) -> impl clap::builder::TypedValueParser<Value = String> {
    move |raw: &str| {
        if raw.is_empty() {
            return Ok(raw.to_string());
        }
        let (min, max) = required_field_range(command, flag);
        raw.parse::<u64>()
            .ok()
            .filter(|value| (min..=max).contains(value))
            .map(|_| raw.to_string())
            .ok_or_else(|| format!("--{flag} accepts {min}..={max}"))
    }
}

pub fn ranged_u32_value_parser(
    command: &'static str,
    flag: &'static str,
) -> impl clap::builder::TypedValueParser<Value = u32> {
    move |raw: &str| {
        let (min, max) = required_field_range(command, flag);
        raw.parse::<u32>()
            .ok()
            .filter(|value| (min..=max).contains(&u64::from(*value)))
            .ok_or_else(|| format!("--{flag} accepts {min}..={max}"))
    }
}

pub fn suggested_string_value_parser(
    _values: &'static [&'static str],
) -> impl clap::builder::TypedValueParser<Value = String> {
    #[derive(Clone)]
    struct Parser;

    impl clap::builder::TypedValueParser for Parser {
        type Value = String;

        fn parse_ref(
            &self,
            _command: &clap::Command,
            _arg: Option<&clap::Arg>,
            value: &std::ffi::OsStr,
        ) -> Result<Self::Value, clap::Error> {
            let value = value.to_string_lossy();
            if value.is_empty() {
                return Err(clap::Error::raw(
                    clap::error::ErrorKind::ValueValidation,
                    "value must not be empty",
                ));
            }
            Ok(value.into_owned())
        }
    }

    Parser
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
    pub capabilities: &'static [Capability],
    pub body_builder: Option<BuilderId>,
    pub validators: &'static [ValidatorId],
    pub mixed_status_exit: bool,
}

impl OperationDef {
    /// `destructive` per the blast-radius triad (D27): dangerous flag OR a DELETE.
    pub fn destructive(&self) -> bool {
        self.dangerous || self.method == Method::Delete
    }

    pub fn confirm_protocol(&self) -> Option<ConfirmProtocol> {
        self.capabilities
            .iter()
            .find_map(|capability| match capability {
                Capability::Confirm(protocol) => Some(*protocol),
                _ => None,
            })
    }

    /// The `(response_field, output_flag, required)` triple if this op mints a
    /// one-time secret that must be captured to a file rather than echoed.
    pub fn secret_capture(&self) -> Option<(&'static str, &'static str, bool)> {
        self.capabilities
            .iter()
            .find_map(|capability| match capability {
                Capability::SecretCapture {
                    response_field,
                    output_flag,
                    required,
                } => Some((*response_field, *output_flag, *required)),
                _ => None,
            })
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
