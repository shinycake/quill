use super::*;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteGroupCallParticipantResult {
    /// `inviteGroupCallParticipantResultSuccess` — carries the
    /// invitation service message's chat/message ids (usable with
    /// `declineGroupCallInvitation` to cancel).
    Success {
        chat_id: i64,
        message_id: i64,
    },
    UserPrivacyRestricted,
    UserAlreadyParticipant,
    UserWasBanned,
}

/// Phase C1: `CallDiscardReason` (TDLib 1.8.67,
/// `schema/td_api.tl:6981`): `callDiscardReasonEmpty` (:6984),
/// `callDiscardReasonMissed` (:6987), `callDiscardReasonDeclined`
/// (:6990), `callDiscardReasonDisconnected` (:6993),
/// `callDiscardReasonHungUp` (:6996),
/// `callDiscardReasonUpgradeToGroupCall` (:6999, carries an invite
/// link — group calls are C3, the link is kept but unused).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallDiscardReason {
    Empty,
    Missed,
    Declined,
    Disconnected,
    HungUp,
    UpgradeToGroupCall { invite_link: String },
    Unknown(String),
}

impl CallDiscardReason {
    pub fn from_value(value: Option<&Value>) -> Self {
        let type_name = value
            .and_then(|v| v.get("@type"))
            .and_then(Value::as_str)
            .unwrap_or("");
        match type_name {
            "callDiscardReasonEmpty" => CallDiscardReason::Empty,
            "callDiscardReasonMissed" => CallDiscardReason::Missed,
            "callDiscardReasonDeclined" => CallDiscardReason::Declined,
            "callDiscardReasonDisconnected" => CallDiscardReason::Disconnected,
            "callDiscardReasonHungUp" => CallDiscardReason::HungUp,
            "callDiscardReasonUpgradeToGroupCall" => CallDiscardReason::UpgradeToGroupCall {
                invite_link: value
                    .and_then(|v| v.get("invite_link"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            },
            other => CallDiscardReason::Unknown(other.to_string()),
        }
    }

    /// Human-readable ended-call line shown on the call-end screen.
    /// `is_outgoing` disambiguates "Declined" (they declined ours vs.
    /// we declined theirs).
    pub fn summary(&self, is_outgoing: bool) -> String {
        match self {
            CallDiscardReason::Empty => "Call ended".to_string(),
            CallDiscardReason::Missed => {
                if is_outgoing {
                    "Call not answered".to_string()
                } else {
                    "Missed call".to_string()
                }
            }
            CallDiscardReason::Declined => {
                if is_outgoing {
                    "Declined".to_string()
                } else {
                    "You declined the call".to_string()
                }
            }
            CallDiscardReason::Disconnected => "Call disconnected".to_string(),
            CallDiscardReason::HungUp => "Call ended".to_string(),
            CallDiscardReason::UpgradeToGroupCall { .. } => "Upgraded to a group call".to_string(),
            CallDiscardReason::Unknown(_) => "Call ended".to_string(),
        }
    }
}

/// Phase C1: `CallState` (TDLib 1.8.67, `schema/td_api.tl:7051`):
/// `callStatePending` (:7058, `is_created` / `is_received`),
/// `callStateExchangingKeys` (:7063), `callStateReady` (:7066),
/// `callStateHangingUp` (:7077), `callStateDiscarded` (:7080 —
/// `reason` / `need_rating` / `need_debug_information` / `need_log`),
/// `callStateError` (:7081 — the `error` wrapper; only its numeric
/// code is kept, never the message text, which can contain
/// secrets).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRtcServer {
    pub id: u64,
    pub ipv4: String,
    pub ipv6: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub turn: bool,
    pub stun: bool,
    pub tcp: bool,
    pub peer_tag: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadyParams {
    pub encryption_key: Vec<u8>,
    pub library_versions: Vec<String>,
    pub custom_parameters: String,
    pub servers: Vec<ParsedRtcServer>,
    pub allow_p2p: bool,
    /// `callStateReady.emojis` (schema 1.8.67, :7068): the 4-emoji
    /// E2E fingerprint, shown on the 1:1 call card.
    pub emojis: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallState {
    Pending {
        is_created: bool,
        is_received: bool,
    },
    ExchangingKeys,
    Ready,
    HangingUp,
    Discarded {
        reason: CallDiscardReason,
        need_rating: bool,
        need_debug_information: bool,
        need_log: bool,
    },
    /// TDLib error code only — the `message` text is deliberately not
    /// stored (it can contain phone numbers or other secrets).
    Error {
        code: i32,
    },
    Unknown(String),
}

impl CallState {
    pub fn from_value(value: Option<&Value>) -> Self {
        let value = match value {
            Some(v) => v,
            None => return CallState::Unknown(String::new()),
        };
        let type_name = value.get("@type").and_then(Value::as_str).unwrap_or("");
        match type_name {
            "callStatePending" => CallState::Pending {
                is_created: value
                    .get("is_created")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                is_received: value
                    .get("is_received")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
            "callStateExchangingKeys" => CallState::ExchangingKeys,
            "callStateReady" => CallState::Ready,
            "callStateHangingUp" => CallState::HangingUp,
            "callStateDiscarded" => CallState::Discarded {
                reason: CallDiscardReason::from_value(value.get("reason")),
                need_rating: value
                    .get("need_rating")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                need_debug_information: value
                    .get("need_debug_information")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                need_log: value
                    .get("need_log")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            },
            "callStateError" => {
                let error = value.get("error");
                CallState::Error {
                    code: error
                        .and_then(|e| e.get("code"))
                        .and_then(Value::as_i64)
                        .unwrap_or(0)
                        .sat_i32(),
                }
            }
            other => CallState::Unknown(other.to_string()),
        }
    }

    /// Terminal states end the tracked call. `Unknown` is deliberately
    /// *not* terminal — a future state the schema doesn't know yet
    /// must not silently drop a live call; the UI labels it honestly.
    pub fn is_terminal(&self) -> bool {
        matches!(self, CallState::Discarded { .. } | CallState::Error { .. })
    }
}

pub(crate) fn parse_rtc_server(value: &Value) -> Option<ParsedRtcServer> {
    let kind = value.get("type")?;
    let type_name = kind.get("@type").and_then(Value::as_str).unwrap_or("");
    let (username, password, turn, stun, tcp, peer_tag) = match type_name {
        "callServerTypeTelegramReflector" => (
            String::new(),
            String::new(),
            true,
            false,
            kind.get("is_tcp").and_then(Value::as_bool).unwrap_or(false),
            kind.get("peer_tag")
                .and_then(Value::as_str)
                .and_then(|tag| STANDARD.decode(tag).ok())
                .unwrap_or_default(),
        ),
        "callServerTypeWebrtc" => (
            kind.get("username")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            kind.get("password")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            kind.get("supports_turn")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            kind.get("supports_stun")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            false,
            Vec::new(),
        ),
        _ => return None,
    };
    Some(ParsedRtcServer {
        id: value
            .get("id")
            .and_then(|id| id.as_u64().or_else(|| id.as_str()?.parse().ok()))?,
        ipv4: value
            .get("ip_address")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        ipv6: value
            .get("ipv6_address")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        port: value.get("port")?.as_u64()?.try_into().ok()?,
        username,
        password,
        turn,
        stun,
        tcp,
        peer_tag,
    })
}

/// Phase C1: `call` subset (TDLib 1.8.67, `schema/td_api.tl:7287`):
/// `call id:int32 unique_id:int64 user_id:int53 is_outgoing:Bool
/// is_video:Bool state:CallState = Call;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCall {
    pub id: i32,
    pub unique_id: i64,
    pub user_id: i64,
    pub is_outgoing: bool,
    pub is_video: bool,
    pub state: CallState,
    pub ready: Option<ReadyParams>,
}

pub(crate) fn parse_call(value: Option<&Value>) -> Option<ParsedCall> {
    let value = value?;
    let state_value = value.get("state");
    let ready = state_value
        .filter(|state| state.get("@type").and_then(Value::as_str) == Some("callStateReady"))
        .map(|state| ReadyParams {
            library_versions: state
                .get("protocol")
                .and_then(|protocol| protocol.get("library_versions"))
                .and_then(Value::as_array)
                .map(|versions| {
                    versions
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            custom_parameters: state
                .get("custom_parameters")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            encryption_key: state
                .get("encryption_key")
                .and_then(Value::as_str)
                .and_then(|key| STANDARD.decode(key).ok())
                .unwrap_or_default(),
            servers: state
                .get("servers")
                .and_then(Value::as_array)
                .map(|servers| servers.iter().filter_map(parse_rtc_server).collect())
                .unwrap_or_default(),
            allow_p2p: state
                .get("allow_p2p")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            emojis: state
                .get("emojis")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        });
    Some(ParsedCall {
        id: value
            .get("id")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        unique_id: int53_or_zero(value.get("unique_id")),
        user_id: int53(value.get("user_id")).ok()?,
        is_outgoing: value
            .get("is_outgoing")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_video: value
            .get("is_video")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        state: CallState::from_value(state_value),
        ready,
    })
}

/// Phase C3a: `groupCall` subset (TDLib 1.8.67, `schema/td_api.tl:7154`).
/// Only the fields the signaling surface needs are parsed: identity,
/// join state, admin rights, participant bookkeeping, self video
/// state, and recent speakers. Video/RTMP/record fields are dropped —
/// media transport is Phase C2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGroupCall {
    pub id: i32,
    pub title: String,
    pub is_active: bool,
    pub is_video_chat: bool,
    pub is_live_story: bool,
    pub is_joined: bool,
    pub need_rejoin: bool,
    pub is_owned: bool,
    pub can_be_managed: bool,
    pub participant_count: i32,
    pub loaded_all_participants: bool,
    /// `(participant_id, is_speaking)` from
    /// `groupCallRecentSpeaker` (schema 1.8.67, line 7118).
    pub recent_speakers: Vec<(MessageSender, bool)>,
    pub is_my_video_enabled: bool,
    pub is_my_video_paused: bool,
    pub can_enable_video: bool,
    pub mute_new_participants: bool,
    pub can_toggle_mute_new_participants: bool,
    pub scheduled_start_date: i32,
    /// `enabled_start_notification` (schema 1.8.67, :7154): the current
    /// user gets a notification when a scheduled video chat starts —
    /// toggled via `toggleVideoChatEnabledStartNotification` (:14282).
    pub enabled_start_notification: bool,
    /// Phase C2h: message permissions (schema 1.8.67, lines
    /// 7147-7150) — gate the in-call chat UI.
    pub can_send_messages: bool,
    pub are_messages_allowed: bool,
    pub can_toggle_are_messages_allowed: bool,
    pub can_delete_messages: bool,
    /// Phase C2h: recording state (schema 1.8.67, lines 7151-7152):
    /// ongoing recording duration in seconds (0 = none) and whether a
    /// video file is being recorded.
    pub record_duration: i32,
    pub is_video_recorded: bool,
}

pub(crate) fn parse_group_call(value: Option<&Value>) -> Option<ParsedGroupCall> {
    let value = value?;
    let recent_speakers = value
        .get("recent_speakers")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let participant_id = parse_message_sender(item.get("participant_id")).ok()?;
                    let is_speaking = item
                        .get("is_speaking")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    Some((participant_id, is_speaking))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(ParsedGroupCall {
        id: value
            .get("id")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        is_active: value
            .get("is_active")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_video_chat: value
            .get("is_video_chat")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_live_story: value
            .get("is_live_story")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_joined: value
            .get("is_joined")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        need_rejoin: value
            .get("need_rejoin")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_owned: value
            .get("is_owned")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_be_managed: value
            .get("can_be_managed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        participant_count: value
            .get("participant_count")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        loaded_all_participants: value
            .get("loaded_all_participants")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        recent_speakers,
        is_my_video_enabled: value
            .get("is_my_video_enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        is_my_video_paused: value
            .get("is_my_video_paused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_enable_video: value
            .get("can_enable_video")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        mute_new_participants: value
            .get("mute_new_participants")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_toggle_mute_new_participants: value
            .get("can_toggle_mute_new_participants")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        scheduled_start_date: value
            .get("scheduled_start_date")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        enabled_start_notification: value
            .get("enabled_start_notification")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_send_messages: value
            .get("can_send_messages")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        are_messages_allowed: value
            .get("are_messages_allowed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_toggle_are_messages_allowed: value
            .get("can_toggle_are_messages_allowed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_delete_messages: value
            .get("can_delete_messages")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        record_duration: value
            .get("record_duration")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        is_video_recorded: value
            .get("is_video_recorded")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Phase C2h: `groupCallMessage` subset (TDLib 1.8.67,
/// `schema/td_api.tl:7200`):
/// `groupCallMessage message_id:int32 sender_id:MessageSender date:int32
/// text:formattedText paid_message_star_count:int53 is_from_owner:Bool
/// can_be_deleted:Bool = GroupCallMessage;`
/// Entities are dropped — plain text only (the in-call chat is a
/// minimal list + composer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGroupCallMessage {
    pub message_id: i32,
    pub sender_id: MessageSender,
    pub date: i32,
    pub text: String,
    pub is_from_owner: bool,
    pub can_be_deleted: bool,
}

pub(crate) fn parse_group_call_message(value: Option<&Value>) -> Option<ParsedGroupCallMessage> {
    let value = value?;
    Some(ParsedGroupCallMessage {
        message_id: value.get("message_id").and_then(Value::as_i64)?.sat_i32(),
        sender_id: parse_message_sender(value.get("sender_id")).ok()?,
        date: value
            .get("date")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        text: parse_formatted_text(value.get("text")),
        is_from_owner: value
            .get("is_from_owner")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        can_be_deleted: value
            .get("can_be_deleted")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Phase C2g: `groupCallVideoSourceGroup` (TDLib 1.8.67,
/// `schema/td_api.tl:7157`):
/// `groupCallVideoSourceGroup semantics:string source_ids:vector<int32>
/// = GroupCallVideoSourceGroup;`
/// The `source_ids` are the RTP synchronization sources of one video
/// channel; ntgcalls' `ntg_add_incoming_video` subscribes by endpoint +
/// these groups, and incoming frames are attributed to the participant
/// by matching `ntg_frame.ssrc` against them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupCallVideoSourceGroup {
    pub semantics: String,
    pub source_ids: Vec<u32>,
}

/// Phase C2g: `groupCallParticipantVideoInfo` (TDLib 1.8.67,
/// `schema/td_api.tl:7163`):
/// `groupCallParticipantVideoInfo source_groups:vector<groupCallVideoSourceGroup>
/// endpoint_id:string is_paused:Bool = GroupCallParticipantVideoInfo;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupCallVideoInfo {
    pub endpoint_id: String,
    pub is_paused: bool,
    pub source_groups: Vec<GroupCallVideoSourceGroup>,
}

pub(crate) fn parse_group_call_video_info(value: Option<&Value>) -> Option<GroupCallVideoInfo> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    let source_groups = value
        .get("source_groups")
        .and_then(Value::as_array)
        .map(|groups| {
            groups
                .iter()
                .map(|group| GroupCallVideoSourceGroup {
                    semantics: group
                        .get("semantics")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    source_ids: group
                        .get("source_ids")
                        .and_then(Value::as_array)
                        .map(|ids| {
                            ids.iter()
                                .filter_map(Value::as_i64)
                                .map(|id| id as u32)
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    Some(GroupCallVideoInfo {
        endpoint_id: value
            .get("endpoint_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        is_paused: value
            .get("is_paused")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        source_groups,
    })
}

/// Phase C3a: `groupCallParticipant` subset (TDLib 1.8.67,
/// `schema/td_api.tl:7184`). Video info fields
/// (`video_info`/`screen_sharing_video_info`) are parsed in Phase C2g
/// and drive the engine's incoming-video subscriptions. An empty
/// `order` means the participant must be removed from the list
/// (schema note on `order`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGroupCallParticipant {
    pub participant_id: MessageSender,
    pub audio_source_id: i32,
    pub is_current_user: bool,
    pub is_speaking: bool,
    pub is_hand_raised: bool,
    pub can_be_muted_for_all_users: bool,
    pub can_be_unmuted_for_all_users: bool,
    pub can_be_muted_for_current_user: bool,
    pub can_be_unmuted_for_current_user: bool,
    pub is_muted_for_all_users: bool,
    pub is_muted_for_current_user: bool,
    pub can_unmute_self: bool,
    pub volume_level: i32,
    pub order: String,
    /// Phase C3a: `video_info != null` / `screen_sharing_video_info !=
    /// null` (schema 1.8.67, line 7184).
    pub video_enabled: bool,
    pub screen_sharing_enabled: bool,
    /// Phase C2g: the parsed `video_info` / `screen_sharing_video_info`
    /// channels; `None` matches the `*_enabled` flags above. The engine
    /// subscribes to `video_info` endpoints via
    /// `ntg_add_incoming_video`.
    pub video_info: Option<GroupCallVideoInfo>,
    pub screen_sharing_video_info: Option<GroupCallVideoInfo>,
}

pub(crate) fn parse_group_call_participant(
    value: Option<&Value>,
) -> Option<ParsedGroupCallParticipant> {
    let value = value?;
    let flag = |name: &str| value.get(name).and_then(Value::as_bool).unwrap_or(false);
    Some(ParsedGroupCallParticipant {
        participant_id: parse_message_sender(value.get("participant_id")).ok()?,
        audio_source_id: value
            .get("audio_source_id")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        is_current_user: flag("is_current_user"),
        is_speaking: flag("is_speaking"),
        is_hand_raised: flag("is_hand_raised"),
        can_be_muted_for_all_users: flag("can_be_muted_for_all_users"),
        can_be_unmuted_for_all_users: flag("can_be_unmuted_for_all_users"),
        can_be_muted_for_current_user: flag("can_be_muted_for_current_user"),
        can_be_unmuted_for_current_user: flag("can_be_unmuted_for_current_user"),
        is_muted_for_all_users: flag("is_muted_for_all_users"),
        is_muted_for_current_user: flag("is_muted_for_current_user"),
        can_unmute_self: flag("can_unmute_self"),
        volume_level: value
            .get("volume_level")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        order: value
            .get("order")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        video_enabled: value.get("video_info").is_some_and(|info| !info.is_null()),
        screen_sharing_enabled: value
            .get("screen_sharing_video_info")
            .is_some_and(|info| !info.is_null()),
        video_info: parse_group_call_video_info(value.get("video_info")),
        screen_sharing_video_info: parse_group_call_video_info(
            value.get("screen_sharing_video_info"),
        ),
    })
}

/// Phase C3a: `videoChat` (TDLib 1.8.67, `schema/td_api.tl:3579`):
/// `videoChat group_call_id:int32 has_participants:Bool
/// default_participant_id:MessageSender = VideoChat;`
/// `group_call_id` is 0 when the chat has no active video chat.
/// `default_participant_id` is the "join as" the user last chose here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedVideoChat {
    pub group_call_id: i32,
    pub has_participants: bool,
    pub default_participant_id: Option<MessageSender>,
}

pub(crate) fn parse_video_chat(value: Option<&Value>) -> Option<ParsedVideoChat> {
    let value = value?;
    Some(ParsedVideoChat {
        group_call_id: value
            .get("group_call_id")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .sat_i32(),
        has_participants: value
            .get("has_participants")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        default_participant_id: value
            .get("default_participant_id")
            .and_then(|v| parse_message_sender(Some(v)).ok()),
    })
}
