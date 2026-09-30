use super::*;
use serde_json::Value;

/// tdesktop default mute submenu (`SessionSettings::mutePeriods` when unset /
/// `DefaultTimePickerValues`): 1 hour, 8 hours, 2 days. Seconds, matching
/// `chatNotificationSettings.mute_for`.
pub const MUTE_FOR_1_HOUR: i32 = 3600;
pub const MUTE_FOR_8_HOURS: i32 = 8 * 3600;

pub const MUTE_FOR_2_DAYS: i32 = 2 * 86400;
/// tdesktop `MuteMenu::kMuteForeverValue` (`numeric_limits<int>::max()`).
/// TDLib: mute_for longer than 366 days is muted forever.
pub const MUTE_FOREVER: i32 = i32::MAX;
pub const MUTE_FOREVER_AFTER_SECONDS: i32 = 366 * 86400;

/// `chatNotificationSettings` (TDLib 1.8.67). Other fields are copied through
/// so a mute change does not reset sound / preview exceptions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatNotificationSettings {
    pub use_default_mute_for: bool,
    pub mute_for: i32,
    pub use_default_sound: bool,
    pub sound_id: i64,
    pub use_default_show_preview: bool,
    pub show_preview: bool,
    pub use_default_mute_stories: bool,
    pub mute_stories: bool,
    pub use_default_story_sound: bool,
    pub story_sound_id: i64,
    pub use_default_show_story_poster: bool,
    pub show_story_poster: bool,
    pub use_default_disable_pinned_message_notifications: bool,
    pub disable_pinned_message_notifications: bool,
    pub use_default_disable_mention_notifications: bool,
    pub disable_mention_notifications: bool,
}

impl Default for ChatNotificationSettings {
    fn default() -> Self {
        Self {
            use_default_mute_for: true,
            mute_for: 0,
            use_default_sound: true,
            sound_id: 0,
            use_default_show_preview: true,
            show_preview: false,
            use_default_mute_stories: true,
            mute_stories: false,
            use_default_story_sound: true,
            story_sound_id: 0,
            use_default_show_story_poster: true,
            show_story_poster: false,
            use_default_disable_pinned_message_notifications: true,
            disable_pinned_message_notifications: false,
            use_default_disable_mention_notifications: true,
            disable_mention_notifications: false,
        }
    }
}

impl ChatNotificationSettings {
    /// Exception mute. `use_default_mute_for` stays true until the user sets one
    /// (Unigram clones settings and clears the default flag).
    pub fn with_mute_for(mut self, mute_for: i32) -> Self {
        self.use_default_mute_for = false;
        self.mute_for = mute_for;
        self
    }

    /// Effective chat mute. Scope defaults are not applied here.
    pub fn is_muted(&self) -> bool {
        !self.use_default_mute_for && self.mute_for > 0
    }

    pub fn is_muted_forever(&self) -> bool {
        self.is_muted() && self.mute_for > MUTE_FOREVER_AFTER_SECONDS
    }
}

/// `notificationSound` (TDLib 1.8.67, line 8857): "Describes a notification
/// sound in MP3 format".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationSound {
    pub id: i64,
    pub duration: i32,
    pub date: i32,
    pub title: String,
    pub data: String,
    /// `sound:file` — downloaded on demand with `downloadFile` when a
    /// notification needs it.
    pub sound: ParsedFile,
}

/// `NotificationSettingsScope` (TDLib 1.8.67, lines 3337–3343).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationSettingsScope {
    PrivateChats,
    GroupChats,
    ChannelChats,
}

impl NotificationSettingsScope {
    /// The `@type` constructor name for `getScopeNotificationSettings` /
    /// `setScopeNotificationSettings` requests.
    pub fn type_name(&self) -> &'static str {
        match self {
            NotificationSettingsScope::PrivateChats => "notificationSettingsScopePrivateChats",
            NotificationSettingsScope::GroupChats => "notificationSettingsScopeGroupChats",
            NotificationSettingsScope::ChannelChats => "notificationSettingsScopeChannelChats",
        }
    }

    /// Human label for the scope-defaults settings UI.
    pub fn label(&self) -> &'static str {
        match self {
            NotificationSettingsScope::PrivateChats => "Private chats",
            NotificationSettingsScope::GroupChats => "Groups",
            NotificationSettingsScope::ChannelChats => "Channels",
        }
    }

    /// All three scopes, in UI order.
    pub const ALL: [NotificationSettingsScope; 3] = [
        NotificationSettingsScope::PrivateChats,
        NotificationSettingsScope::GroupChats,
        NotificationSettingsScope::ChannelChats,
    ];
}

/// `scopeNotificationSettings` (TDLib 1.8.67, line 3375): defaults applied
/// when a chat's `chatNotificationSettings` keeps a `use_default_*` flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeNotificationSettings {
    pub mute_for: i32,
    /// 0 = disabled; -1 = app-dependent default sound (schema line 3368).
    pub sound_id: i64,
    pub show_preview: bool,
    pub use_default_mute_stories: bool,
    pub mute_stories: bool,
    pub story_sound_id: i64,
    pub show_story_poster: bool,
    pub disable_pinned_message_notifications: bool,
    pub disable_mention_notifications: bool,
}

impl Default for ScopeNotificationSettings {
    fn default() -> Self {
        Self {
            mute_for: 0,
            sound_id: -1,
            show_preview: true,
            use_default_mute_stories: true,
            mute_stories: false,
            story_sound_id: -1,
            show_story_poster: true,
            disable_pinned_message_notifications: false,
            disable_mention_notifications: false,
        }
    }
}

/// `reactionNotificationSource` (TDLib 1.8.67, lines 3378-3387): which
/// reactions get notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReactionNotificationSource {
    /// Notifications for reactions are disabled.
    #[default]
    None,
    /// Notifications are shown only for reactions from contacts.
    Contacts,
    /// Notifications are shown for all reactions.
    All,
}

impl ReactionNotificationSource {
    /// The `@type` constructor name for `setReactionNotificationSettings`.
    pub fn type_name(self) -> &'static str {
        match self {
            ReactionNotificationSource::None => "reactionNotificationSourceNone",
            ReactionNotificationSource::Contacts => "reactionNotificationSourceContacts",
            ReactionNotificationSource::All => "reactionNotificationSourceAll",
        }
    }
}

/// `reactionNotificationSettings` (TDLib 1.8.67, line 3396): notification
/// settings for reactions and poll votes. There is no getter — the current
/// values arrive via `updateReactionNotificationSettings`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactionNotificationSettings {
    pub message_reaction_source: ReactionNotificationSource,
    pub story_reaction_source: ReactionNotificationSource,
    pub poll_vote_source: ReactionNotificationSource,
    /// 0 = disabled; -1 = app-dependent default sound (schema line 3394).
    pub sound_id: i64,
    /// True if reaction sender and emoji must be displayed in notifications.
    pub show_preview: bool,
}

impl Default for ReactionNotificationSettings {
    fn default() -> Self {
        Self {
            message_reaction_source: ReactionNotificationSource::None,
            story_reaction_source: ReactionNotificationSource::None,
            poll_vote_source: ReactionNotificationSource::None,
            sound_id: -1,
            show_preview: true,
        }
    }
}

pub(crate) fn parse_chat_notification_settings(value: Option<&Value>) -> ChatNotificationSettings {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return ChatNotificationSettings::default();
    };
    let defaults = ChatNotificationSettings::default();
    ChatNotificationSettings {
        use_default_mute_for: json_bool(
            value.get("use_default_mute_for"),
            defaults.use_default_mute_for,
        ),
        mute_for: json_i32(value.get("mute_for"), defaults.mute_for),
        use_default_sound: json_bool(value.get("use_default_sound"), defaults.use_default_sound),
        sound_id: json_i64_field(value.get("sound_id"), defaults.sound_id),
        use_default_show_preview: json_bool(
            value.get("use_default_show_preview"),
            defaults.use_default_show_preview,
        ),
        show_preview: json_bool(value.get("show_preview"), defaults.show_preview),
        use_default_mute_stories: json_bool(
            value.get("use_default_mute_stories"),
            defaults.use_default_mute_stories,
        ),
        mute_stories: json_bool(value.get("mute_stories"), defaults.mute_stories),
        use_default_story_sound: json_bool(
            value.get("use_default_story_sound"),
            defaults.use_default_story_sound,
        ),
        story_sound_id: json_i64_field(value.get("story_sound_id"), defaults.story_sound_id),
        use_default_show_story_poster: json_bool(
            value.get("use_default_show_story_poster"),
            defaults.use_default_show_story_poster,
        ),
        show_story_poster: json_bool(value.get("show_story_poster"), defaults.show_story_poster),
        use_default_disable_pinned_message_notifications: json_bool(
            value.get("use_default_disable_pinned_message_notifications"),
            defaults.use_default_disable_pinned_message_notifications,
        ),
        disable_pinned_message_notifications: json_bool(
            value.get("disable_pinned_message_notifications"),
            defaults.disable_pinned_message_notifications,
        ),
        use_default_disable_mention_notifications: json_bool(
            value.get("use_default_disable_mention_notifications"),
            defaults.use_default_disable_mention_notifications,
        ),
        disable_mention_notifications: json_bool(
            value.get("disable_mention_notifications"),
            defaults.disable_mention_notifications,
        ),
    }
}

/// `notificationSound` (TDLib 1.8.67, line 8857). Returns `None` when the
/// object is missing or its `sound` file does not parse.
pub(crate) fn parse_notification_sound(value: &Value) -> Option<NotificationSound> {
    if value.get("@type").and_then(Value::as_str) != Some("notificationSound") {
        return None;
    }
    let sound = parse_file(value.get("sound")).ok()?;
    Some(NotificationSound {
        id: json_i64_field(value.get("id"), 0),
        duration: json_i32(value.get("duration"), 0),
        date: json_i32(value.get("date"), 0),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        data: value
            .get("data")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        sound,
    })
}

/// `NotificationSettingsScope` from its constructor name (td_api.tl lines
/// 3337–3343). Unknown names map to `None` so a future scope never
/// mis-files settings.
pub(crate) fn parse_notification_settings_scope(
    type_name: Option<&str>,
) -> Option<NotificationSettingsScope> {
    match type_name {
        Some("notificationSettingsScopePrivateChats") => {
            Some(NotificationSettingsScope::PrivateChats)
        }
        Some("notificationSettingsScopeGroupChats") => Some(NotificationSettingsScope::GroupChats),
        Some("notificationSettingsScopeChannelChats") => {
            Some(NotificationSettingsScope::ChannelChats)
        }
        _ => None,
    }
}

/// `scopeNotificationSettings` (TDLib 1.8.67, line 3375).
pub(crate) fn parse_scope_notification_settings(
    value: Option<&Value>,
) -> ScopeNotificationSettings {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return ScopeNotificationSettings::default();
    };
    let defaults = ScopeNotificationSettings::default();
    ScopeNotificationSettings {
        mute_for: json_i32(value.get("mute_for"), defaults.mute_for),
        sound_id: json_i64_field(value.get("sound_id"), defaults.sound_id),
        show_preview: json_bool(value.get("show_preview"), defaults.show_preview),
        use_default_mute_stories: json_bool(
            value.get("use_default_mute_stories"),
            defaults.use_default_mute_stories,
        ),
        mute_stories: json_bool(value.get("mute_stories"), defaults.mute_stories),
        story_sound_id: json_i64_field(value.get("story_sound_id"), defaults.story_sound_id),
        show_story_poster: json_bool(value.get("show_story_poster"), defaults.show_story_poster),
        disable_pinned_message_notifications: json_bool(
            value.get("disable_pinned_message_notifications"),
            defaults.disable_pinned_message_notifications,
        ),
        disable_mention_notifications: json_bool(
            value.get("disable_mention_notifications"),
            defaults.disable_mention_notifications,
        ),
    }
}

/// `reactionNotificationSettings` (TDLib 1.8.67, line 3396).
pub(crate) fn parse_reaction_notification_settings(
    value: Option<&Value>,
) -> ReactionNotificationSettings {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return ReactionNotificationSettings::default();
    };
    let defaults = ReactionNotificationSettings::default();
    ReactionNotificationSettings {
        message_reaction_source: parse_reaction_notification_source(
            value.get("message_reaction_source"),
        ),
        story_reaction_source: parse_reaction_notification_source(
            value.get("story_reaction_source"),
        ),
        poll_vote_source: parse_reaction_notification_source(value.get("poll_vote_source")),
        sound_id: json_i64_field(value.get("sound_id"), defaults.sound_id),
        show_preview: json_bool(value.get("show_preview"), defaults.show_preview),
    }
}

/// `reactionNotificationSource` (TDLib 1.8.67, lines 3378-3387); unknown
/// constructors fall back to `None` (disabled).
pub(crate) fn parse_reaction_notification_source(
    value: Option<&Value>,
) -> ReactionNotificationSource {
    match value.and_then(|v| v.get("@type")).and_then(Value::as_str) {
        Some("reactionNotificationSourceContacts") => ReactionNotificationSource::Contacts,
        Some("reactionNotificationSourceAll") => ReactionNotificationSource::All,
        _ => ReactionNotificationSource::None,
    }
}
