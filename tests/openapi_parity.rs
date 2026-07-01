use exa_agent_cli::registry;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;

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
        "ResearchController_createResearch" => &["instructions"],
        _ => &[],
    }
}

fn known_skips() -> BTreeSet<&'static str> {
    [
        // Docs-only overlay-defined single-witness command; no upstream OpenAPI
        // JSON requestBody schema exists to compare.
        "context",
    ]
    .into_iter()
    .collect()
}
