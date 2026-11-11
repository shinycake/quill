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

    /// Cancel on the open row. Refused (`false`) while its Save is out.
    pub(crate) fn cancel(&mut self, requests: Option<&quill::state::RequestRegistry>) -> bool {
        if self.is_saving(requests) {
            return false;
        }
        *self = Self::Idle;
        true
    }

    /// Edit on a row. Refused (`false`) while a Save is out: its answer
    /// would have nothing to report to.
    pub(crate) fn open(
        &mut self,
        welcome_id: i32,
        requests: Option<&quill::state::RequestRegistry>,
    ) -> bool {
        if self.is_saving(requests) {
            return false;
        }
        *self = Self::row(welcome_id);
        true
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

#[cfg(test)]
mod tests {
    use super::WelcomeEdit;
    use quill::state::{RequestPurpose, RequestRegistry, Submit};

    /// 10-071: Cancel and Edit on another row leave a Save that is out
    /// alone; once it answered they work again.
    #[test]
    fn a_row_whose_save_is_out_is_neither_cancelled_nor_swapped() {
        let mut registry = RequestRegistry::default();
        let id = registry.register(
            quill::ids::AccountGeneration::default(),
            RequestPurpose::SetChatTitle,
            None,
            None,
        );
        let mut edit = WelcomeEdit::Row {
            welcome_id: 1,
            save: Submit::sent(vec![id]),
        };
        assert!(edit.is_saving(Some(&registry)));
        assert!(!edit.cancel(Some(&registry)), "Cancel is refused");
        assert!(!edit.open(2, Some(&registry)), "so is another row");
        assert_eq!(edit.welcome_id(), Some(1));
        registry.take(id);
        assert!(edit.open(2, Some(&registry)), "answered: free again");
        assert_eq!(edit.welcome_id(), Some(2));
        assert!(edit.cancel(Some(&registry)));
        assert_eq!(edit.welcome_id(), None);
    }
}
