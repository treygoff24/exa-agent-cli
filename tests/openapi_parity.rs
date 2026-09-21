use exa_agent_cli::registry;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::process::Command;

struct SpecDoc {
    name: &'static str,
    value: Value,
}

#[derive(Default)]
struct BodyShape {
    properties: BTreeSet<String>,
    required: BTreeSet<String>,
}

#[test]
fn every_vendored_api_operation_has_a_typed_command() {
    let registry_ids: BTreeSet<_> = registry::REGISTRY
        .iter()
        .map(|op| op.operation_id)
        .collect();
    for spec in load_specs() {
        let mut operations = BTreeSet::new();
        for methods in spec.value["paths"]
            .as_object()
            .expect("OpenAPI paths")
            .values()
        {
            for method in ["get", "post", "put", "patch", "delete", "head", "options"] {
                if let Some(operation) = methods.get(method) {
                    operations.insert(operation["operationId"].as_str().expect("operationId"));
                }
            }
        }
        assert!(
            !operations.is_empty(),
            "{} has no API operations",
            spec.name
        );
        let missing: Vec<_> = operations.difference(&registry_ids).collect();
        assert!(
            missing.is_empty(),
            "{} operations missing typed commands: {missing:?}",
            spec.name
        );
    }
}

#[test]
fn modeled_registry_fields_match_openapi_request_bodies() {
    let specs = load_specs();
    let mut checked = Vec::new();
    let mut skipped = Vec::new();
    let mut skipped_ids = BTreeSet::new();
    let mut failures = Vec::new();

    for op in registry::REGISTRY.iter() {
        let (spec, schema, shape) = match request_body_shape(&specs, op.operation_id) {
            BodyLookup::Shape(spec, schema, shape) => (spec, schema, shape),
            // The operation exists upstream and simply has no JSON request body (every GET,
            // most DELETEs). Nothing to compare, and nothing worth reporting as a skip
            // unless the overlay models fields for it anyway.
            BodyLookup::NoRequestBody => {
                if !op.fields.is_empty() {
                    skipped.push(format!(
                        "{}: no resolvable OpenAPI JSON requestBody schema",
                        op.operation_id
                    ));
                    skipped_ids.insert(op.operation_id);
                }
                continue;
            }
            BodyLookup::NoOperation => {
                if !op.fields.is_empty() {
                    skipped.push(format!(
                        "{}: no such operation in any vendored spec",
                        op.operation_id
                    ));
                    skipped_ids.insert(op.operation_id);
                }
                continue;
            }
            BodyLookup::Failed(err) => {
                skipped.push(format!("{}: {err}", op.operation_id));
                skipped_ids.insert(op.operation_id);
                continue;
            }
        };
        checked.push(format!("{} ({})", op.operation_id, spec.name));

        for field in op.fields {
            if field.request_location != registry::RequestLocation::Body {
                continue;
            }
            if let Err(err) = resolve_body_path(&spec.value, schema, field.body_path) {
                failures.push(format!(
                    "{} field `{}` body_path `{}`: {err}",
                    op.operation_id, field.flag, field.body_path
                ));
            }
        }

        // The required half runs for every operation with a request body, including the
        // ones the registry models no fields for — those expose their required properties
        // through bespoke clap flags, and nothing else notices when one disappears.
        let required_fields: BTreeSet<&str> = op
            .fields
            .iter()
            .filter(|field| {
                field.request_location == registry::RequestLocation::Body && field.required
            })
            .map(|field| top_level_segment(field.body_path))
            .collect();
        let externally_sourced = externally_sourced_required(op.operation_id);
        for required in &shape.required {
            if required_fields.contains(required.as_str())
                || externally_sourced.contains(&required.as_str())
            {
                continue;
            }
            failures.push(format!(
                "{} OpenAPI required property `{}` is not covered by a required FieldDef or the externally-sourced allowlist; required modeled top-level fields: {:?}",
                op.operation_id, required, required_fields
            ));
        }
    }

    checked.sort();
    skipped.sort();
    println!("OpenAPI parity checked: {}", checked.join(", "));
    println!("OpenAPI parity skipped: {}", skipped.join(", "));

    assert!(
        !checked.is_empty(),
        "OpenAPI requestBody parity checked zero modeled ops"
    );
    let known_skips = known_skips();
    let unexpected_skips: Vec<_> = skipped_ids.difference(&known_skips).copied().collect();
    assert!(
        unexpected_skips.is_empty(),
        "unexpected OpenAPI parity skip(s): {:?}; this modeled op no longer resolves an OpenAPI requestBody; either fix its schema resolution or justify it in known_skips()",
        unexpected_skips
    );
    assert!(
        failures.is_empty(),
        "OpenAPI requestBody parity failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn modeled_query_parameters_match_openapi() {
    let specs = load_specs();
    let mut checked = Vec::new();
    let mut failures = Vec::new();

    for op in registry::REGISTRY {
        for field in op
            .fields
            .iter()
            .filter(|field| field.request_location == registry::RequestLocation::Query)
        {
            let Some((spec, operation)) = specs
                .iter()
                .find_map(|spec| find_operation(&spec.value, op.operation_id).map(|op| (spec, op)))
            else {
                failures.push(format!(
                    "{} query field `{}` has no vendored OpenAPI operation",
                    op.operation_id, field.flag
                ));
                continue;
            };
            let parameter = operation["parameters"].as_array().and_then(|parameters| {
                parameters.iter().find(|parameter| {
                    parameter["name"].as_str() == Some(field.body_path)
                        && parameter["in"].as_str() == Some("query")
                })
            });
            if parameter.is_none() {
                failures.push(format!(
                    "{} field `{}` does not match query parameter `{}` in {}",
                    op.operation_id, field.flag, field.body_path, spec.name
                ));
            } else {
                checked.push(format!("{} --{}", op.operation_id, field.flag));
            }
        }
    }

    assert_eq!(checked, ["websets-preview --search"]);
    assert!(
        failures.is_empty(),
        "OpenAPI query-parameter parity failures:\n{}",
        failures.join("\n")
    );
}

#[test]
fn agent_effort_and_budget_match_current_openapi() {
    let spec = load_specs()
        .into_iter()
        .find(|spec| spec.name == "exa-openapi")
        .expect("exa openapi spec");
    let schemas = spec
        .value
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .expect("schemas");
    let effort: BTreeSet<_> = schemas["AgentEffort"]["enum"]
        .as_array()
        .expect("AgentEffort enum")
        .iter()
        .map(|value| value.as_str().expect("enum string").to_string())
        .collect();
    assert_eq!(
        effort,
        BTreeSet::from([
            "auto".to_string(),
            "high".to_string(),
            "low".to_string(),
            "max".to_string(),
            "medium".to_string(),
            "minimal".to_string(),
            "xhigh".to_string(),
        ])
    );
    let create_props = schemas["CreateAgentRunRequest"]["properties"]
        .as_object()
        .expect("CreateAgentRunRequest properties");
    assert!(create_props.contains_key("budget"));
    assert!(create_props.contains_key("effort"));
}

#[test]
fn agent_data_source_runtime_accepts_current_openapi_provider_enum() {
    let spec = load_specs()
        .into_iter()
        .find(|spec| spec.name == "exa-openapi")
        .expect("exa openapi spec");
    let schemas = spec
        .value
        .pointer("/components/schemas")
        .and_then(Value::as_object)
        .expect("schemas");
    let expected: Value = schemas["AgentDataSourceProvider"]["enum"].clone();
    assert_eq!(
        serde_json::to_value(registry::AGENT_DATA_SOURCE_PROVIDERS).unwrap(),
        expected,
        "registry provider source must track AgentDataSourceProvider enum"
    );
    for provider in expected.as_array().expect("provider enum array") {
        let provider = provider.as_str().expect("provider enum string");
        for given in [provider.to_string(), provider.to_ascii_uppercase()] {
            let output = Command::new(env!("CARGO_BIN_EXE_exa-agent"))
                .args([
                    "agent",
                    "runs",
                    "create",
                    "q",
                    "--data-source",
                    &given,
                    "--dry-run",
                    "--compact",
                ])
                .env("EXA_AGENT_NO_NETWORK", "1")
                .env_remove("EXA_API_KEY")
                .env_remove("EXA_SERVICE_KEY")
                .output()
                .unwrap_or_else(|err| panic!("run exa-agent with provider {given}: {err}"));
            assert_eq!(
                output.status.code(),
                Some(0),
                "provider {given} should be accepted\nstdout:\n{}\nstderr:\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let ok: Value = serde_json::from_slice(&output.stdout).expect("stdout JSON");
            assert_eq!(
                ok["data"]["request"]["body"]["dataSources"],
                serde_json::json!([{ "provider": provider }]),
                "provider {given} should canonicalize to {provider}"
            );
        }
    }

    let output = Command::new(env!("CARGO_BIN_EXE_exa-agent"))
        .args([
            "agent",
            "runs",
            "create",
            "q",
            "--data-source",
            "not_a_provider",
            "--dry-run",
            "--compact",
        ])
        .env("EXA_AGENT_NO_NETWORK", "1")
        .env_remove("EXA_API_KEY")
        .env_remove("EXA_SERVICE_KEY")
        .output()
        .expect("run exa-agent");
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&output.stderr).expect("stderr JSON");
    assert_eq!(error["error"]["details"]["accepted"], expected);
}

/// `openapi/PROVENANCE.md` went six operations and two hashes stale while
/// `xtask vendor-spec --check` stayed green, because that check compared `info.title` and
/// `info.version` — neither of which moves when upstream adds operations. The record now
/// lives in `openapi/provenance.toml` and is re-measured here as well, so a re-vendor that
/// forgets it fails in the ordinary test run rather than only in a tool nobody runs.
#[test]
fn provenance_records_the_committed_specs() {
    use sha2::{Digest, Sha256};

    let record: toml::Value = toml::from_str(
        &fs::read_to_string("openapi/provenance.toml").expect("read openapi/provenance.toml"),
    )
    .expect("parse openapi/provenance.toml");
    let specs = load_specs();
    for (section, path, name) in [
        ("exa", "openapi/exa-openapi.json", "exa-openapi"),
        ("admin", "openapi/team-management.json", "team-management"),
    ] {
        let entry = record
            .get(section)
            .unwrap_or_else(|| panic!("provenance.toml has no [{section}] table"));
        assert_eq!(
            entry.get("vendored_path").and_then(toml::Value::as_str),
            Some(path),
            "[{section}] vendored_path"
        );
        let bytes = fs::read(path).unwrap_or_else(|err| panic!("read {path}: {err}"));
        assert_eq!(
            entry.get("vendored_sha256").and_then(toml::Value::as_str),
            Some(format!("{:x}", Sha256::digest(&bytes)).as_str()),
            "[{section}] vendored_sha256 is stale; re-run `cargo run -p xtask -- vendor-spec --check`"
        );
        let spec = specs
            .iter()
            .find(|spec| spec.name == name)
            .expect("loaded spec");
        let operations = spec.value["paths"]
            .as_object()
            .expect("OpenAPI paths")
            .values()
            .filter_map(Value::as_object)
            .flat_map(|methods| methods.values())
            .filter_map(|operation| operation.get("operationId"))
            .count() as i64;
        assert_eq!(
            entry.get("operations").and_then(toml::Value::as_integer),
            Some(operations),
            "[{section}] operations is stale; the spec now has {operations}"
        );
    }
}

fn load_specs() -> Vec<SpecDoc> {
    [
        ("openapi/exa-openapi.json", "exa-openapi"),
        ("openapi/team-management.json", "team-management"),
    ]
    .into_iter()
    .map(|(path, name)| SpecDoc {
        name,
        value: serde_json::from_str(
            &fs::read_to_string(path).unwrap_or_else(|err| panic!("failed to read {path}: {err}")),
        )
        .unwrap_or_else(|err| panic!("failed to parse {path}: {err}")),
    })
    .collect()
}

/// What the vendored specs say about one operation's request body. Distinguishing "this
/// operation has no request body" from "this operation is not in any vendored spec" is what
/// lets the required-property check run over every operation without drowning in skips.
enum BodyLookup<'a> {
    Shape(&'a SpecDoc, &'a Value, BodyShape),
    NoRequestBody,
    NoOperation,
    Failed(String),
}

fn request_body_shape<'a>(specs: &'a [SpecDoc], operation_id: &str) -> BodyLookup<'a> {
    for spec in specs {
        let Some(operation) = find_operation(&spec.value, operation_id) else {
            continue;
        };
        let Some(schema) = operation
            .get("requestBody")
            .and_then(|body| body.get("content"))
            .and_then(|content| content.get("application/json"))
            .and_then(|json| json.get("schema"))
        else {
            return BodyLookup::NoRequestBody;
        };
        return match collect_shape(&spec.value, schema, 0) {
            Ok(shape) => BodyLookup::Shape(spec, schema, shape),
            Err(err) => BodyLookup::Failed(err),
        };
    }
    BodyLookup::NoOperation
}

fn find_operation<'a>(doc: &'a Value, operation_id: &str) -> Option<&'a Value> {
    const METHODS: &[&str] = &["get", "post", "put", "patch", "delete"];
    for path_item in doc.get("paths")?.as_object()?.values() {
        let methods = path_item.as_object()?;
        for method in METHODS {
            let Some(operation) = methods.get(*method) else {
                continue;
            };
            if operation
                .get("operationId")
                .and_then(Value::as_str)
                .is_some_and(|id| id == operation_id)
            {
                return Some(operation);
            }
        }
    }
    None
}

fn collect_shape(doc: &Value, schema: &Value, depth: usize) -> Result<BodyShape, String> {
    if depth > 4 {
        return Err("requestBody schema resolution exceeded depth limit".to_string());
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return collect_shape(doc, resolve_schema_ref(doc, reference)?, depth + 1);
    }

    let mut shape = BodyShape::default();
    let mut saw_shape = false;

    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        saw_shape = true;
        shape.properties.extend(properties.keys().cloned());
    }
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        saw_shape = true;
        shape.required.extend(
            required
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned),
        );
    }
    if let Some(parts) = schema.get("allOf").and_then(Value::as_array) {
        saw_shape = true;
        for part in parts {
            shape.merge(collect_shape(doc, part, depth + 1)?);
        }
    }
    for composition in ["oneOf", "anyOf"] {
        if let Some(parts) = schema.get(composition).and_then(Value::as_array) {
            saw_shape = true;
            for part in parts {
                shape
                    .properties
                    .extend(collect_shape(doc, part, depth + 1)?.properties);
            }
        }
    }

    if saw_shape {
        Ok(shape)
    } else {
        Err("requestBody schema has no resolvable shape metadata".to_string())
    }
}

/// Walk a dotted `body_path` segment by segment through the request-body schema, merging
/// `$ref`, `allOf`, `oneOf`, and `anyOf` branches at every level. Checking only the first
/// segment let a nested typo — `search.includeDomain` for `search.includeDomains` — pass the
/// gate while the CLI silently sent a property the API drops.
fn resolve_body_path(doc: &Value, schema: &Value, body_path: &str) -> Result<(), String> {
    let mut current: Vec<&Value> = vec![schema];
    let mut walked: Vec<&str> = Vec::new();
    for segment in body_path.split('.') {
        if segment.is_empty() {
            return Err(format!("empty segment in body path `{body_path}`"));
        }
        let mut properties: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
        for schema in &current {
            merge_properties(doc, schema, 0, &mut properties)?;
        }
        let Some(next) = properties.remove(segment) else {
            let known: Vec<&str> = properties.keys().copied().collect();
            let parent = if walked.is_empty() {
                "the request body".to_string()
            } else {
                format!("`{}`", walked.join("."))
            };
            return Err(format!(
                "segment `{segment}` is not a property of {parent}; available: {known:?}"
            ));
        };
        walked.push(segment);
        current = next;
    }
    Ok(())
}

/// Collect every property name reachable from `schema` without descending into a property's
/// own sub-schema, mapping each name to the sub-schemas the composition branches give it.
fn merge_properties<'a>(
    doc: &'a Value,
    schema: &'a Value,
    depth: usize,
    out: &mut BTreeMap<&'a str, Vec<&'a Value>>,
) -> Result<(), String> {
    if depth > 8 {
        return Err("body-path schema resolution exceeded depth limit".to_string());
    }
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return merge_properties(doc, resolve_schema_ref(doc, reference)?, depth + 1, out);
    }
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (name, sub) in properties {
            out.entry(name.as_str()).or_default().push(sub);
        }
    }
    // An array of objects exposes its element's properties one level down, so a body path
    // through it reads the same way an object's does.
    if let Some(items) = schema.get("items") {
        merge_properties(doc, items, depth + 1, out)?;
    }
    for composition in ["allOf", "oneOf", "anyOf"] {
        if let Some(parts) = schema.get(composition).and_then(Value::as_array) {
            for part in parts {
                merge_properties(doc, part, depth + 1, out)?;
            }
        }
    }
    Ok(())
}

fn resolve_schema_ref<'a>(doc: &'a Value, reference: &str) -> Result<&'a Value, String> {
    let name = reference
        .strip_prefix("#/components/schemas/")
        .ok_or_else(|| format!("unsupported requestBody schema ref `{reference}`"))?;
    doc.get("components")
        .and_then(|components| components.get("schemas"))
        .and_then(|schemas| schemas.get(name))
        .ok_or_else(|| format!("missing OpenAPI component schema `{name}`"))
}

impl BodyShape {
    fn merge(&mut self, other: BodyShape) {
        self.properties.extend(other.properties);
        self.required.extend(other.required);
    }
}

/// The first body-path segment, which is the level OpenAPI `required` lists name. Whole-path
/// validation is [`resolve_body_path`]'s job, not this one's.
fn top_level_segment(body_path: &str) -> &str {
    body_path
        .split('.')
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or(body_path)
}

/// Required body properties the CLI supplies from something other than a required
/// `FieldDef` — a positional argument, or a bespoke clap flag outside the registry. Every
/// entry names a real, verified source; never use this to silence a property nothing sends.
fn externally_sourced_required(operation_id: &str) -> &'static [&'static str] {
    match operation_id {
        // Positional arguments.
        "answer" | "createAgentRun" | "search" => &["query"],
        "findSimilar" => &["url"],
        // `monitor batch` takes its body whole from --body/--set; both properties are
        // required by the bespoke validator in src/lib.rs (`monitor batch requires
        // \`action\``, `monitor batch requires a non-empty \`filter\` object`).
        "batchMonitors" => &["action", "filter"],
        // Same shape: `requests` may arrive via --requests, --body, --set, or a preset, so
        // the overlay leaves it optional and `batches::validate_create_body` requires a
        // non-empty array on the merged body.
        "createBatch" => &["requests"],
        // `websets monitors create` builds all three from typed clap flags outside the
        // registry: --webset-id, --cron/--timezone, and --search-behavior/--query/--count.
        "monitors-create" => &["websetId", "cadence", "behavior"],
        _ => &[],
    }
}

fn known_skips() -> BTreeSet<&'static str> {
    [
        // Docs-only overlay-defined command; no upstream OpenAPI JSON requestBody
        // schema exists to compare.
        "context",
    ]
    .into_iter()
    .collect()
}
