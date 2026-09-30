use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::ids::ChatId;
/// Slice G1: what a `UsernameDialog` text prompt edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextPromptKind {
    /// `setSupergroupUsername` (schema 1.8.67, line 15136).
    Username,
    /// `setChatMemberTag` custom title (schema 1.8.67, line 13598).
    CustomTitle { user_id: i64 },
    /// Slice G8: `setChatTitle` (schema 1.8.67, line 13430) — 1–128
    /// chars, gated on `can_change_info` (basic groups: every member).
    GroupTitle,
    /// Slice G8: `setChatDescription` (schema 1.8.67, line 13533) —
    /// 0–255 chars, empty clears; same gate as the title.
    GroupDescription,
    /// Slice G8: `setChatPhoto` (schema 1.8.67, line 13435) — a local
    /// file path, empty removes the photo; same gate as the title.
    GroupPhoto,
    /// Slice G10: `setCommunityName` (schema 1.8.67, line 11811).
    /// `chat_id` on the dialog is unused for this kind; the community
    /// id rides the variant.
    CommunityName { community_id: i64 },
}

pub struct UsernameDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) kind: TextPromptKind,
    pub(crate) input: Entity<TextareaState>,
}

impl UsernameDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        chat_id: ChatId,
        kind: TextPromptKind,
        current: &str,
        placeholder: &str,
    ) -> Self {
        let input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(placeholder)
                .auto_grow(1, 1)
                .submit_on_enter(false)
        });
        input.update(cx, |input, cx| {
            input.set_value(current, window, cx);
        });
        Self {
            chat_id,
            kind,
            input,
        }
    }
}
