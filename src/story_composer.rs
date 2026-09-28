//! Phase 9.3: story posting composer — pure state, no GPUI.
//!
//! The composer is a path-entry dialog (no native file-picker
//! infrastructure exists yet — see DECISIONS.md Phase 9.3): a local
//! photo/video path, a caption, and a 4-way privacy selector. The actual
//! TDLib shapes live in `telegram::requests` (`postStory`,
//! `canPostStory`) and `telegram::envelope` (`canPostStoryResult*`);
//! this module holds the composer-side state the UI overlay renders:
//! privacy selection, the selected-users set, the eligibility
//! gate (`canPostStory` is sent before every post), and — Phase 9.4 —
//! expiry selection, the post-to-chat-page / protect-content toggles,
//! and story areas (link + suggested-reaction types only).
//!
//! Scope: photo/video, Everyone / Contacts / Close friends /
//! Selected users, caption with entities (via `formatted_caption`),
//! expiry, post-to-chat-page, protect-content, link + reaction areas,
//! honest pending / succeeded / failed states. Out: location / venue /
//! message / weather / gift areas, editing, posting as a channel.

use serde_json::{Value, json};

/// Media kinds the composer accepts, detected from the path extension.
/// `Unknown` means the Post button stays disabled — the user must point
/// at a real photo/video file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StoryMediaKind {
    Photo,
    Video,
    #[default]
    Unknown,
}

impl StoryMediaKind {
    /// Classify a file path by extension (case-insensitive).
    pub fn detect(path: &str) -> Self {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        match ext.as_str() {
            "jpg" | "jpeg" | "png" | "webp" | "heic" | "bmp" => StoryMediaKind::Photo,
            // Telegram stories treat GIFs as animation videos.
            "gif" | "mp4" | "mov" | "mkv" | "webm" | "3gp" | "avi" => StoryMediaKind::Video,
            _ => StoryMediaKind::Unknown,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            StoryMediaKind::Photo => "Photo",
            StoryMediaKind::Video => "Video",
            StoryMediaKind::Unknown => "Unknown type",
        }
    }
}

/// The four story privacy levels (TDLib 1.8.67, `schema/td_api.tl:8928`
/// – `td_api.tl:8937`). `except_user_ids` is unused this slice — no
/// exclusion UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StoryPrivacy {
    #[default]
    Everyone,
    Contacts,
    CloseFriends,
    SelectedUsers,
}

impl StoryPrivacy {
    pub const ALL: [StoryPrivacy; 4] = [
        StoryPrivacy::Everyone,
        StoryPrivacy::Contacts,
        StoryPrivacy::CloseFriends,
        StoryPrivacy::SelectedUsers,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            StoryPrivacy::Everyone => "Everyone",
            StoryPrivacy::Contacts => "Contacts",
            StoryPrivacy::CloseFriends => "Close friends",
            StoryPrivacy::SelectedUsers => "Selected users",
        }
    }

    /// `StoryPrivacySettings` JSON for `postStory` (schema 1.8.67
    /// `td_api.tl:8928` – `td_api.tl:8937`). `user_ids` only applies to
    /// `SelectedUsers`.
    pub fn settings_json(&self, user_ids: &[i64]) -> Value {
        match self {
            StoryPrivacy::Everyone => json!({
                "@type": "storyPrivacySettingsEveryone",
                "except_user_ids": []
            }),
            StoryPrivacy::Contacts => json!({
                "@type": "storyPrivacySettingsContacts",
                "except_user_ids": []
            }),
            StoryPrivacy::CloseFriends => json!({
                "@type": "storyPrivacySettingsCloseFriends"
            }),
            StoryPrivacy::SelectedUsers => json!({
                "@type": "storyPrivacySettingsSelectedUsers",
                "user_ids": user_ids
            }),
        }
    }
}

/// Phase 9.4: story expiry — the only `active_period` values TDLib
/// 1.8.67 accepts (schema `td_api.tl:13715`, parameter comment: "must be
/// one of 6 * 3600, 12 * 3600, 86400, or 2 * 86400 for Telegram Premium
/// users, and 86400 otherwise"). The 6h / 12h / 48h options need
/// Premium; the server rejects them honestly for non-Premium accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StoryExpiry {
    SixHours,
    TwelveHours,
    #[default]
    Day,
    TwoDays,
}

impl StoryExpiry {
    pub const ALL: [StoryExpiry; 4] = [
        StoryExpiry::SixHours,
        StoryExpiry::TwelveHours,
        StoryExpiry::Day,
        StoryExpiry::TwoDays,
    ];

    pub fn seconds(&self) -> i32 {
        match self {
            StoryExpiry::SixHours => 21600,
            StoryExpiry::TwelveHours => 43200,
            StoryExpiry::Day => 86400,
            StoryExpiry::TwoDays => 172800,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            StoryExpiry::SixHours => "6 hours (Premium)",
            StoryExpiry::TwelveHours => "12 hours (Premium)",
            StoryExpiry::Day => "24 hours",
            StoryExpiry::TwoDays => "48 hours (Premium)",
        }
    }
}

/// `storyAreaPosition` (schema 1.8.67, `td_api.tl:6530`) — percentages
/// of the story media. The composer has no media canvas yet, so areas
/// get these fixed sensible placements.
// ponytail: drag placement once a media canvas exists.
fn area_position(x: f64, y: f64, w: f64, h: f64, corner_radius: f64) -> Value {
    json!({
        "@type": "storyAreaPosition",
        "x_percentage": x,
        "y_percentage": y,
        "width_percentage": w,
        "height_percentage": h,
        "rotation_angle": 0.0,
        "corner_radius_percentage": corner_radius
    })
}

/// Composer state. Pure (no GPUI); the UI layer owns the text inputs and
/// syncs them into this before posting.
#[derive(Debug, Clone, Default)]
pub struct StoryComposer {
    pub open: bool,
    pub privacy: StoryPrivacy,
    pub selected_user_ids: Vec<i64>,
    /// Phase 9.4: expiry selection, post-to-chat-page, protect-content,
    /// link area URL, and space-separated suggested-reaction emoji.
    pub expiry: StoryExpiry,
    pub post_to_chat_page: bool,
    pub protect_content: bool,
    pub link_url: String,
    pub reaction_emojis: String,
    /// A `canPostStory` round-trip is in flight; the UI tick converts
    /// `Session::story_post.eligibility` into a post or an ineligible
    /// message once it lands.
    pub check_sent: bool,
    /// `postStory` was sent and its answer has not landed yet. While
    /// set the Post button stays disabled (double-press would post a
    /// duplicate story). The UI tick clears it once
    /// `Session::story_post.outcome` leaves `None`.
    pub post_sent: bool,
    /// UI-local validation / flow error (no valid media path yet, no
    /// users selected, demo mode, …). Cleared on open.
    pub local_error: Option<String>,
}

impl StoryComposer {
    pub fn open() -> Self {
        Self {
            open: true,
            ..Default::default()
        }
    }

    pub fn close(&mut self) {
        *self = Self::default();
    }

    /// `true` when the privacy selector needs at least one picked user.
    pub fn needs_users(&self) -> bool {
        matches!(self.privacy, StoryPrivacy::SelectedUsers) && self.selected_user_ids.is_empty()
    }

    pub fn toggle_user(&mut self, user_id: i64) {
        if let Some(pos) = self.selected_user_ids.iter().position(|id| *id == user_id) {
            self.selected_user_ids.remove(pos);
        } else {
            self.selected_user_ids.push(user_id);
        }
    }

    /// `inputStoryAreas` JSON for `postStory` (schema 1.8.67,
    /// `td_api.tl:6619` / `td_api.tl:6610`). Built from the composer's
    /// link URL and reaction emoji — the only area types expressible
    /// with plain text inputs: a link area (`inputStoryAreaTypeLink`,
    /// `td_api.tl:6597`; HTTP or tg:// URL; schema limits: Premium-only,
    /// up to `story_link_area_count_max`), and one
    /// `inputStoryAreaTypeSuggestedReaction` (`td_api.tl:6588`) with
    /// `reactionTypeEmoji` (`td_api.tl:2915`) per emoji.
    /// Location / venue / message / weather / gift areas need pickers or
    /// live data that don't exist yet (DECISIONS.md Phase 9.4).
    /// The returned value is baked into the `postStory` request JSON, so
    /// the areas ride along with the post→answer correlation (temp
    /// story id → `updateStoryPostSucceeded`/`updateStoryPostFailed`).
    pub fn areas_json(&self) -> Value {
        let mut areas = Vec::new();
        let url = self.link_url.trim();
        if !url.is_empty() {
            areas.push(json!({
                "@type": "inputStoryArea",
                "position": area_position(35.0, 80.0, 30.0, 9.0, 20.0),
                "type": { "@type": "inputStoryAreaTypeLink", "url": url }
            }));
        }
        // ponytail: UI-level cap of 5 emojis; server enforces the real
        // `story_suggested_reaction_area_count_max`. Raise the take() bound if
        // the server max is ever surfaced client-side.
        for (i, emoji) in self.reaction_emojis.split_whitespace().take(5).enumerate() {
            areas.push(json!({
                "@type": "inputStoryArea",
                "position": area_position(44.0, 24.0 + i as f64 * 13.0, 12.0, 12.0, 50.0),
                "type": {
                    "@type": "inputStoryAreaTypeSuggestedReaction",
                    "reaction_type": { "@type": "reactionTypeEmoji", "emoji": emoji },
                    "is_dark": false,
                    "is_flipped": false
                }
            }));
        }
        json!({ "@type": "inputStoryAreas", "areas": areas })
    }

    /// Light validation for the link area (schema 1.8.67 `td_api.tl:6597`:
    /// "An area pointing to a HTTP or tg:// link"). Empty = no link area.
    pub fn link_url_error(&self) -> Option<&'static str> {
        let url = self.link_url.trim();
        if url.is_empty() {
            return None;
        }
        if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("tg://") {
            None
        } else {
            Some("Link sticker URL must start with http://, https:// or tg://")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_kind_detection() {
        assert_eq!(StoryMediaKind::detect("/a/b.jpg"), StoryMediaKind::Photo);
        assert_eq!(StoryMediaKind::detect("clip.MP4"), StoryMediaKind::Video);
        // Telegram stories treat GIFs as animation videos.
        assert_eq!(StoryMediaKind::detect("anim.gif"), StoryMediaKind::Video);
        assert_eq!(StoryMediaKind::detect("noext"), StoryMediaKind::Unknown);
        assert_eq!(StoryMediaKind::detect("a.txt"), StoryMediaKind::Unknown);
    }

    #[test]
    fn privacy_settings_json_shapes() {
        let json = StoryPrivacy::Everyone.settings_json(&[]);
        assert_eq!(json["@type"], "storyPrivacySettingsEveryone");
        assert_eq!(json["except_user_ids"], json!([]));
        let json = StoryPrivacy::CloseFriends.settings_json(&[]);
        assert_eq!(json["@type"], "storyPrivacySettingsCloseFriends");
        let json = StoryPrivacy::SelectedUsers.settings_json(&[7, 9]);
        assert_eq!(json["@type"], "storyPrivacySettingsSelectedUsers");
        assert_eq!(json["user_ids"], json!([7, 9]));
    }

    #[test]
    fn composer_open_close_toggle_users() {
        let mut c = StoryComposer::open();
        assert!(c.open);
        assert!(!c.needs_users());
        c.privacy = StoryPrivacy::SelectedUsers;
        assert!(c.needs_users());
        c.toggle_user(7);
        assert!(!c.needs_users());
        c.toggle_user(7);
        assert!(c.needs_users());
        c.close();
        assert!(!c.open);
        assert!(c.selected_user_ids.is_empty());
    }

    #[test]
    fn expiry_maps_to_schema_legal_values() {
        // td_api.tl:13715 comment: 6 * 3600, 12 * 3600, 86400, 2 * 86400.
        assert_eq!(StoryExpiry::SixHours.seconds(), 21600);
        assert_eq!(StoryExpiry::TwelveHours.seconds(), 43200);
        assert_eq!(StoryExpiry::Day.seconds(), 86400);
        assert_eq!(StoryExpiry::TwoDays.seconds(), 172800);
        // Default is the non-Premium legal value.
        assert_eq!(StoryComposer::default().expiry, StoryExpiry::Day);
    }

    #[test]
    fn areas_json_shapes_match_1_8_67() {
        // Empty inputs → empty areas.
        let c = StoryComposer::default();
        let v = c.areas_json();
        assert_eq!(v["@type"], "inputStoryAreas");
        assert_eq!(v["areas"], json!([]));

        let c = StoryComposer {
            link_url: " https://t.me/quill ".into(),
            reaction_emojis: "❤️ 🔥".into(),
            ..Default::default()
        };
        let v = c.areas_json();
        let areas = v["areas"].as_array().unwrap();
        assert_eq!(areas.len(), 3);
        // Link area: td_api.tl:6610 (inputStoryArea) / td_api.tl:6597
        // (inputStoryAreaTypeLink); position td_api.tl:6530.
        assert_eq!(areas[0]["@type"], "inputStoryArea");
        assert_eq!(areas[0]["position"]["@type"], "storyAreaPosition");
        assert_eq!(areas[0]["type"]["@type"], "inputStoryAreaTypeLink");
        assert_eq!(areas[0]["type"]["url"], "https://t.me/quill");
        // Reaction areas: td_api.tl:6588 + td_api.tl:2915.
        assert_eq!(
            areas[1]["type"]["@type"],
            "inputStoryAreaTypeSuggestedReaction"
        );
        assert_eq!(
            areas[1]["type"]["reaction_type"]["@type"],
            "reactionTypeEmoji"
        );
        assert_eq!(areas[1]["type"]["reaction_type"]["emoji"], "❤️");
        assert_eq!(areas[1]["type"]["is_dark"], false);
        assert_eq!(areas[1]["type"]["is_flipped"], false);
        assert_eq!(areas[2]["type"]["reaction_type"]["emoji"], "🔥");
    }

    #[test]
    fn link_url_validation_accepts_schema_schemes() {
        let mut c = StoryComposer::default();
        assert!(c.link_url_error().is_none());
        for url in ["http://a.b", "https://t.me/quill", "tg://resolve"] {
            c.link_url = url.into();
            assert!(c.link_url_error().is_none(), "{url}");
        }
        c.link_url = "t.me/quill".into();
        assert!(c.link_url_error().is_some());
    }
}
