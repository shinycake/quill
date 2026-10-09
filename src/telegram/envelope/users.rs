use super::*;
use serde_json::Value;

/// Phase 6: `userStatus*` (TDLib 1.8.67, `schema/td_api.tl:6407`).
/// Unknown constructors fall back to `Empty` — a hostile status can never
/// crash the parse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UserStatusKind {
    #[default]
    Empty,
    Online,
    Offline {
        was_online: i32,
    },
    Recently,
    LastWeek,
    LastMonth,
}

impl UserStatusKind {
    /// Cheap status line for the contacts list / user info panel.
    pub fn display(&self) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.display_at(now)
    }

    pub(crate) fn display_at(&self, now_secs: u64) -> String {
        match *self {
            UserStatusKind::Empty => String::new(),
            UserStatusKind::Online => "online".to_string(),
            UserStatusKind::Recently => "last seen recently".to_string(),
            UserStatusKind::LastWeek => "last seen within a week".to_string(),
            UserStatusKind::LastMonth => "last seen within a month".to_string(),
            UserStatusKind::Offline { was_online } => {
                let was = was_online.max(0) as u64;
                if was == 0 || was > now_secs {
                    return "last seen a long time ago".to_string();
                }
                let ago = now_secs - was;
                if ago < 60 {
                    "last seen just now".to_string()
                } else if ago < 3600 {
                    format!("last seen {}m ago", ago / 60)
                } else if ago < 86400 {
                    format!("last seen {}h ago", ago / 3600)
                } else if ago < 7 * 86400 {
                    format!("last seen {}d ago", ago / 86400)
                } else {
                    "last seen a long time ago".to_string()
                }
            }
        }
    }

    pub fn is_online(&self) -> bool {
        matches!(self, UserStatusKind::Online)
    }
}

/// Phase 6: `user` subset (TDLib 1.8.67, `schema/td_api.tl:2403`) kept for
/// the contacts list and the user info panel. Dropped (documented, not
/// forgotten): accent/background color ids, support flag, restriction info, active story state,
/// new-chat restrictions, paid-message star count, access flags, chat
/// language, attachment-menu flag.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedUser {
    pub id: i64,
    pub first_name: String,
    pub last_name: String,
    /// First entry of `usernames.active_usernames` (schema 1.8.67, line
    /// 2372 — this schema version has no singular `username` field).
    pub username: String,
    /// A5: full `usernames.active_usernames` list (schema 1.8.67, line
    /// 2372); the first entry is the primary username and the order is
    /// user-reorderable via `reorderActiveUsernames`.
    pub active_usernames: Vec<String>,
    /// A5: `usernames.disabled_usernames` — re-activatable via
    /// `toggleUsernameIsActive`.
    pub disabled_usernames: Vec<String>,
    /// A5: `usernames.editable_username` — the username `setUsername`
    /// changes (schema 1.8.67, line 2372).
    pub editable_username: String,
    pub phone_number: String,
    pub is_contact: bool,
    pub is_bot: bool,
    /// Bots slice: `userTypeBot.is_inline` (schema 1.8.67, line 2424) —
    /// whether the bot supports inline mode (`getInlineQueryResults`).
    /// False for non-bots and for bots without inline mode enabled.
    pub is_inline: bool,
    /// Subsection tabs: `userTypeBot.has_topics` (schema 1.8.67, line
    /// 816) — the private chat with this bot is split into forum topics
    /// (`getForumTopics` works on it). False for non-bots.
    pub has_topics: bool,
    /// Subsection tabs: `userTypeBot.allows_users_to_create_topics`
    /// (schema 1.8.67, line 816). When false the bot creates the topics
    /// itself, and Telegram Desktop only shows the tabs once at least one
    /// topic exists (`Data::IsBotCreatesTopics`, `displayAsForum`).
    pub allows_users_to_create_topics: bool,
    pub status: UserStatusKind,
    /// `profile_photo.small.id` (`profilePhoto`, schema 1.8.67 line 754);
    /// 0 = no photo.
    pub photo_small_file_id: i32,
    /// `user.accent_color_id` (schema 1.8.67, line 2383): the name color in
    /// chats. 0–6 are the built-in colors (red, orange, violet, green,
    /// cyan, blue, pink); higher ids are server palettes.
    pub accent_color_id: i32,
    /// A12: `user.profile_accent_color_id` (schema 1.8.67, line 2386) —
    /// the accent color for the user's profile; -1 if none.
    pub profile_accent_color_id: i32,
    /// A12: `user.profile_background_custom_emoji_id` (schema 1.8.67,
    /// line 2403) — preserved when `setProfileAccentColor` changes the
    /// color; 0 if none.
    pub profile_background_custom_emoji_id: i64,
    /// Rich-text premium gate: `user.is_premium` (schema 1.8.67, line
    /// 2403) — whether the user has Telegram Premium (gates
    /// `premiumFeatureRichMessages`, "The ability to send rich messages").
    pub is_premium: bool,
    /// Chat-row title badge: `user.verification_status` (schema line 2403,
    /// `verificationStatus` line 860). Older payloads carried top-level
    /// `is_verified` / `is_scam` / `is_fake`; those are read as a fallback.
    pub verification: crate::peer_badge::VerificationStatus,
    /// `user.emoji_status.type` custom emoji id (schema line 2343), 0 when
    /// there is no status or it is not a custom emoji. TDLib sends
    /// `updateUser` when a status expires, so the expiry is not tracked.
    pub emoji_status_id: i64,
}

impl ParsedUser {
    pub fn display_name(&self) -> String {
        let name = format!("{} {}", self.first_name, self.last_name);
        let name = name.trim();
        if name.is_empty() {
            format!("User {}", self.id)
        } else {
            name.to_string()
        }
    }

    /// Two-letter avatar fallback ("Ada Lovelace" → "AL").
    pub fn initials(&self) -> String {
        let mut out = String::new();
        for part in [&self.first_name, &self.last_name] {
            if let Some(ch) = part.chars().next() {
                out.push(ch);
                if out.chars().count() == 2 {
                    break;
                }
            }
        }
        if out.is_empty() {
            out.push('?');
        }
        out
    }
}

/// Phase 6: `userStatus*` parser — null/absent/unknown → `Empty`.
pub(crate) fn parse_user_status(value: Option<&Value>) -> UserStatusKind {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return UserStatusKind::Empty;
    };
    match value.get("@type").and_then(Value::as_str) {
        Some("userStatusOnline") => UserStatusKind::Online,
        Some("userStatusOffline") => UserStatusKind::Offline {
            was_online: value
                .get("was_online")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32(),
        },
        Some("userStatusRecently") => UserStatusKind::Recently,
        Some("userStatusLastWeek") => UserStatusKind::LastWeek,
        Some("userStatusLastMonth") => UserStatusKind::LastMonth,
        _ => UserStatusKind::Empty,
    }
}

/// A5: full username lists from `usernames` (schema 1.8.67, line 2372).
/// Null/absent → empty lists and an empty editable username.
pub(crate) fn parse_username_lists(value: Option<&Value>) -> (Vec<String>, Vec<String>, String) {
    fn names(value: Option<&Value>, key: &str) -> Vec<String> {
        value
            .filter(|v| !v.is_null())
            .and_then(|u| u.get(key))
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    }
    let editable = value
        .filter(|v| !v.is_null())
        .and_then(|u| u.get("editable_username"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    (
        names(value, "active_usernames"),
        names(value, "disabled_usernames"),
        editable,
    )
}

/// A5: `checkChatUsernameResult*` (schema 1.8.67, lines 8583–8598) — the
/// availability verdict for the current user's own username, queried via
/// `checkChatUsername` with the private chat with self (the documented
/// path per the schema doc at line 11676; TGX `EditUsernameController`
/// does the same with `tdlib.selfChatId()`). TDLib exposes no
/// self-username `checkUsername` — a concept-level search of td_api.tl
/// finds only the chat-scoped `checkChatUsername` and the bot-scoped
/// `checkBotUsername`, so this is the honest wiring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsernameCheckResult {
    Available,
    Occupied,
    Invalid,
    Purchasable,
    PublicChatsTooMany,
    PublicGroupsUnavailable,
}

/// Parity slice: first entry of `usernames.active_usernames` (schema
/// 1.8.67, line 2372 — "the first one must be shown as the primary
/// username"). Null/absent/empty → empty string.
pub(crate) fn parse_first_active_username(value: Option<&Value>) -> String {
    value
        .filter(|v| !v.is_null())
        .and_then(|u| u.get("active_usernames"))
        .and_then(Value::as_array)
        .and_then(|names| names.first())
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Phase 6: full `user` parser (schema 1.8.67, line 2403). `None` when the
/// object carries no id.
pub(crate) fn parse_user(value: &Value) -> Option<ParsedUser> {
    let id = int53(value.get("id")).ok()?;
    let first_name = value
        .get("first_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let last_name = value
        .get("last_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let (active_usernames, disabled_usernames, editable_username) =
        parse_username_lists(value.get("usernames"));
    let username = active_usernames.first().cloned().unwrap_or_default();
    let phone_number = value
        .get("phone_number")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let is_contact = value
        .get("is_contact")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_bot = value
        .get("type")
        .and_then(|t| t.get("@type"))
        .and_then(Value::as_str)
        == Some("userTypeBot");
    // Bots slice: `userTypeBot.is_inline` — only present on the bot
    // type object; defaults false for everyone else.
    let is_inline = value
        .get("type")
        .and_then(|t| t.get("is_inline"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let bot_flag = |field: &str| {
        value
            .get("type")
            .filter(|_| is_bot)
            .and_then(|t| t.get(field))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let has_topics = bot_flag("has_topics");
    let allows_users_to_create_topics = bot_flag("allows_users_to_create_topics");
    let status = parse_user_status(value.get("status"));
    let photo_small_file_id = i32::try_from(int53_or_zero(
        value
            .get("profile_photo")
            .and_then(|p| p.get("small"))
            .and_then(|f| f.get("id")),
    ))
    .unwrap_or(0);
    let accent_color_id = value
        .get("accent_color_id")
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(0);
    // A12: -1 = no profile accent color (schema 1.8.67, line 2386).
    let profile_accent_color_id = value
        .get("profile_accent_color_id")
        .and_then(Value::as_i64)
        .and_then(|n| i32::try_from(n).ok())
        .unwrap_or(-1);
    let profile_background_custom_emoji_id = value
        .get("profile_background_custom_emoji_id")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let is_premium = value
        .get("is_premium")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let flag = |name: &str| {
        value
            .get("verification_status")
            .and_then(|v| v.get(name))
            .or_else(|| value.get(name))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    let verification = crate::peer_badge::VerificationStatus {
        is_verified: flag("is_verified"),
        is_scam: flag("is_scam"),
        is_fake: flag("is_fake"),
    };
    let emoji_status_id = value
        .get("emoji_status")
        .and_then(|s| s.get("type"))
        .and_then(|t| t.get("custom_emoji_id"))
        .and_then(|id| int64(Some(id)))
        .unwrap_or(0);
    Some(ParsedUser {
        id,
        first_name,
        last_name,
        username,
        active_usernames,
        disabled_usernames,
        editable_username,
        phone_number,
        is_contact,
        is_bot,
        is_inline,
        has_topics,
        allows_users_to_create_topics,
        status,
        photo_small_file_id,
        accent_color_id,
        profile_accent_color_id,
        profile_background_custom_emoji_id,
        is_premium,
        verification,
        emoji_status_id,
    })
}

/// Phase 6: preferred profile-photo file from a `userFullInfo` (or the
/// nested `user_full_info` of an `updateUserFullInfo`) object's
/// `photo:chatPhoto` (schema 1.8.67, lines 1030 and 2468). Reuses
/// `parse_photo_sizes`; prefers `type == "m"`, else the largest size up to
/// 320px wide, else the smallest size. `None` when there is no photo or
/// the constructor is not a `chatPhoto`.
pub(crate) fn parse_user_full_info_photo(value: &Value) -> Option<ParsedFile> {
    let photo = value.get("photo")?;
    if photo.get("@type").and_then(Value::as_str) != Some("chatPhoto") {
        return None;
    }
    let (sizes, files) = parse_photo_sizes(photo);
    let pick = sizes
        .iter()
        .find(|size| size.type_name == "m")
        .or_else(|| {
            sizes
                .iter()
                .filter(|size| size.width > 0 && size.width <= 320)
                .max_by_key(|size| size.width)
        })
        .or_else(|| sizes.iter().min_by_key(|size| (size.width, size.height)))?;
    files.into_iter().find(|file| file.id == pick.file_id)
}

/// B10: one `chatPhoto` of `getUserProfilePhotos`. The grid thumbnail is
/// the same pick as the panel photo (`"m"`, else the largest up to 320px
/// wide, else the smallest); the full size is the largest. `None` when
/// the photo has no usable size.
pub(crate) fn parse_profile_photo(photo: &Value) -> Option<ParsedProfilePhoto> {
    let (sizes, files) = parse_photo_sizes(photo);
    let full = sizes.iter().max_by_key(|size| (size.width, size.height))?;
    let thumb = sizes
        .iter()
        .find(|size| size.type_name == "m")
        .or_else(|| {
            sizes
                .iter()
                .filter(|size| size.width > 0 && size.width <= 320)
                .max_by_key(|size| size.width)
        })
        .or_else(|| sizes.iter().min_by_key(|size| (size.width, size.height)))?;
    Some(ParsedProfilePhoto {
        id: int53(photo.get("id")).ok()?,
        added_date: int53_or_zero(photo.get("added_date")).sat_i32(),
        thumb_file_id: thumb.file_id,
        full_file_id: full.file_id,
        width: full.width,
        height: full.height,
        files,
    })
}
