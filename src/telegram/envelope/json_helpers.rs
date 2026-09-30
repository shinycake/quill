use super::*;
use crate::ids::MessageId;
use serde_json::Value;

pub(crate) fn json_bool(value: Option<&Value>, default: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(default)
}

pub(crate) fn json_i32(value: Option<&Value>, default: i32) -> i32 {
    value
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(default as i64) as i32
}

pub(crate) fn json_i64_field(value: Option<&Value>, default: i64) -> i64 {
    value
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(default)
}

pub(crate) fn json_field_str(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

pub(crate) fn int53(value: Option<&Value>) -> Result<i64, ParseError> {
    match value {
        Some(Value::Number(n)) => n.as_i64().ok_or(ParseError::BadInt),
        Some(Value::String(s)) => s.parse().map_err(|_| ParseError::BadInt),
        _ => Err(ParseError::MissingField),
    }
}

pub(crate) fn int53_or_zero(value: Option<&Value>) -> i64 {
    int53(value).unwrap_or(0)
}

pub(crate) fn int64(value: Option<&Value>) -> Option<i64> {
    match value {
        Some(Value::String(s)) => s.parse().ok(),
        Some(Value::Number(n)) => n.as_i64(),
        _ => None,
    }
}

pub(crate) fn int53_array(value: Option<&Value>) -> Vec<MessageId> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| int53(Some(v)).ok())
        .map(MessageId)
        .collect()
}
