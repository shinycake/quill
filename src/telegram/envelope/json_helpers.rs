use super::*;
use crate::ids::MessageId;
use serde_json::Value;

/// Saturating narrowing of a TDLib 64-bit integer to the `i32` the
/// state model stores. A malformed or out-of-range payload clamps to
/// the nearest representable value instead of wrapping to garbage.
pub(crate) trait SatI32 {
    fn sat_i32(self) -> i32;
}

impl SatI32 for i64 {
    fn sat_i32(self) -> i32 {
        self.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
    }
}

pub(crate) fn json_bool(value: Option<&Value>, default: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(default)
}

pub(crate) fn json_i32(value: Option<&Value>, default: i32) -> i32 {
    value
        .and_then(|v| {
            v.as_i64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(default as i64)
        .sat_i32()
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

#[cfg(test)]
mod hardening_tests {
    use super::{SatI32, json_i32};
    use crate::telegram::envelope::message::{
        MessageSelfDestruct, SelfDestructKind, parse_auto_delete_in,
    };
    use crate::telegram::envelope::message_audio_video::{
        parse_message_audio, parse_message_video, parse_message_video_note,
    };
    use serde_json::json;

    #[test]
    fn sat_i32_clamps_out_of_range_server_values() {
        assert_eq!(i64::MAX.sat_i32(), i32::MAX);
        assert_eq!(i64::MIN.sat_i32(), i32::MIN);
        assert_eq!(42_i64.sat_i32(), 42);
        assert_eq!(json_i32(Some(&json!(9_999_999_999_i64)), 0), i32::MAX);
    }

    #[test]
    fn media_parsers_degrade_on_missing_or_wrong_typed_media() {
        for value in [
            json!({}),
            json!({"video": null}),
            json!({"video": {"@type": "x"}}),
        ] {
            let _ = parse_message_video(&value);
            let _ = parse_message_video_note(&json!({"video_note": value.get("video")}));
            let _ = parse_message_audio(&json!({"audio": value.get("video")}));
        }
    }

    #[test]
    fn huge_timers_do_not_overflow() {
        let destruct = MessageSelfDestruct {
            kind: SelfDestructKind::Immediately,
            expires_in_ms: i64::MAX,
            fetched_at_ms: 0,
        };
        assert!(destruct.remaining_secs(0).is_some());
        let auto = parse_auto_delete_in(Some(&json!(1e300))).expect("finite positive");
        let _ = auto.remaining_secs(u64::MAX);
    }
}
