use exa_agent_cli::registry;
use serde_json::Value;
use std::collections::BTreeSet;
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
fn modeled_registry_fields_match_openapi_request_bodies() {
    let specs = load_specs();
    let mut checked = Vec::new();
    let mut skipped = Vec::new();
    let mut skipped_ids = BTreeSet::new();
    let mut failures = Vec::new();

    for op in registry::REGISTRY.iter().filter(|op| !op.fields.is_empty()) {
        let shape = match request_body_shape(&specs, op.operation_id) {
            Ok(Some((spec_name, shape))) => {
                checked.push(format!("{} ({spec_name})", op.operation_id));
                shape
            }
            Ok(None) => {
                skipped.push(format!(
                    "{}: no resolvable OpenAPI JSON requestBody schema",
                    op.operation_id
                ));
                skipped_ids.insert(op.operation_id);
                continue;
            }
            Err(err) => {
                skipped.push(format!("{}: {err}", op.operation_id));
                skipped_ids.insert(op.operation_id);
                continue;
            }
        };

        for field in op.fields {
            let top = top_level_segment(field.body_path);
            if !shape.properties.contains(top) {
                failures.push(format!(
                    "{} field `{}` body_path `{}` has top-level segment `{}` missing from OpenAPI requestBody properties {:?}",
                    op.operation_id, field.flag, field.body_path, top, shape.properties
                ));
            }
        }

        let required_fields: BTreeSet<&str> = op
            .fields
            .iter()
            .filter(|field| field.required)
            .map(|field| top_level_segment(field.body_path))
            .collect();
        let positional_required = positional_required_allowlist(op.operation_id);
        for required in &shape.required {
            if required_fields.contains(required.as_str())
                || positional_required.contains(&required.as_str())
            {
                continue;
            }
            failures.push(format!(
                "{} OpenAPI required property `{}` is not covered by a required FieldDef or positional-source allowlist; required modeled top-level fields: {:?}",
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

fn request_body_shape(
    specs: &[SpecDoc],
    operation_id: &str,
) -> Result<Option<(&'static str, BodyShape)>, String> {
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
            return Ok(None);
        };
        let shape = collect_shape(&spec.value, schema, 0)?;
        return Ok(Some((spec.name, shape)));
    }
    Ok(None)
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

/// Returns the top-level body-path segment only.
///
/// Known non-goal: nested typos like `entity.typ` are not caught here; validating
/// nested segments would require per-branch oneOf/anyOf checking.
fn top_level_segment(body_path: &str) -> &str {
    body_path
        .split('.')
        .next()
        .filter(|segment| !segment.is_empty())
        .unwrap_or(body_path)
}

fn positional_required_allowlist(operation_id: &str) -> &'static [&'static str] {
    match operation_id {
        // Forward-looking net for required body properties sourced from positional
        // args but not modeled as required FieldDefs. Redundant today: every entry
        // is already covered by a required FieldDef. Only add genuine positional-
        // sourced required body properties; never use this to silence a real miss.
        "answer" | "createAgentRun" | "search" => &["query"],
        "findSimilar" => &["url"],
        _ => &[],
    }
}

fn known_skips() -> BTreeSet<&'static str> {
    [
        // Docs-only overlay-defined commands; no upstream OpenAPI JSON requestBody
        // schema exists to compare.
        "context",
        "websets-exports-create",
    ]
    .into_iter()
    .collect()
}
