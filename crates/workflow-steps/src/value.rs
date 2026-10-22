use serde_json::{Map, Number, Value};

/// Conversion permissive utilisée pour reproduire `float(...)` de Python sur
/// des valeurs JSON qui peuvent être numériques ou textuelles.
pub(crate) fn as_f64_loose(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
        .or_else(|| value.as_bool().map(|flag| if flag { 1.0 } else { 0.0 }))
}

/// Conversion permissive utilisée pour `int(...)` de Python.
pub(crate) fn as_i64_loose(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|number| number as i64))
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
        .or_else(|| value.as_bool().map(i64::from))
}

pub(crate) fn json_number(value: f64) -> Value {
    Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// Vérité au sens de Python (`bool(value)`), utilisée par les champs
/// optionnels des schémas STEP5/STEP6.
pub(crate) fn value_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|number| number != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

/// Normalise `video_speakers` / `active_speakers` comme Python :
/// liste de chaînes non vides, ou chaîne unique, ou chaîne séparée par virgules.
pub(crate) fn normalize_str_list(value: Option<&Value>) -> Vec<String> {
    let Some(value) = value else {
        return Vec::new();
    };
    match value {
        Value::Array(items) => items
            .iter()
            .filter_map(Value::as_str)
            .filter(|item| !item.is_empty())
            .map(str::to_string)
            .collect(),
        Value::String(text) if !text.is_empty() => {
            if text.contains(',') {
                text.split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(str::to_string)
                    .collect()
            } else {
                vec![text.clone()]
            }
        }
        _ => Vec::new(),
    }
}

/// `video_speakers` en priorité, puis `active_speakers`.
pub(crate) fn object_speakers(object: &Map<String, Value>) -> Vec<String> {
    let value = object
        .get("video_speakers")
        .filter(|value| value_truthy(value))
        .or_else(|| {
            object
                .get("active_speakers")
                .filter(|value| value_truthy(value))
        });
    normalize_str_list(value)
}

/// Approximation de `str(value)` pour les champs scalaires de production.
pub(crate) fn stringify(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        Value::Number(value) => value.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_speaker_shapes() {
        assert_eq!(
            normalize_str_list(Some(&json!(["A", "", "B"]))),
            vec!["A".to_string(), "B".to_string()]
        );
        assert_eq!(
            normalize_str_list(Some(&json!(" A, B ,"))),
            vec!["A".to_string(), "B".to_string()]
        );
        assert_eq!(normalize_str_list(Some(&json!("A"))), vec!["A".to_string()]);
        assert!(normalize_str_list(Some(&json!(null))).is_empty());
    }

    #[test]
    fn python_truthiness() {
        assert!(!value_truthy(&json!(0)));
        assert!(!value_truthy(&json!("")));
        assert!(!value_truthy(&json!([])));
        assert!(value_truthy(&json!([0])));
        assert!(value_truthy(&json!({"a": 1})));
    }
}
