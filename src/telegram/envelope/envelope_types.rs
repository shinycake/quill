use super::*;
use crate::ids::{MessageId, RequestId};
use serde::Deserialize;
use serde_json::Value;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    pub type_name: String,
    pub extra: Option<RequestId>,
    pub client_id: Option<i32>,
    pub payload: EnvelopePayload,
}

/// MED4: `optionValue*` (TDLib 1.8.67, `schema/td_api.tl:8889`).
#[derive(Debug, Clone, PartialEq)]
pub enum OptionValue {
    Boolean(bool),
    Integer(i64),
    String(String),
    Empty,
}

/// One parsed TDLib object. Every variant but the generic answers wraps a
/// domain enum (`src/telegram/envelope/domains/<domain>/mod.rs`); match
/// e.g. `EnvelopePayload::Stories(StoriesPayload::UpdateStory { .. })`.
#[derive(Clone, PartialEq)]
pub enum EnvelopePayload {
    /// Sign-in, registration and the TDLib session lifecycle.
    Auth(AuthPayload),
    /// Bots: inline queries, callback buttons, commands, games and login URLs.
    Bots(BotsPayload),
    /// One-to-one calls, group calls and video chats.
    Calls(CallsPayload),
    /// The chat list: loading, folders, archive and pins.
    ChatList(ChatListPayload),
    /// Chat-level look and actions: backgrounds, themes, deep links, action bar.
    Chats(ChatsPayload),
    /// Options, connection state and scalar answers shared by many requests.
    Common(CommonPayload),
    /// Groups and channels: members, admin rights, invite links, join requests, boosts, communities.
    Groups(GroupsPayload),
    /// Files, downloads, uploads and the shared-media gallery.
    Media(MediaPayload),
    /// History, sending, editing, reactions, polls, translation and message menus.
    Messages(MessagesPayload),
    /// Payments, Premium, Stars and gifts.
    Payments(PaymentsPayload),
    /// Global and in-chat search, top chats and date jumps.
    Search(SearchPayload),
    /// Settings: privacy, notifications, sessions, storage, proxy and the account.
    Settings(SettingsPayload),
    /// Stickers, custom emoji, emoji statuses, reactions, GIFs and the media library.
    Stickers(StickersPayload),
    /// Stories, story albums and close friends.
    Stories(StoriesPayload),
    /// Forum topics, comment threads and Saved Messages.
    Threads(ThreadsPayload),
    /// Users, contacts, profiles and secret chats.
    Users(UsersPayload),
    Ok,
    Error(TdError),
    Unknown(UnknownKind),
}

/// Debug prints the inner variant only (`UpdateNewMessage(..)`, not
/// `Messages(UpdateNewMessage(..))`), as before the domain split; the
/// frame-trace log keys on the leading name.
impl std::fmt::Debug for EnvelopePayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auth(inner) => inner.fmt(f),
            Self::Bots(inner) => inner.fmt(f),
            Self::Calls(inner) => inner.fmt(f),
            Self::ChatList(inner) => inner.fmt(f),
            Self::Chats(inner) => inner.fmt(f),
            Self::Common(inner) => inner.fmt(f),
            Self::Groups(inner) => inner.fmt(f),
            Self::Media(inner) => inner.fmt(f),
            Self::Messages(inner) => inner.fmt(f),
            Self::Payments(inner) => inner.fmt(f),
            Self::Search(inner) => inner.fmt(f),
            Self::Settings(inner) => inner.fmt(f),
            Self::Stickers(inner) => inner.fmt(f),
            Self::Stories(inner) => inner.fmt(f),
            Self::Threads(inner) => inner.fmt(f),
            Self::Users(inner) => inner.fmt(f),
            Self::Ok => f.write_str("Ok"),
            Self::Error(value) => f.debug_tuple("Error").field(value).finish(),
            Self::Unknown(value) => f.debug_tuple("Unknown").field(value).finish(),
        }
    }
}

macro_rules! payload_domains {
    ($($domain:ident($ty:ident)),* $(,)?) => {
        $(
            impl From<$ty> for EnvelopePayload {
                fn from(payload: $ty) -> Self {
                    Self::$domain(payload)
                }
            }
        )*
    };
}

payload_domains!(
    Auth(AuthPayload),
    Bots(BotsPayload),
    Calls(CallsPayload),
    ChatList(ChatListPayload),
    Chats(ChatsPayload),
    Common(CommonPayload),
    Groups(GroupsPayload),
    Media(MediaPayload),
    Messages(MessagesPayload),
    Payments(PaymentsPayload),
    Search(SearchPayload),
    Settings(SettingsPayload),
    Stickers(StickersPayload),
    Stories(StoriesPayload),
    Threads(ThreadsPayload),
    Users(UsersPayload),
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownKind {
    pub type_name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct RawEnvelope {
    #[serde(rename = "@type")]
    type_name: String,
    #[serde(rename = "@extra")]
    extra: Option<Value>,
    #[serde(rename = "@client_id")]
    client_id: Option<i32>,
}

pub fn parse_envelope(json: &str) -> Result<Envelope, ParseError> {
    let raw: RawEnvelope = serde_json::from_str(json).map_err(|_| ParseError::InvalidJson)?;
    let export_extra = raw
        .extra
        .as_ref()
        .and_then(|v| v.get("quill_account_export"));
    let extra = parse_extra(export_extra.or(raw.extra.as_ref()));
    let payload = if export_extra.is_some() {
        EnvelopePayload::Common(CommonPayload::AccountExport(
            serde_json::from_str(json).map_err(|_| ParseError::InvalidJson)?,
        ))
    } else {
        parse_payload(&raw.type_name, json)?
    };
    Ok(Envelope {
        type_name: raw.type_name,
        extra,
        client_id: raw.client_id,
        payload,
    })
}

pub(crate) fn parse_extra(value: Option<&Value>) -> Option<RequestId> {
    match value {
        Some(Value::String(s)) => RequestId::from_str(s).ok(),
        Some(Value::Number(n)) => n.as_u64().map(RequestId),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    InvalidJson,
    MissingField,
    BadInt,
}

/// The `messageProperties` flags the message context menu needs (TDLib
/// 1.8.67, `schema/td_api.tl:6262`): Telegram Desktop shows an action only
/// when the message allows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MessageActions {
    pub can_be_copied: bool,
    pub can_be_deleted_only_for_self: bool,
    pub can_be_deleted_for_all_users: bool,
    pub can_be_edited: bool,
    pub can_be_forwarded: bool,
    pub can_be_pinned: bool,
    pub can_be_replied: bool,
    pub can_get_link: bool,
    pub can_get_message_thread: bool,
    /// Content may be saved locally (Save As…, Copy Image, Show in Folder).
    pub can_be_saved: bool,
    /// The message can be reported with `reportChat`.
    pub can_report_chat: bool,
    /// `getMessageViewers` works ("N Seen").
    pub can_get_viewers: bool,
    /// `getMessageReadDate` works ("Seen 12:34" in private chats).
    pub can_get_read_date: bool,
    /// An admin may report it with `reportSupergroupSpam`.
    pub can_report_supergroup_spam: bool,
    /// An admin may delete other members' reactions on it.
    pub can_delete_reactions: bool,
    /// A scheduled message may be rescheduled or sent now.
    pub can_edit_scheduling_state: bool,
    /// B15: `getPollVoteStatistics` works (poll creator / admin view).
    pub can_get_poll_vote_statistics: bool,
    /// "Reply in Another Chat" is offered.
    pub can_be_replied_in_another_chat: bool,
    /// An admin may add or edit a fact check (`setMessageFactCheck`).
    pub can_set_fact_check: bool,
}

impl MessageActions {
    pub fn can_be_deleted(&self) -> bool {
        self.can_be_deleted_only_for_self || self.can_be_deleted_for_all_users
    }
}

/// Profile details from `userFullInfo` beyond the bio: the birthday and
/// how many groups you share with the user.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserProfileExtras {
    pub birthdate: Option<Birthdate>,
    pub groups_in_common: i32,
    /// B10: `personal_chat_id` (the user's personal channel); 0 = none.
    pub personal_chat_id: i64,
    /// B10: the private `note` added to the contact (plain text).
    pub note: String,
    /// B10: `need_phone_number_privacy_exception` — the edit-contact box
    /// then offers "Share my phone number" (tdesktop `NeedContactsException`).
    pub need_phone_exception: bool,
    /// B13: `gift_settings` — only the own user's is ever shown.
    pub gift_settings: Option<crate::privacy::GiftSettings>,
    /// `uses_unofficial_app` — the user runs an unofficial client that
    /// poses a security risk (tdesktop `unofficialSecurityRisk`).
    pub uses_unofficial_app: bool,
    /// `personal_photo` — the photo the current user set for this contact
    /// (shown first in the gallery as "Photo set by you").
    pub personal_photo: Option<ParsedProfilePhoto>,
    /// `business_info`: Telegram Business hours and location.
    pub business: Option<crate::business_info::BusinessInfo>,
    /// `main_profile_tab`: the tab the profile opens on.
    pub main_profile_tab: Option<crate::profile_tab::ProfileTab>,
    /// `can_be_called` is false or `has_private_calls` is true.
    pub calls_blocked: bool,
    /// `has_private_calls`: the block comes from the user's privacy settings.
    pub calls_private: bool,
    /// `supports_video_calls` is false.
    pub video_calls_unsupported: bool,
}

/// B10: one `chatPhoto` (schema 1.8.67, line 1030) from
/// `getUserProfilePhotos`. `files` holds every size; `thumb_file_id` is
/// the grid size and `full_file_id` the largest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProfilePhoto {
    /// `chatPhoto.id` — the `chat_photo_id` for "Set as main photo".
    pub id: i64,
    /// Unix time the photo was added (`added_date`); 0 if unknown.
    pub added_date: i32,
    pub files: Vec<ParsedFile>,
    pub thumb_file_id: crate::ids::FileId,
    pub full_file_id: crate::ids::FileId,
    /// Largest size's pixel dimensions.
    pub width: i32,
    pub height: i32,
}

/// `birthdate` (schema 1.8.67, line 868); the year is optional (0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Birthdate {
    pub day: u8,
    pub month: u8,
    pub year: Option<i32>,
}

pub(crate) fn parse_user_profile_extras(info: Option<&serde_json::Value>) -> UserProfileExtras {
    let Some(info) = info else {
        return UserProfileExtras::default();
    };
    let field = |value: &serde_json::Value, key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0)
    };
    let birthdate = info
        .get("birthdate")
        .filter(|value| !value.is_null())
        .and_then(|value| {
            let (day, month, year) = (
                field(value, "day"),
                field(value, "month"),
                field(value, "year"),
            );
            ((1..=31).contains(&day) && (1..=12).contains(&month)).then(|| Birthdate {
                day: day as u8,
                month: month as u8,
                year: (year > 0).then_some(year.sat_i32()),
            })
        });
    UserProfileExtras {
        birthdate,
        groups_in_common: field(info, "group_in_common_count").max(0).sat_i32(),
        personal_chat_id: info
            .get("personal_chat_id")
            .and_then(|v| super::json_helpers::int53(Some(v)).ok())
            .unwrap_or(0),
        note: super::message_content::parse_formatted_text(info.get("note")),
        need_phone_exception: info
            .get("need_phone_number_privacy_exception")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        gift_settings: crate::privacy::GiftSettings::from_value(info.get("gift_settings")),
        uses_unofficial_app: info
            .get("uses_unofficial_app")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        personal_photo: info
            .get("personal_photo")
            .filter(|value| !value.is_null())
            .and_then(super::users::parse_profile_photo),
        business: crate::business_info::parse_business_info(info.get("business_info")),
        main_profile_tab: crate::profile_tab::ProfileTab::from_value(info.get("main_profile_tab")),
        calls_blocked: flag(info, "can_be_called") == Some(false)
            || flag(info, "has_private_calls") == Some(true),
        calls_private: flag(info, "has_private_calls") == Some(true),
        video_calls_unsupported: flag(info, "supports_video_calls") == Some(false),
    }
}

fn flag(info: &serde_json::Value, key: &str) -> Option<bool> {
    info.get(key).and_then(serde_json::Value::as_bool)
}

/// One `messageCalendarDay` (schema line 3191): the first message sent on
/// the day and how many matched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarDay {
    pub total_count: i32,
    pub message_id: MessageId,
    pub date: i32,
}

/// B7: the `supergroupFullInfo` flags behind the group admin toggles
/// (schema 1.8.67, line 2792).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SupergroupFullAdmin {
    /// `can_hide_members` — `toggleSupergroupHasHiddenMembers` may be used.
    pub can_hide_members: bool,
    /// `has_hidden_members` — non-admins can't list the members.
    pub has_hidden_members: bool,
    /// `is_all_history_available` — new members see older messages.
    pub is_all_history_available: bool,
    /// `can_enable_paid_reaction` — channels only.
    pub can_enable_paid_reaction: bool,
}

/// A chat's name color and reply emoji (`chat.accent_color_id`,
/// `chat.background_custom_emoji_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChatAccent {
    pub accent_color_id: i32,
    pub background_custom_emoji_id: i64,
}

impl ChatAccent {
    pub(crate) fn parse(value: &serde_json::Value) -> Self {
        Self {
            accent_color_id: value
                .get("accent_color_id")
                .and_then(serde_json::Value::as_i64)
                .and_then(|id| i32::try_from(id).ok())
                .unwrap_or(0),
            background_custom_emoji_id: int64(value.get("background_custom_emoji_id")).unwrap_or(0),
        }
    }
}
