use std::collections::BTreeMap;

pub fn canonical_json_bytes(bytes: &[u8]) -> Result<Vec<u8>, serde_json::Error> {
    let value = serde_json::from_slice::<serde_json::Value>(bytes)?;
    let mut text = serde_json::to_string_pretty(&sort_json_objects(value))?;
    text.push('\n');
    Ok(text.into_bytes())
}

fn sort_json_objects(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let sorted: BTreeMap<_, _> = map
                .into_iter()
                .map(|(key, value)| (key, sort_json_objects(value)))
                .collect();
            serde_json::Value::Object(sorted.into_iter().collect())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(sort_json_objects).collect())
        }
        other => other,
    }
}
