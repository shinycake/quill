//! Telegram Business settings of the own account: greeting and away
//! messages, their recipients, and the request bodies for those and for
//! opening hours and location (TDLib 1.8.68 `setBusiness*`).
//!
//! What Telegram Desktop does (`settings/business/`):
//!
//! - `settings_greeting.cpp`: "Send Greeting Message" switch; the messages
//!   are the quick reply shortcut `hello`; recipients; "Period of no
//!   activity" of 7, 14, 21 or 28 days (7 by default).
//! - `settings_away_message.cpp`: "Send Away Message" switch; the messages
//!   are the shortcut `away`; schedule Always / Outside of Business Hours /
//!   Custom (start and end time, a day by default); "Only if Offline";
//!   recipients.
//! - Both refuse to turn on when the account is at its quick reply limit
//!   and has no such shortcut yet ("You have too many quick replies...").
//! - `settings_recipients_helper.cpp`: "All 1-to-1 Chats Except..." or
//!   "Only Selected Chats", with the excluded or included chats and chat
//!   types (New, Existing, Contacts, Non-Contacts). Recipients with no
//!   included chats are always "all except" (`MakeValid`).
//! - `data_business_info.cpp`: nothing is sent when the value did not
//!   change, and a greeting or away message is only sent with a shortcut.
//!
//! Opening hours and location data live in `crate::business_info`; the
//! day editor's interval math is in `crate::business_hours_edit`.

use crate::business_info::{BusinessLocation, OpeningHours};
use serde_json::{Value, json};

/// The quick reply shortcut that holds the greeting messages.
pub const GREETING_SHORTCUT: &str = "hello";
/// The quick reply shortcut that holds the away messages.
pub const AWAY_SHORTCUT: &str = "away";

/// `businessGreetingMessageSettings.inactivity_days`: "must be one of 7,
/// 14, 21, or 28".
pub const INACTIVITY_DAYS: [i32; 4] = [7, 14, 21, 28];
/// tdesktop `kDefaultNoActivityDays`.
pub const DEFAULT_INACTIVITY_DAYS: i32 = 7;

/// tdesktop's include/exclude chat limit of the recipients picker.
pub const RECIPIENT_CHATS_MAX: usize = 100;

/// `businessLocation.address`: 1-96 characters.
pub const ADDRESS_MAX: usize = 96;

/// tdesktop `StartTimeMin`: August 2013, when Telegram launched.
pub const CUSTOM_START_MIN: i64 = 1_375_315_200;

/// Chat categories of a recipients list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChatTypes {
    pub new_chats: bool,
    pub existing_chats: bool,
    pub contacts: bool,
    pub non_contacts: bool,
}

/// One chat category, in tdesktop's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatType {
    NewChats,
    ExistingChats,
    Contacts,
    NonContacts,
}

impl ChatType {
    pub const ALL: [ChatType; 4] = [
        Self::NewChats,
        Self::ExistingChats,
        Self::Contacts,
        Self::NonContacts,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::NewChats => "New Chats",
            Self::ExistingChats => "Existing Chats",
            Self::Contacts => "Contacts",
            Self::NonContacts => "Non-Contacts",
        }
    }
}

impl ChatTypes {
    pub fn is_empty(&self) -> bool {
        !(self.new_chats || self.existing_chats || self.contacts || self.non_contacts)
    }

    pub fn has(&self, kind: ChatType) -> bool {
        match kind {
            ChatType::NewChats => self.new_chats,
            ChatType::ExistingChats => self.existing_chats,
            ChatType::Contacts => self.contacts,
            ChatType::NonContacts => self.non_contacts,
        }
    }

    pub fn set(&mut self, kind: ChatType, on: bool) {
        match kind {
            ChatType::NewChats => self.new_chats = on,
            ChatType::ExistingChats => self.existing_chats = on,
            ChatType::Contacts => self.contacts = on,
            ChatType::NonContacts => self.non_contacts = on,
        }
    }

    /// The chosen categories, in tdesktop's order.
    pub fn chosen(&self) -> Vec<ChatType> {
        ChatType::ALL.into_iter().filter(|t| self.has(*t)).collect()
    }
}

/// Chats and categories on one side of a recipients list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BusinessChats {
    pub types: ChatTypes,
    /// Private chat ids, in the order they were chosen.
    pub chat_ids: Vec<i64>,
}

impl BusinessChats {
    pub fn is_empty(&self) -> bool {
        self.types.is_empty() && self.chat_ids.is_empty()
    }

    /// Flip one chat; adding stops at [`RECIPIENT_CHATS_MAX`]. `false` when
    /// the chat could not be added.
    pub fn toggle_chat(&mut self, chat_id: i64) -> bool {
        if let Some(at) = self.chat_ids.iter().position(|id| *id == chat_id) {
            self.chat_ids.remove(at);
            return true;
        }
        if self.chat_ids.len() >= RECIPIENT_CHATS_MAX {
            return false;
        }
        self.chat_ids.push(chat_id);
        true
    }
}

/// `businessRecipients`, in tdesktop's shape: the included and excluded
/// lists and which of them applies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BusinessRecipients {
    pub included: BusinessChats,
    pub excluded: BusinessChats,
    /// "All 1-to-1 Chats Except..." (`exclude_selected`).
    pub all_but_excluded: bool,
}

impl BusinessRecipients {
    /// What a newly enabled greeting or away message starts with.
    pub fn everyone() -> Self {
        Self {
            all_but_excluded: true,
            ..Self::default()
        }
    }

    /// tdesktop `MakeValid`: nothing included means "all except".
    pub fn make_valid(mut self) -> Self {
        if self.included.is_empty() {
            self.all_but_excluded = true;
        }
        self
    }

    /// The list that is sent: excluded chats for "all except", included
    /// ones otherwise.
    pub fn active(&self) -> &BusinessChats {
        if self.all_but_excluded {
            &self.excluded
        } else {
            &self.included
        }
    }

    /// tdesktop's exclude-picker save: a non-empty exclusion drops the
    /// included list.
    pub fn set_excluded(&mut self, value: BusinessChats) {
        if !value.is_empty() {
            self.included = BusinessChats::default();
        }
        self.excluded = value;
    }

    /// tdesktop's include-picker save: a non-empty inclusion drops the
    /// excluded categories and selects "Only Selected Chats"; an empty one
    /// falls back to "all except".
    pub fn set_included(&mut self, value: BusinessChats) {
        if !value.is_empty() {
            self.excluded.types = ChatTypes::default();
        }
        self.all_but_excluded = value.is_empty();
        self.included = value;
    }

    /// `businessRecipients` from TDLib.
    pub fn from_value(value: &Value) -> Self {
        let flag = |key: &str| value.get(key).and_then(Value::as_bool).unwrap_or(false);
        let ids = |key: &str| -> Vec<i64> {
            value
                .get(key)
                .and_then(Value::as_array)
                .map(|list| list.iter().filter_map(Value::as_i64).collect())
                .unwrap_or_default()
        };
        let chats = BusinessChats {
            types: ChatTypes {
                new_chats: flag("select_new_chats"),
                existing_chats: flag("select_existing_chats"),
                contacts: flag("select_contacts"),
                non_contacts: flag("select_non_contacts"),
            },
            chat_ids: ids("chat_ids"),
        };
        let all_but_excluded = flag("exclude_selected");
        if all_but_excluded {
            Self {
                excluded: chats,
                all_but_excluded,
                ..Self::default()
            }
        } else {
            Self {
                included: chats,
                all_but_excluded,
                ..Self::default()
            }
        }
    }

    /// `businessRecipients` for a greeting or away message (no
    /// `excluded_chat_ids`; that field is for connected bots only).
    pub fn to_json(&self) -> Value {
        let chats = self.active();
        json!({
            "@type": "businessRecipients",
            "chat_ids": chats.chat_ids,
            "excluded_chat_ids": [],
            "select_existing_chats": chats.types.existing_chats,
            "select_new_chats": chats.types.new_chats,
            "select_contacts": chats.types.contacts,
            "select_non_contacts": chats.types.non_contacts,
            "exclude_selected": self.all_but_excluded,
        })
    }
}

/// `businessGreetingMessageSettings`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GreetingSettings {
    pub shortcut_id: i32,
    pub recipients: BusinessRecipients,
    pub inactivity_days: i32,
}

/// `BusinessAwayMessageSchedule`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwaySchedule {
    Always,
    OutsideOpeningHours,
    /// Unix times.
    Custom {
        start: i64,
        end: i64,
    },
}

impl AwaySchedule {
    fn to_json(self) -> Value {
        match self {
            Self::Always => json!({"@type": "businessAwayMessageScheduleAlways"}),
            Self::OutsideOpeningHours => {
                json!({"@type": "businessAwayMessageScheduleOutsideOfOpeningHours"})
            }
            Self::Custom { start, end } => json!({
                "@type": "businessAwayMessageScheduleCustom",
                "start_date": start,
                "end_date": end,
            }),
        }
    }

    fn from_value(value: &Value) -> Option<Self> {
        match value.get("@type")?.as_str()? {
            "businessAwayMessageScheduleAlways" => Some(Self::Always),
            "businessAwayMessageScheduleOutsideOfOpeningHours" => Some(Self::OutsideOpeningHours),
            "businessAwayMessageScheduleCustom" => Some(Self::Custom {
                start: value.get("start_date")?.as_i64()?,
                end: value.get("end_date")?.as_i64()?,
            }),
            _ => None,
        }
    }
}

/// tdesktop `BadCustomInterval`: an empty interval, or one before 2013.
pub fn bad_custom_interval(start: i64, end: i64) -> bool {
    start >= end || start < CUSTOM_START_MIN || end < CUSTOM_START_MIN + 3600
}

/// The custom interval an editor starts with: the saved one, or the next
/// 24 hours from `now` (tdesktop).
pub fn initial_custom_interval(schedule: Option<AwaySchedule>, now: i64) -> (i64, i64) {
    match schedule {
        Some(AwaySchedule::Custom { start, end }) if !bad_custom_interval(start, end) => {
            (start, end)
        }
        _ => (now, now + 24 * 3600),
    }
}

/// `businessAwayMessageSettings`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AwaySettings {
    pub shortcut_id: i32,
    pub recipients: BusinessRecipients,
    pub schedule: AwaySchedule,
    pub offline_only: bool,
}

fn not_null(value: Option<&Value>) -> Option<&Value> {
    value.filter(|v| !v.is_null())
}

fn int32(value: &Value, key: &str) -> Option<i32> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
}

/// `businessInfo.greeting_message_settings`; `None` when off.
pub fn parse_greeting(value: Option<&Value>) -> Option<GreetingSettings> {
    let value = not_null(value)?;
    Some(GreetingSettings {
        shortcut_id: int32(value, "shortcut_id")?,
        recipients: not_null(value.get("recipients"))
            .map(BusinessRecipients::from_value)
            .unwrap_or_default(),
        inactivity_days: int32(value, "inactivity_days").unwrap_or(DEFAULT_INACTIVITY_DAYS),
    })
}

/// `businessInfo.away_message_settings`; `None` when off.
pub fn parse_away(value: Option<&Value>) -> Option<AwaySettings> {
    let value = not_null(value)?;
    Some(AwaySettings {
        shortcut_id: int32(value, "shortcut_id")?,
        recipients: not_null(value.get("recipients"))
            .map(BusinessRecipients::from_value)
            .unwrap_or_default(),
        schedule: AwaySchedule::from_value(not_null(value.get("schedule"))?)?,
        offline_only: value
            .get("offline_only")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// `setBusinessGreetingMessageSettings`; `None` turns the greeting off.
pub fn set_greeting_request(extra: &str, settings: Option<&GreetingSettings>) -> String {
    let value = settings.map_or(Value::Null, |s| {
        json!({
            "@type": "businessGreetingMessageSettings",
            "shortcut_id": s.shortcut_id,
            "recipients": s.recipients.to_json(),
            "inactivity_days": s.inactivity_days,
        })
    });
    json!({
        "@type": "setBusinessGreetingMessageSettings",
        "@extra": extra,
        "greeting_message_settings": value,
    })
    .to_string()
}

/// `setBusinessAwayMessageSettings`; `None` turns the away message off.
pub fn set_away_request(extra: &str, settings: Option<&AwaySettings>) -> String {
    let value = settings.map_or(Value::Null, |s| {
        json!({
            "@type": "businessAwayMessageSettings",
            "shortcut_id": s.shortcut_id,
            "recipients": s.recipients.to_json(),
            "schedule": s.schedule.to_json(),
            "offline_only": s.offline_only,
        })
    });
    json!({
        "@type": "setBusinessAwayMessageSettings",
        "@extra": extra,
        "away_message_settings": value,
    })
    .to_string()
}

/// `setBusinessOpeningHours`; `None` removes the hours.
pub fn set_opening_hours_request(extra: &str, hours: Option<&OpeningHours>) -> String {
    let value = hours.map_or(Value::Null, |h| {
        let intervals: Vec<Value> = h
            .intervals
            .iter()
            .map(|&(start, end)| {
                json!({
                    "@type": "businessOpeningHoursInterval",
                    "start_minute": start,
                    "end_minute": end,
                })
            })
            .collect();
        json!({
            "@type": "businessOpeningHours",
            "time_zone_id": h.time_zone_id,
            "opening_hours": intervals,
        })
    });
    json!({
        "@type": "setBusinessOpeningHours",
        "@extra": extra,
        "opening_hours": value,
    })
    .to_string()
}

/// `setBusinessLocation`; `None` removes the location.
pub fn set_location_request(extra: &str, location: Option<&BusinessLocation>) -> String {
    let value = location.map_or(Value::Null, |l| {
        let point = l.point_e6.map_or(Value::Null, |(lat, lon)| {
            json!({
                "@type": "location",
                "latitude": f64::from(lat) / 1e6,
                "longitude": f64::from(lon) / 1e6,
                "horizontal_accuracy": 0.0,
            })
        });
        json!({"@type": "businessLocation", "location": point, "address": l.address})
    });
    json!({
        "@type": "setBusinessLocation",
        "@extra": extra,
        "location": value,
    })
    .to_string()
}

/// The location to save from the editor: the trimmed address and the
/// kept map point; `None` (remove) when both are gone.
pub fn location_to_save(address: &str, point_e6: Option<(i32, i32)>) -> Option<BusinessLocation> {
    let address = address.trim().to_string();
    (!address.is_empty() || point_e6.is_some()).then_some(BusinessLocation { address, point_e6 })
}

/// Which business setting a `setBusiness*` request saves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusinessSettingKind {
    Greeting,
    Away,
    OpeningHours,
    Location,
    StartPage,
}

impl BusinessSettingKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Greeting => "greeting message",
            Self::Away => "away message",
            Self::OpeningHours => "business hours",
            Self::Location => "location",
            Self::StartPage => "start page",
        }
    }
}

/// The line for a refused save. `reason` is the error class in words
/// (Quill never keeps TDLib's message text).
pub fn save_error_line(kind: BusinessSettingKind, reason: &str) -> String {
    format!("Couldn't save the {}: {reason}", kind.label())
}

/// tdesktop's `_canHave`: a greeting or away message can be turned on
/// while the account is below its quick reply limit, or when its shortcut
/// already has messages.
pub fn can_enable(shortcuts_with_messages: usize, limit: usize, own_has_messages: bool) -> bool {
    shortcuts_with_messages < limit || own_has_messages
}

/// "You have too many quick replies..." for the greeting / away switch.
pub fn limit_reached_line(kind: BusinessSettingKind) -> &'static str {
    match kind {
        BusinessSettingKind::Away => {
            "You have too many quick replies. Remove one to add an away message."
        }
        _ => "You have too many quick replies. Remove one to add a greeting message.",
    }
}

/// "7 days" (tdesktop `lng_days`).
pub fn days_label(days: i32) -> String {
    if days == 1 {
        "1 day".to_string()
    } else {
        format!("{days} days")
    }
}

/// `timeZone` (`getTimeZones`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeZone {
    pub id: String,
    pub name: String,
    /// Seconds.
    pub utc_offset: i32,
}

impl TimeZone {
    /// tdesktop `TimezoneFullName`: "(UTC+2:00) Jerusalem".
    pub fn full_name(&self) -> String {
        let abs = self.utc_offset.unsigned_abs();
        let sign = if self.utc_offset < 0 { '-' } else { '+' };
        format!(
            "(UTC{sign}{}:{:02}) {}",
            abs / 3600,
            (abs % 3600) / 60,
            self.name
        )
    }
}

/// `toggleHasSponsoredMessagesEnabled` (the hub's "Do Not Hide Ads").
pub fn toggle_sponsored_request(extra: &str, enabled: bool) -> String {
    json!({
        "@type": "toggleHasSponsoredMessagesEnabled",
        "@extra": extra,
        "has_sponsored_messages_enabled": enabled,
    })
    .to_string()
}

pub fn get_time_zones_request(extra: &str) -> String {
    json!({"@type": "getTimeZones", "@extra": extra}).to_string()
}

/// `timeZones` -> the list sorted like tdesktop's picker (offset, then
/// name).
pub fn parse_time_zones(value: &Value) -> Vec<TimeZone> {
    let mut list: Vec<TimeZone> = value
        .get("time_zones")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|zone| {
                    Some(TimeZone {
                        id: zone.get("id")?.as_str()?.to_string(),
                        name: zone.get("name")?.as_str()?.to_string(),
                        utc_offset: int32(zone, "utc_time_offset").unwrap_or(0),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    list.sort_by(|a, b| (a.utc_offset, &a.name).cmp(&(b.utc_offset, &b.name)));
    list
}

/// tdesktop `FindClosestTimezoneId`: the zone whose offset is nearest to
/// the local one (`local_offset` seconds east of UTC).
pub fn closest_time_zone(list: &[TimeZone], local_offset: i32) -> Option<&TimeZone> {
    list.iter().min_by_key(|zone| {
        let mut distance = zone.utc_offset - local_offset;
        while distance > 12 * 3600 {
            distance -= 24 * 3600;
        }
        while distance < -12 * 3600 {
            distance += 24 * 3600;
        }
        distance.abs()
    })
}

#[cfg(test)]
#[path = "business_settings_tests.rs"]
mod tests;
