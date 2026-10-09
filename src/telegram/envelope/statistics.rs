use super::*;
use serde_json::Value;

/// Phase D2: `statisticalValue` (TDLib 1.8.67, `schema/td_api.tl:10139`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatisticalValue {
    pub value: f64,
    pub previous_value: f64,
    pub growth_rate_percentage: f64,
}

impl StatisticalValue {
    fn parse(value: &Value) -> Result<Self, ParseError> {
        // `statisticalValue value:double previous_value:double
        // growth_rate_percentage:double` (schema 1.8.67, line 10139) — all
        // three fields are required; missing or mistyped fields are a
        // parse error rather than fabricated zeros.
        Ok(StatisticalValue {
            value: value
                .get("value")
                .and_then(Value::as_f64)
                .ok_or(ParseError::MissingField)?,
            previous_value: value
                .get("previous_value")
                .and_then(Value::as_f64)
                .ok_or(ParseError::MissingField)?,
            growth_rate_percentage: value
                .get("growth_rate_percentage")
                .and_then(Value::as_f64)
                .ok_or(ParseError::MissingField)?,
        })
    }
}

/// Phase D2: `StatisticalGraph` (TDLib 1.8.67) — `statisticalGraphData`
/// (`schema/td_api.tl:10145`), `statisticalGraphAsync` (`:10148`),
/// `statisticalGraphError` (`:10151`). Unknown variants are a parse error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatisticalGraph {
    Data {
        json_data: String,
        zoom_token: String,
    },
    Async {
        token: String,
    },
    Error {
        error_message: String,
    },
}

impl StatisticalGraph {
    pub(crate) fn parse(value: &Value) -> Result<Self, ParseError> {
        match value.get("@type").and_then(Value::as_str) {
            Some("statisticalGraphData") => Ok(StatisticalGraph::Data {
                json_data: value
                    .get("json_data")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                zoom_token: value
                    .get("zoom_token")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }),
            Some("statisticalGraphAsync") => Ok(StatisticalGraph::Async {
                token: value
                    .get("token")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }),
            Some("statisticalGraphError") => Ok(StatisticalGraph::Error {
                error_message: value
                    .get("error_message")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            }),
            _ => Err(ParseError::MissingField),
        }
    }
}

/// Phase D2: `ChatStatisticsObjectType` (TDLib 1.8.67) —
/// `chatStatisticsObjectTypeMessage` (`schema/td_api.tl:10157`),
/// `chatStatisticsObjectTypeStory` (`schema/td_api.tl:10160`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatStatisticsObject {
    Message { message_id: i64 },
    Story { story_id: i32 },
}

/// Phase D2: `chatStatisticsInteractionInfo` (TDLib 1.8.67,
/// `schema/td_api.tl:10168`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatStatisticsInteractionInfo {
    pub object: ChatStatisticsObject,
    pub view_count: i32,
    pub forward_count: i32,
    pub reaction_count: i32,
}

/// Phase D2: `chatStatisticsMessageSenderInfo` (TDLib 1.8.67,
/// `schema/td_api.tl:10174`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatStatisticsMessageSenderInfo {
    pub user_id: i64,
    pub sent_message_count: i32,
    pub average_character_count: i32,
}

/// Phase D2: `chatStatisticsAdministratorActionsInfo` (TDLib 1.8.67,
/// `schema/td_api.tl:10181`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatStatisticsAdministratorActionsInfo {
    pub user_id: i64,
    pub deleted_message_count: i32,
    pub banned_user_count: i32,
    pub restricted_user_count: i32,
}

/// Phase D2: `chatStatisticsInviterInfo` (TDLib 1.8.67,
/// `schema/td_api.tl:10186`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatStatisticsInviterInfo {
    pub user_id: i64,
    pub added_member_count: i32,
}

/// Phase D2: `chatStatisticsChannel` (TDLib 1.8.67, `schema/td_api.tl:10233`).
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelStatistics {
    pub period_start: i32,
    pub period_end: i32,
    pub member_count: StatisticalValue,
    pub mean_message_view_count: StatisticalValue,
    pub mean_message_share_count: StatisticalValue,
    pub mean_message_reaction_count: StatisticalValue,
    pub mean_story_view_count: StatisticalValue,
    pub mean_story_share_count: StatisticalValue,
    pub mean_story_reaction_count: StatisticalValue,
    pub enabled_notifications_percentage: f64,
    pub member_count_graph: StatisticalGraph,
    pub join_graph: StatisticalGraph,
    pub mute_graph: StatisticalGraph,
    pub view_count_by_hour_graph: StatisticalGraph,
    pub view_count_by_source_graph: StatisticalGraph,
    pub join_by_source_graph: StatisticalGraph,
    pub language_graph: StatisticalGraph,
    pub message_interaction_graph: StatisticalGraph,
    pub message_reaction_graph: StatisticalGraph,
    pub story_interaction_graph: StatisticalGraph,
    pub story_reaction_graph: StatisticalGraph,
    pub instant_view_interaction_graph: StatisticalGraph,
    pub recent_interactions: Vec<ChatStatisticsInteractionInfo>,
}

/// Phase D2: `chatStatisticsSupergroup` (TDLib 1.8.67,
/// `schema/td_api.tl:10208`).
#[derive(Debug, Clone, PartialEq)]
pub struct SupergroupStatistics {
    pub period_start: i32,
    pub period_end: i32,
    pub member_count: StatisticalValue,
    pub message_count: StatisticalValue,
    pub viewer_count: StatisticalValue,
    pub sender_count: StatisticalValue,
    pub member_count_graph: StatisticalGraph,
    pub join_graph: StatisticalGraph,
    pub join_by_source_graph: StatisticalGraph,
    pub language_graph: StatisticalGraph,
    pub message_content_graph: StatisticalGraph,
    pub action_graph: StatisticalGraph,
    pub day_graph: StatisticalGraph,
    pub week_graph: StatisticalGraph,
    pub top_senders: Vec<ChatStatisticsMessageSenderInfo>,
    pub top_administrators: Vec<ChatStatisticsAdministratorActionsInfo>,
    pub top_inviters: Vec<ChatStatisticsInviterInfo>,
}

/// Phase D2: `ChatStatistics` (TDLib 1.8.67) — the `getChatStatistics`
/// response (`schema/td_api.tl:15760`). Revenue/star variants stay out of
/// this slice.
#[derive(Debug, Clone, PartialEq)]
pub enum ChatStatistics {
    Channel(Box<ChannelStatistics>),
    Supergroup(Box<SupergroupStatistics>),
}

pub(crate) fn parse_statistical_value(
    value: Option<&Value>,
) -> Result<StatisticalValue, ParseError> {
    // All `statisticalValue` fields on `chatStatisticsChannel` /
    // `chatStatisticsSupergroup` are required by the schema; a missing,
    // null, or mistyped value is a parse error rather than fabricated
    // zeros.
    let value = value
        .filter(|v| !v.is_null())
        .ok_or(ParseError::MissingField)?;
    StatisticalValue::parse(value)
}

pub(crate) fn parse_statistical_graph(
    value: Option<&Value>,
) -> Result<StatisticalGraph, ParseError> {
    let value = value
        .filter(|v| !v.is_null())
        .ok_or(ParseError::MissingField)?;
    StatisticalGraph::parse(value)
}

pub(crate) fn parse_statistics_period(value: Option<&Value>) -> Result<(i32, i32), ParseError> {
    // `dateRange start_date:int32 end_date:int32` (schema 1.8.67, line 10135).
    let value = value
        .filter(|v| !v.is_null())
        .ok_or(ParseError::MissingField)?;
    let start = value
        .get("start_date")
        .and_then(Value::as_i64)
        .ok_or(ParseError::MissingField)? as i32;
    let end = value
        .get("end_date")
        .and_then(Value::as_i64)
        .ok_or(ParseError::MissingField)? as i32;
    Ok((start, end))
}

pub(crate) fn parse_chat_statistics_object(value: Option<&Value>) -> Option<ChatStatisticsObject> {
    let value = value?;
    match value.get("@type").and_then(Value::as_str) {
        Some("chatStatisticsObjectTypeMessage") => Some(ChatStatisticsObject::Message {
            message_id: int53_or_zero(value.get("message_id")),
        }),
        Some("chatStatisticsObjectTypeStory") => Some(ChatStatisticsObject::Story {
            story_id: value.get("story_id").and_then(Value::as_i64).unwrap_or(0) as i32,
        }),
        _ => None,
    }
}

pub(crate) fn parse_chat_statistics_interaction_info(
    value: &Value,
) -> Option<ChatStatisticsInteractionInfo> {
    Some(ChatStatisticsInteractionInfo {
        object: parse_chat_statistics_object(value.get("object_type"))?,
        view_count: value.get("view_count").and_then(Value::as_i64).unwrap_or(0) as i32,
        forward_count: value
            .get("forward_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
        reaction_count: value
            .get("reaction_count")
            .and_then(Value::as_i64)
            .unwrap_or(0) as i32,
    })
}

/// Phase D2: parses `chatStatisticsChannel` / `chatStatisticsSupergroup`
/// into `ChatStatistics`. Unknown `ChatStatistics` variants are a parse
/// error (the panel renders an honest "unsupported" state).
pub(crate) fn parse_chat_statistics(value: &Value) -> Result<ChatStatistics, ParseError> {
    match value.get("@type").and_then(Value::as_str) {
        Some("chatStatisticsChannel") => {
            let (period_start, period_end) = parse_statistics_period(value.get("period"))?;
            let graph = |name: &str| parse_statistical_graph(value.get(name));
            Ok(ChatStatistics::Channel(Box::new(ChannelStatistics {
                period_start,
                period_end,
                member_count: parse_statistical_value(value.get("member_count"))?,
                mean_message_view_count: parse_statistical_value(
                    value.get("mean_message_view_count"),
                )?,
                mean_message_share_count: parse_statistical_value(
                    value.get("mean_message_share_count"),
                )?,
                mean_message_reaction_count: parse_statistical_value(
                    value.get("mean_message_reaction_count"),
                )?,
                mean_story_view_count: parse_statistical_value(value.get("mean_story_view_count"))?,
                mean_story_share_count: parse_statistical_value(
                    value.get("mean_story_share_count"),
                )?,
                mean_story_reaction_count: parse_statistical_value(
                    value.get("mean_story_reaction_count"),
                )?,
                enabled_notifications_percentage: value
                    .get("enabled_notifications_percentage")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0),
                member_count_graph: graph("member_count_graph")?,
                join_graph: graph("join_graph")?,
                mute_graph: graph("mute_graph")?,
                view_count_by_hour_graph: graph("view_count_by_hour_graph")?,
                view_count_by_source_graph: graph("view_count_by_source_graph")?,
                join_by_source_graph: graph("join_by_source_graph")?,
                language_graph: graph("language_graph")?,
                message_interaction_graph: graph("message_interaction_graph")?,
                message_reaction_graph: graph("message_reaction_graph")?,
                story_interaction_graph: graph("story_interaction_graph")?,
                story_reaction_graph: graph("story_reaction_graph")?,
                instant_view_interaction_graph: graph("instant_view_interaction_graph")?,
                recent_interactions: value
                    .get("recent_interactions")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(parse_chat_statistics_interaction_info)
                            .collect()
                    })
                    .unwrap_or_default(),
            })))
        }
        Some("chatStatisticsSupergroup") => {
            let (period_start, period_end) = parse_statistics_period(value.get("period"))?;
            let graph = |name: &str| parse_statistical_graph(value.get(name));
            Ok(ChatStatistics::Supergroup(Box::new(SupergroupStatistics {
                period_start,
                period_end,
                member_count: parse_statistical_value(value.get("member_count"))?,
                message_count: parse_statistical_value(value.get("message_count"))?,
                viewer_count: parse_statistical_value(value.get("viewer_count"))?,
                sender_count: parse_statistical_value(value.get("sender_count"))?,
                member_count_graph: graph("member_count_graph")?,
                join_graph: graph("join_graph")?,
                join_by_source_graph: graph("join_by_source_graph")?,
                language_graph: graph("language_graph")?,
                message_content_graph: graph("message_content_graph")?,
                action_graph: graph("action_graph")?,
                day_graph: graph("day_graph")?,
                week_graph: graph("week_graph")?,
                top_senders: value
                    .get("top_senders")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| ChatStatisticsMessageSenderInfo {
                                user_id: int53_or_zero(item.get("user_id")),
                                sent_message_count: item
                                    .get("sent_message_count")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0)
                                    as i32,
                                average_character_count: item
                                    .get("average_character_count")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0)
                                    as i32,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                top_administrators: value
                    .get("top_administrators")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| ChatStatisticsAdministratorActionsInfo {
                                user_id: int53_or_zero(item.get("user_id")),
                                deleted_message_count: item
                                    .get("deleted_message_count")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0)
                                    as i32,
                                banned_user_count: item
                                    .get("banned_user_count")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0)
                                    as i32,
                                restricted_user_count: item
                                    .get("restricted_user_count")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0)
                                    as i32,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                top_inviters: value
                    .get("top_inviters")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .map(|item| ChatStatisticsInviterInfo {
                                user_id: int53_or_zero(item.get("user_id")),
                                added_member_count: item
                                    .get("added_member_count")
                                    .and_then(Value::as_i64)
                                    .unwrap_or(0)
                                    as i32,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            })))
        }
        _ => Err(ParseError::MissingField),
    }
}
