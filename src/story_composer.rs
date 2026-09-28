//! Phase 9.3: story posting composer — pure state, no GPUI.
//!
//! The composer is a path-entry dialog (no native file-picker
//! infrastructure exists yet — see DECISIONS.md Phase 9.3): a local
//! photo/video path, a caption, and a 4-way privacy selector. The actual
//! TDLib shapes live in `telegram::requests` (`postStory`,
//! `canPostStory`) and `telegram::envelope` (`canPostStoryResult*`);
//! this module holds the composer-side state the UI overlay renders:
//! privacy selection, the selected-users set, and the eligibility
//! gate (`canPostStory` is sent before every post).
//!
//! Scope: photo/video, Everyone / Contacts / Close friends /
//! Selected users, caption with entities (via `formatted_caption`),
//! honest pending / succeeded / failed states. Out: story areas, active
//! period / expiry, "post to chat page", "protect content", editing,
//! posting as a channel.

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

/// Composer state. Pure (no GPUI); the UI layer owns the text inputs and
/// syncs them into this before posting.
#[derive(Debug, Clone, Default)]
pub struct StoryComposer {
    pub open: bool,
    pub privacy: StoryPrivacy,
    pub selected_user_ids: Vec<i64>,
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
}
