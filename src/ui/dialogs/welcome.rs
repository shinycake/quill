use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
/// Slice G2: chat welcome-message editor (info panel → "Welcome
/// message", admins with `can_send_welcome_messages` only).
/// `new_input` feeds `addChatWelcomeMessage`; `edit` +
/// `edit_input` drive the inline edit row
/// (`editChatWelcomeMessage`); each row also offers
/// `deleteChatWelcomeMessage`.
pub struct WelcomeDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) new_input: Entity<TextareaState>,
    /// The row being edited, with its Save: the row closes on the answer,
    /// stays open on the error.
    pub(crate) edit: WelcomeEdit,
    pub(crate) edit_input: Entity<TextareaState>,
    /// The last Add: the typed text goes on its answer, stays on its error.
    pub(crate) add: quill::state::Submit,
}

/// The inline edit of one welcome message. A row whose Save is out can
/// neither be cancelled nor swapped for another row: the answer would have
/// nothing to report to.
#[derive(Debug, Default)]
pub(crate) enum WelcomeEdit {
    #[default]
    Idle,
    Row {
        welcome_id: i32,
        save: quill::state::Submit,
    },
}

impl WelcomeEdit {
    /// A row opens for editing, its Save not yet sent.
    pub(crate) fn row(welcome_id: i32) -> Self {
        Self::Row {
            welcome_id,
            save: Default::default(),
        }
    }

    /// The message whose row is open.
    pub(crate) fn welcome_id(&self) -> Option<i32> {
        match self {
            Self::Row { welcome_id, .. } => Some(*welcome_id),
            Self::Idle => None,
        }
    }

    /// The open row's Save, if it is being edited.
    pub(crate) fn save(&self) -> Option<&quill::state::Submit> {
        match self {
            Self::Row { save, .. } => Some(save),
            Self::Idle => None,
        }
    }

    /// Whether the open row's Save is out on `requests`.
    pub(crate) fn is_saving(&self, requests: Option<&quill::state::RequestRegistry>) -> bool {
        self.save().is_some_and(|save| save.is_sending(requests))
    }
}

impl WelcomeDialog {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<QuillApp>, chat_id: ChatId) -> Self {
        let new_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_OPS_NEW_WELCOME_MESSAGE))
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        let edit_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder(tr(keys::WELCOME_MESSAGES_TITLE))
                .auto_grow(1, 3)
                .submit_on_enter(false)
        });
        Self {
            chat_id,
            new_input,
            edit: WelcomeEdit::Idle,
            edit_input,
            add: Default::default(),
        }
    }
}
