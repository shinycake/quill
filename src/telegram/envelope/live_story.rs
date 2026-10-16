//! Live story updates and answers (TDLib 1.8.68): top donors, the paid
//! comment levels, paid reactions and the streamer. Wrapped as
//! [`CallsPayload::LiveStory`].
use crate::paid_reaction::PaidReactor;
use crate::telegram::envelope::*;
use serde_json::Value;

/// `liveStoryDonors total_star_count:int53 top_donors:vector<paidReactor>`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveStoryDonors {
    pub total_star_count: i64,
    /// Most Stars first.
    pub top_donors: Vec<PaidReactor>,
}

/// `groupCallMessageLevel`: what a paid comment of at least
/// `min_star_count` Stars gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GroupCallMessageLevel {
    pub min_star_count: i64,
    /// Seconds the comment stays pinned.
    pub pin_duration: i32,
    pub max_text_length: i32,
    pub max_custom_emoji_count: i32,
    /// RGB.
    pub first_color: u32,
    pub second_color: u32,
    pub background_color: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LiveStoryPayload {
    /// `updateLiveStoryTopDonors`.
    UpdateTopDonors {
        group_call_id: i32,
        donors: LiveStoryDonors,
    },
    /// `liveStoryDonors`: the `getLiveStoryTopDonors` answer.
    Donors(LiveStoryDonors),
    /// `updateGroupCallMessageLevels`, in decreasing `min_star_count`.
    UpdateMessageLevels(Vec<GroupCallMessageLevel>),
    /// `updateNewGroupCallPaidReaction`.
    UpdateNewPaidReaction {
        group_call_id: i32,
        sender_id: MessageSender,
        star_count: i64,
    },
    /// `groupCallParticipant`: the `getLiveStoryStreamer` answer.
    Streamer(ParsedGroupCallParticipant),
}

/// The live story TDLib types; `Ok(None)` for any other type.
pub(crate) fn parse_live_story_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        "updateLiveStoryTopDonors" => LiveStoryPayload::UpdateTopDonors {
            group_call_id: group_call_id(value)?,
            donors: parse_donors(value.get("donors")),
        },
        "liveStoryDonors" => LiveStoryPayload::Donors(parse_donors(Some(value))),
        "updateGroupCallMessageLevels" => LiveStoryPayload::UpdateMessageLevels(
            value
                .get("levels")
                .and_then(Value::as_array)
                .map(|levels| levels.iter().map(parse_level).collect())
                .unwrap_or_default(),
        ),
        "updateNewGroupCallPaidReaction" => LiveStoryPayload::UpdateNewPaidReaction {
            group_call_id: group_call_id(value)?,
            sender_id: parse_message_sender(value.get("sender_id"))?,
            star_count: value
                .get("star_count")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .max(0),
        },
        "groupCallParticipant" => LiveStoryPayload::Streamer(
            parse_group_call_participant(Some(value)).ok_or(ParseError::MissingField)?,
        ),
        _ => return Ok(None),
    };
    Ok(Some(EnvelopePayload::Calls(CallsPayload::LiveStory(
        payload,
    ))))
}

fn group_call_id(value: &Value) -> Result<i32, ParseError> {
    Ok(value
        .get("group_call_id")
        .and_then(Value::as_i64)
        .ok_or(ParseError::MissingField)?
        .sat_i32())
}

fn parse_donors(value: Option<&Value>) -> LiveStoryDonors {
    LiveStoryDonors {
        total_star_count: value
            .and_then(|v| v.get("total_star_count"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .max(0),
        top_donors: parse_paid_reactors(value.and_then(|v| v.get("top_donors"))),
    }
}

fn parse_level(value: &Value) -> GroupCallMessageLevel {
    let int = |key: &str| value.get(key).and_then(Value::as_i64).unwrap_or(0);
    let color = |key: &str| u32::try_from(int(key)).unwrap_or(0) & 0x00FF_FFFF;
    GroupCallMessageLevel {
        min_star_count: int("min_star_count").max(0),
        pin_duration: int("pin_duration").sat_i32().max(0),
        max_text_length: int("max_text_length").sat_i32().max(0),
        max_custom_emoji_count: int("max_custom_emoji_count").sat_i32().max(0),
        first_color: color("first_color"),
        second_color: color("second_color"),
        background_color: color("background_color"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Option<LiveStoryPayload> {
        let value: Value = serde_json::from_str(json).ok()?;
        let type_name = value.get("@type")?.as_str()?.to_string();
        match parse_live_story_payload(&type_name, &value) {
            Ok(Some(EnvelopePayload::Calls(CallsPayload::LiveStory(payload)))) => Some(payload),
            _ => None,
        }
    }

    #[test]
    fn top_donors_parse_most_stars_first() {
        let payload = parse(
            r#"{"@type":"updateLiveStoryTopDonors","group_call_id":42,"donors":{"@type":"liveStoryDonors","total_star_count":300,"top_donors":[
                {"@type":"paidReactor","sender_id":{"@type":"messageSenderUser","user_id":7},"star_count":50,"is_top":true,"is_me":false,"is_anonymous":false},
                {"@type":"paidReactor","sender_id":{"@type":"messageSenderUser","user_id":8},"star_count":250,"is_top":true,"is_me":true,"is_anonymous":false}]}}"#,
        );
        let Some(LiveStoryPayload::UpdateTopDonors {
            group_call_id,
            donors,
        }) = payload
        else {
            panic!("not top donors: {payload:?}");
        };
        assert_eq!(group_call_id, 42);
        assert_eq!(donors.total_star_count, 300);
        let first = donors.top_donors.first().map(|d| (d.star_count, d.is_me));
        assert_eq!(first, Some((250, true)));
    }

    #[test]
    fn levels_keep_their_colors_and_limits() {
        let payload = parse(
            r#"{"@type":"updateGroupCallMessageLevels","levels":[{"@type":"groupCallMessageLevel","min_star_count":50,"pin_duration":120,"max_text_length":80,"max_custom_emoji_count":2,"first_color":4629483,"second_color":20622,"background_color":20622}]}"#,
        );
        let Some(LiveStoryPayload::UpdateMessageLevels(levels)) = payload else {
            panic!("not levels: {payload:?}");
        };
        let level = levels.first().copied();
        assert_eq!(level.map(|l| l.min_star_count), Some(50));
        assert_eq!(level.map(|l| l.first_color), Some(0x46A3EB));
        assert_eq!(level.map(|l| l.pin_duration), Some(120));
    }

    #[test]
    fn a_paid_reaction_and_the_streamer_parse() {
        let reaction = parse(
            r#"{"@type":"updateNewGroupCallPaidReaction","group_call_id":42,"sender_id":{"@type":"messageSenderChat","chat_id":-5},"star_count":10}"#,
        );
        assert_eq!(
            reaction,
            Some(LiveStoryPayload::UpdateNewPaidReaction {
                group_call_id: 42,
                sender_id: MessageSender::Chat { chat_id: -5 },
                star_count: 10,
            })
        );
        let streamer = parse(
            r#"{"@type":"groupCallParticipant","participant_id":{"@type":"messageSenderChat","chat_id":-5},"video_info":{"@type":"groupCallParticipantVideoInfo","source_groups":[],"endpoint_id":"ep","is_paused":false},"order":"1"}"#,
        );
        let Some(LiveStoryPayload::Streamer(participant)) = streamer else {
            panic!("not a streamer");
        };
        assert_eq!(
            participant.video_info.map(|info| info.endpoint_id),
            Some("ep".to_string())
        );
    }
}
