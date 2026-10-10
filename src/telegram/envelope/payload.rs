use super::*;
use serde_json::Value;

/// Slice CL3: true when a `BlockList` JSON value is `blockListMain`
/// (schema 1.8.67, lines 9692–9695).
pub(crate) fn is_block_list_main(block_list: Option<&Value>) -> bool {
    block_list
        .and_then(|b| b.get("@type"))
        .and_then(Value::as_str)
        == Some("blockListMain")
}

/// B7: the admin-toggle flags of a `supergroupFullInfo` object (schema
/// 1.8.67, line 2792); missing flags read as false.
pub(crate) fn parse_supergroup_full_admin(info: Option<&Value>) -> SupergroupFullAdmin {
    let flag = |name: &str| {
        info.and_then(|info| info.get(name))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    SupergroupFullAdmin {
        can_hide_members: flag("can_hide_members"),
        has_hidden_members: flag("has_hidden_members"),
        is_all_history_available: flag("is_all_history_available"),
        can_enable_paid_reaction: flag("can_enable_paid_reaction"),
    }
}

type DomainParser = fn(&str, &Value) -> Result<Option<EnvelopePayload>, ParseError>;

/// Each domain claims its own TDLib type names (`Ok(None)` for the
/// rest); the most frequent updates come first.
const DOMAIN_PARSERS: &[DomainParser] = &[
    parse_messages_payload,
    parse_chats_payload,
    parse_chat_list_payload,
    parse_users_payload,
    parse_common_payload,
    parse_groups_payload,
    parse_media_payload,
    parse_stories_payload,
    parse_stickers_payload,
    parse_settings_payload,
    parse_calls_payload,
    parse_threads_payload,
    parse_search_payload,
    parse_bots_payload,
    parse_payments_payload,
    parse_auth_payload,
];

pub(crate) fn parse_payload(type_name: &str, json: &str) -> Result<EnvelopePayload, ParseError> {
    let value: Value = serde_json::from_str(json).map_err(|_| ParseError::InvalidJson)?;
    for parse in DOMAIN_PARSERS {
        if let Some(payload) = parse(type_name, &value)? {
            return Ok(payload);
        }
    }
    match type_name {
        "ok" => Ok(EnvelopePayload::Ok),
        "error" => Ok(EnvelopePayload::Error(parse_error(Some(&value)))),
        other => {
            // Phase D2: an unknown future `ChatStatistics` constructor must
            // fail parsing rather than silently becoming `Unknown` and
            // dropping the statistics response. All other unknown types
            // keep the existing `Unknown` convention.
            if other.starts_with("chatStatistics") {
                return Err(ParseError::MissingField);
            }
            Ok(EnvelopePayload::Unknown(UnknownKind {
                type_name: other.to_string(),
            }))
        }
    }
}
