use super::super::*;

/// Slice G2: chat welcome-message editor (info panel → "Welcome
/// message", admins with `can_send_welcome_messages` only).
/// `new_input` feeds `addChatWelcomeMessage`; `editing` +
/// `edit_input` drive the inline edit row
/// (`editChatWelcomeMessage`); each row also offers
/// `deleteChatWelcomeMessage`.
pub struct WelcomeDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) new_input: Entity<TextareaState>,
    pub(crate) editing: Option<i32>,
    pub(crate) edit_input: Entity<TextareaState>,
}

impl WelcomeDialog {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>, chat_id: ChatId) -> Self {
        let new_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("New welcome message")
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        let edit_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("Welcome message")
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        Self {
            chat_id,
            new_input,
            editing: None,
            edit_input,
        }
    }
}
