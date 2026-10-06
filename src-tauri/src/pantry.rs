//! Writes data/pantry.yaml in the same hand-written style as the original:
//! schema header, then one flow-style mapping per item.

use serde_json::Value;

const FIELD_ORDER: [&str; 7] = ["name", "name_da", "category", "status", "quantity", "unit", "staple"];

/// Plain scalar when YAML reads it back as the same string, otherwise double-quoted
/// (a JSON string literal is valid YAML).
fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => {
            let plain = !s.is_empty()
                && s.chars().all(|c| c.is_alphanumeric() || " -_./()'&".contains(c))
                && serde_yaml_ng::from_str::<Value>(s).ok().as_ref() == Some(v);
            if plain { s.clone() } else { v.to_string() }
        }
        other => other.to_string(),
    }
}

pub fn to_yaml(doc: &Value) -> String {
    let mut out = String::from("# yaml-language-server: $schema=../schemas/pantry.schema.yaml\n");
    out += &format!("updated: {}\n", doc["updated"].as_str().unwrap_or_default());
    out += "items:\n";
    for item in doc["items"].as_array().into_iter().flatten() {
        let Some(map) = item.as_object() else { continue };
        let mut keys: Vec<&str> = FIELD_ORDER.iter().copied().filter(|k| map.contains_key(*k)).collect();
        keys.extend(map.keys().map(String::as_str).filter(|k| !FIELD_ORDER.contains(k)));
        let fields: Vec<String> = keys.iter().map(|k| format!("{k}: {}", scalar(&map[*k]))).collect();
        out += &format!("  - {{{}}}\n", fields.join(", "));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_hand_written_style() {
        let hand_written = r#"# yaml-language-server: $schema=../schemas/pantry.schema.yaml
updated: 2026-10-06
items:
  - {name: olive oil, name_da: olivenolie, category: oils, status: stocked, quantity: 1, unit: l, staple: true}
  - {name: salt, category: spices_condiments, status: low}
"#;
        let original: Value = serde_yaml_ng::from_str(hand_written).unwrap();
        assert_eq!(to_yaml(&original), hand_written, "style should match the hand-written file");
    }

    #[test]
    fn quotes_ambiguous_strings() {
        let doc = serde_json::json!({"updated": "2026-10-06", "items": [
            {"name": "yes", "category": "other", "status": "low"},
            {"name": "salt, coarse: flaky", "category": "other", "status": "out", "quantity": 1.5}
        ]});
        let written = to_yaml(&doc);
        assert_eq!(serde_yaml_ng::from_str::<Value>(&written).unwrap(), doc);
    }
}
