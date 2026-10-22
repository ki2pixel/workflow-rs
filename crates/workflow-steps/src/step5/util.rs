use serde_json::{Number, Value};

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
