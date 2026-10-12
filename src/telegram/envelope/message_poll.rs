use super::poll_option_media::parse_poll_option_media;
use super::*;
use crate::text::TextEntity;
use serde_json::Value;

/// `pollOption` (TDLib 1.8.68, `schema/td_api.tl:456`). `recent_voter_ids`,
/// `is_being_chosen`, `author`, and `addition_date` are not kept — Quill
/// renders the text, the picture, the bar, the count, and the chosen mark.
/// Note: `id` is a string identifier for the option, while `setPollAnswer`
/// takes 0-based **indexes** into `poll.options` (schema line 12932).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollOption {
    pub id: String,
    pub text: String,
    /// The option text's entities: TDLib allows only custom emoji.
    pub entities: Vec<TextEntity>,
    /// `pollOption.media`: the picture, clip, sticker, link or place
    /// shown beside the text.
    pub media: Option<PollOptionMedia>,
    pub voter_count: i32,
    pub vote_percentage: i32,
    pub is_chosen: bool,
}

impl PollOption {
    /// An option with nothing but its text (a locally added one).
    pub fn plain(id: String, text: String) -> Self {
        Self {
            id,
            text,
            entities: Vec::new(),
            media: None,
            voter_count: 0,
            vote_percentage: 0,
            is_chosen: false,
        }
    }
}

/// `pollType` (TDLib 1.8.67, `schema/td_api.tl:468` / `:475`).
/// `pollTypeQuiz.explanation` is shown after the user answers (or on the
/// lamp-icon tap, per the schema doc); `explanation_media` is not kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollType {
    Regular,
    Quiz {
        correct_option_ids: Vec<i32>,
        explanation: String,
    },
}

/// `pollVoteRestrictionReason*` (TDLib 1.8.67, `schema/td_api.tl:494`-`:510`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollVoteRestrictionReason {
    /// `pollVoteRestrictionReasonClosed` — the poll is closed.
    Closed,
    /// `pollVoteRestrictionReasonYetUnsent` — the poll isn't sent yet.
    YetUnsent,
    /// `pollVoteRestrictionReasonScheduled` — from a scheduled message.
    Scheduled,
    /// `pollVoteRestrictionReasonCountryRestricted` — the user's country
    /// can't vote here (`country_code` is the ISO 3166-1 alpha-2 code).
    CountryRestricted { country_code: String },
    /// `pollVoteRestrictionReasonMembershipRequired` — the user must have
    /// joined the chat for at least a day.
    MembershipRequired { chat_id: i64 },
    /// `pollVoteRestrictionReasonOther` — some other reason.
    Other,
}

/// `poll` (TDLib 1.8.67, `schema/td_api.tl:711`). `recent_voter_ids`,
/// `can_see_results`, `members_only`, `country_codes`, `option_order`,
/// `open_period`, and `close_date` are not kept in this slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Poll {
    pub id: i64,
    pub question: String,
    pub options: Vec<PollOption>,
    pub total_voter_count: i32,
    pub is_anonymous: bool,
    pub allows_multiple_answers: bool,
    pub allows_revoting: bool,
    pub is_closed: bool,
    pub poll_type: PollType,
    /// `can_get_voters` (schema line 698): `getPollVoters` may be used.
    pub can_get_voters: bool,
    /// `vote_restriction_reason` (schema line 711): why the current user
    /// can't vote; `None` when the user can vote.
    pub vote_restriction_reason: Option<PollVoteRestrictionReason>,
    /// `can_see_results` (schema line 711): false while the creator hid
    /// the results until the poll closes (`hide_results_until_closes`).
    pub can_see_results: bool,
    /// `members_only` (schema line 711): only subscribers can vote.
    pub members_only: bool,
    /// `open_period` (seconds the poll stays open) and `close_date`
    /// (absolute unix time it closes); 0 when the poll has no deadline.
    pub open_period: i32,
    pub close_date: i32,
}

impl Poll {
    /// 0-based indexes of the options currently marked chosen — the values
    /// `setPollAnswer` expects (schema line 12932).
    pub fn chosen_indexes(&self) -> Vec<i32> {
        self.options
            .iter()
            .enumerate()
            .filter(|(_, option)| option.is_chosen)
            .map(|(index, _)| index as i32)
            .collect()
    }

    /// The poll can receive a vote from the user. A non-null
    /// `vote_restriction_reason` (`pollVoteRestrictionReason*`, schema
    /// `td_api.tl:494`-`:510`) also blocks voting.
    pub fn can_vote(&self) -> bool {
        !self.is_closed && self.vote_restriction_reason.is_none()
    }
}

/// `messagePoll` (TDLib 1.8.67, `schema/td_api.tl:5241`). `media` is not
/// rendered in this slice (photo/document/… attachments on polls stay out).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PollContent {
    pub poll: Poll,
    pub description: String,
    /// `messagePoll.can_add_option` (schema line 5240): the user may add
    /// an option with `addPollOption`.
    pub can_add_option: bool,
}

/// `pollOption` (TDLib 1.8.68, `schema/td_api.tl:456`), with the files its
/// media names.
pub(crate) fn parse_poll_option(value: &Value) -> (PollOption, Vec<ParsedFile>) {
    let (media, files) = parse_poll_option_media(value.get("media"));
    let (text, entities) = parse_caption(value.get("text"));
    let option = PollOption {
        id: value
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        text,
        entities,
        media,
        voter_count: value
            .get("voter_count")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        vote_percentage: value
            .get("vote_percentage")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        is_chosen: value
            .get("is_chosen")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };
    (option, files)
}

/// `pollType` (TDLib 1.8.67, `schema/td_api.tl:468` / `:475`).
/// `explanation` is the `formattedText.text` of `pollTypeQuiz.explanation`.
pub(crate) fn parse_poll_type(value: Option<&Value>) -> PollType {
    match value.and_then(|v| v.get("@type")).and_then(Value::as_str) {
        Some("pollTypeQuiz") => PollType::Quiz {
            correct_option_ids: value
                .and_then(|v| v.get("correct_option_ids"))
                .and_then(Value::as_array)
                .map(|ids| {
                    ids.iter()
                        .filter_map(|id| id.as_i64())
                        .map(|n| n.sat_i32())
                        .collect()
                })
                .unwrap_or_default(),
            explanation: value
                .map(|v| parse_formatted_text(v.get("explanation")))
                .unwrap_or_default(),
        },
        _ => PollType::Regular,
    }
}

/// `pollVoteRestrictionReason*` (TDLib 1.8.67, `schema/td_api.tl:494`-`:510`).
/// `None` when the field is absent/null (the user can vote) or the
/// constructor is unknown (never rendered as a fabricated reason).
pub(crate) fn parse_poll_vote_restriction_reason(
    value: Option<&Value>,
) -> Option<PollVoteRestrictionReason> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    match value.get("@type").and_then(Value::as_str) {
        Some("pollVoteRestrictionReasonClosed") => Some(PollVoteRestrictionReason::Closed),
        Some("pollVoteRestrictionReasonYetUnsent") => Some(PollVoteRestrictionReason::YetUnsent),
        Some("pollVoteRestrictionReasonScheduled") => Some(PollVoteRestrictionReason::Scheduled),
        Some("pollVoteRestrictionReasonCountryRestricted") => {
            Some(PollVoteRestrictionReason::CountryRestricted {
                country_code: value
                    .get("country_code")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        }
        Some("pollVoteRestrictionReasonMembershipRequired") => {
            Some(PollVoteRestrictionReason::MembershipRequired {
                chat_id: value.get("chat_id").and_then(Value::as_i64).unwrap_or(0),
            })
        }
        Some("pollVoteRestrictionReasonOther") => Some(PollVoteRestrictionReason::Other),
        _ => None,
    }
}

/// `poll` (TDLib 1.8.67, `schema/td_api.tl:711`). `None` when the `poll`
/// object itself is missing or null.
pub(crate) fn parse_poll(value: Option<&Value>) -> Option<Poll> {
    parse_poll_with_files(value).map(|(poll, _)| poll)
}

/// [`parse_poll`] with the files the options' media name.
pub(crate) fn parse_poll_with_files(value: Option<&Value>) -> Option<(Poll, Vec<ParsedFile>)> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    let mut files = Vec::new();
    let options = value
        .get("options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|option| {
            let (option, option_files) = parse_poll_option(option);
            files.extend(option_files);
            option
        })
        .collect();
    let poll = Poll {
        id: int64(value.get("id")).unwrap_or(0),
        question: parse_formatted_text(value.get("question")),
        options,
        total_voter_count: value
            .get("total_voter_count")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        is_anonymous: value
            .get("is_anonymous")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        allows_multiple_answers: value
            .get("allows_multiple_answers")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        allows_revoting: value
            .get("allows_revoting")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_closed: value
            .get("is_closed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        poll_type: parse_poll_type(value.get("type")),
        can_get_voters: value
            .get("can_get_voters")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        vote_restriction_reason: parse_poll_vote_restriction_reason(
            value.get("vote_restriction_reason"),
        ),
        // Older payloads omit `can_see_results`; default to visible.
        can_see_results: value
            .get("can_see_results")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        members_only: value
            .get("members_only")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        open_period: value
            .get("open_period")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        close_date: value
            .get("close_date")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
    };
    Some((poll, files))
}

/// `messagePoll` (TDLib 1.8.67, `schema/td_api.tl:5241`).
pub(crate) fn parse_message_poll(value: &Value) -> (MessageContent, Vec<ParsedFile>) {
    match parse_poll_with_files(value.get("poll")) {
        Some((poll, files)) => (
            MessageContent::Poll(PollContent {
                poll,
                description: parse_formatted_text(value.get("description")),
                can_add_option: value
                    .get("can_add_option")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            }),
            files,
        ),
        None => (
            MessageContent::Unsupported {
                type_name: "messagePoll".into(),
            },
            Vec::new(),
        ),
    }
}
