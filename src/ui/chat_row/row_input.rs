//! What one chat-list row is drawn from (`session_chat_row`).

use super::super::userpic_videos::UserpicFrame;
use super::*;
use gpui_kit::assets::IconName;
use quill::chatlist_style::ChatListRowStyle;
use quill::peer_badge::TitleBadge;
use quill::row_fx::RowFx;
use quill::state::ChatSummary;
use quill::story_ring::StoryRing;

/// Everything a chat-list row shows, gathered by the list.
pub(in crate::ui) struct ChatRowInput<'a> {
    pub(in crate::ui) chat: &'a ChatSummary,
    pub(in crate::ui) selected: bool,
    /// Parity slice: `(folder id, name, colour)` for folder-tag chips.
    pub(in crate::ui) folders: &'a [(i32, String, i32)],
    /// The folder being viewed (its own chip is not repeated).
    pub(in crate::ui) viewing_folder: Option<i32>,
    /// Parity slice: show folder-tag chips (`are_folder_tags_enabled`).
    pub(in crate::ui) show_tags: bool,
    /// Parity slice: sandboxed display path for the downloaded chat photo
    /// (`chat.photo.small`), if any; the avatar falls back to colored
    /// initials otherwise.
    pub(in crate::ui) photo_path: Option<&'a std::path::Path>,
    /// Slice CL2: pinned rows are drag-reorderable, but only on the
    /// unfiltered list with at least two pinned chats (TGX
    /// `ChatsAdapter`); the caller computes this.
    pub(in crate::ui) draggable: bool,
    /// Slice CL2: which pinned list the row is in (`setPinnedChats`
    /// takes main or archive).
    pub(in crate::ui) archived: bool,
    /// Slice CL3: multi-select mode — the row toggles selection instead
    /// of opening the chat, and shows the check circle.
    pub(in crate::ui) selecting: bool,
    pub(in crate::ui) checked: bool,
    /// Animated progress for this frame: the online dot on the avatar of
    /// a private chat whose user is online, and a newly shown unread badge.
    pub(in crate::ui) fx: RowFx,
    /// Verified / Premium / SCAM mark after the title.
    pub(in crate::ui) title_badge: Option<TitleBadge>,
    /// Still image for a Premium emoji status, once downloaded.
    pub(in crate::ui) badge_emoji: Option<crate::ui::status_emoji::StatusGlyph>,
    /// The verifying bot's icon before the title, once downloaded.
    pub(in crate::ui) verify_icon: Option<std::path::PathBuf>,
    /// Slice chatlist-list-style: preview line count, media icons, and
    /// formatted preview (Settings → Appearance → Chat list rows), and the
    /// density.
    pub(in crate::ui) row_style: ChatListRowStyle,
    /// The height declared to the `VirtualList` (`chat_row_height`).
    pub(in crate::ui) height: Pixels,
    /// The chat with yourself: "Saved Messages" with a bookmark avatar.
    pub(in crate::ui) saved: bool,
    /// Group previews lead with the sender ("Dad: …", "You: …").
    pub(in crate::ui) preview_sender: Option<String>,
    /// Images for custom emoji in the preview.
    pub(in crate::ui) preview_emoji: super::sticker_player::PreviewEmoji,
    /// Telegram Desktop marks groups, channels and bots before the title.
    pub(in crate::ui) kind_icon: Option<IconName>,
    /// A forum row (`displayAsForum`): its topics fill the second line, the
    /// last message the third.
    pub(in crate::ui) forum: Option<topic_titles::ForumLine>,
    /// Active stories: the ring around the avatar (unread: accent gradient,
    /// read: grey).
    pub(in crate::ui) story_ring: Option<StoryRing>,
    pub(in crate::ui) video: Option<UserpicFrame>,
    /// A promoted chat's badge ("Proxy sponsor", "PSA") in place of the date,
    /// and the announcement text in place of the preview.
    pub(in crate::ui) promotion: Option<quill::promotion::RowBadge>,
    pub(in crate::ui) muted_fg: Hsla,
    /// The fill of a row that is not selected: the list's, or none on a
    /// glass card (`QuillApp::list_row_fill`).
    pub(in crate::ui) rest_fill: Hsla,
    /// The row's corners (`QuillApp::list_row_corners`).
    pub(in crate::ui) corners: Pixels,
    /// The open row's glass on a glass card (`QuillApp::list_row_glass`),
    /// in place of its fill. The row under the pointer keeps the plain
    /// hover fill, so the two stay apart.
    pub(in crate::ui) glass: Option<LensStyle>,
}
