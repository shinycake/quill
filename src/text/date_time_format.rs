//! `DateTimeFormattingType` (`schema/td_api.tl:6297`, `:6303`): how a date
//! in a message is shown, kept so a quote can send it back unchanged.
#![deny(clippy::indexing_slicing)]

use serde_json::{Value, json};

/// How a date entity or rich text is formatted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateTimeFormat {
    /// No `formatting_type` (null): the reader's full date and time. TDLib
    /// sends null for a date with no format flags.
    #[default]
    Unset,
    Relative,
    Absolute {
        time: DatePartPrecision,
        date: DatePartPrecision,
        day_of_week: bool,
    },
}

/// `DateTimePartPrecision`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePartPrecision {
    None,
    Short,
    Long,
}

impl DatePartPrecision {
    fn from_json(value: Option<&Value>) -> Option<Self> {
        match value {
            None | Some(Value::Null) => Some(Self::None),
            Some(value) => match value.get("@type").and_then(Value::as_str)? {
                "dateTimePartPrecisionNone" => Some(Self::None),
                "dateTimePartPrecisionShort" => Some(Self::Short),
                "dateTimePartPrecisionLong" => Some(Self::Long),
                _ => None,
            },
        }
    }

    fn to_json(self) -> Value {
        let name = match self {
            Self::None => "dateTimePartPrecisionNone",
            Self::Short => "dateTimePartPrecisionShort",
            Self::Long => "dateTimePartPrecisionLong",
        };
        json!({"@type": name})
    }
}

impl DateTimeFormat {
    /// A received `formatting_type`. `None` for a type Quill does not know.
    pub fn from_json(value: Option<&Value>) -> Option<Self> {
        let Some(value) = value.filter(|value| !value.is_null()) else {
            return Some(Self::Unset);
        };
        match value.get("@type").and_then(Value::as_str)? {
            "dateTimeFormattingTypeRelative" => Some(Self::Relative),
            "dateTimeFormattingTypeAbsolute" => Some(Self::Absolute {
                time: DatePartPrecision::from_json(value.get("time_precision"))?,
                date: DatePartPrecision::from_json(value.get("date_precision"))?,
                day_of_week: value
                    .get("show_day_of_week")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }),
            _ => None,
        }
    }

    /// The `formatting_type` JSON TDLib takes; null when unset.
    pub fn to_json(self) -> Value {
        match self {
            Self::Unset => Value::Null,
            Self::Relative => json!({"@type": "dateTimeFormattingTypeRelative"}),
            Self::Absolute {
                time,
                date,
                day_of_week,
            } => json!({
                "@type": "dateTimeFormattingTypeAbsolute",
                "time_precision": time.to_json(),
                "date_precision": date.to_json(),
                "show_day_of_week": day_of_week,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DateTimeFormat;
    use serde_json::json;

    #[test]
    fn formats_round_trip() {
        for value in [
            serde_json::Value::Null,
            json!({"@type": "dateTimeFormattingTypeRelative"}),
            json!({"@type": "dateTimeFormattingTypeAbsolute",
                "time_precision": {"@type": "dateTimePartPrecisionShort"},
                "date_precision": {"@type": "dateTimePartPrecisionLong"},
                "show_day_of_week": true}),
        ] {
            let format = DateTimeFormat::from_json(Some(&value)).expect("known");
            assert_eq!(format.to_json(), value);
        }
        assert!(DateTimeFormat::from_json(Some(&json!({"@type": "dateTimeX"}))).is_none());
    }
}
