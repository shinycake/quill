use super::super::*;

/// Slice G1: what a `UsernameDialog` text prompt edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextPromptKind {
    /// `setSupergroupUsername` (schema 1.8.67, line 15136).
    Username,
    /// `setChatMemberTag` custom title (schema 1.8.67, line 13598).
    CustomTitle { user_id: i64 },
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
