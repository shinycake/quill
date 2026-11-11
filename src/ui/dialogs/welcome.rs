use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
/// Slice G2: chat welcome-message editor (info panel → "Welcome
/// message", admins with `can_send_welcome_messages` only).
/// `new_input` feeds `addChatWelcomeMessage`; `editing` +
/// `edit_input` drive the inline edit row
/// (`editChatWelcomeMessage`); each row also offers
/// `deleteChatWelcomeMessage`.
pub struct WelcomeDialog {
    pub(crate) chat_id: ChatId,
    pub(crate) new_input: Entity<TextareaState>,
    /// The row being edited, with its Save.
    pub(crate) row: RowEdit,
    pub(crate) edit_input: Entity<TextareaState>,
    /// The last Add: the typed text goes on its answer, stays on its error.
    pub(crate) add: quill::state::Submit,
}

/// The inline edit row. The row id, its Save and the edit text's owner
/// are one value: opening another row replaces it whole.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum RowEdit {
    #[default]
    Closed,
    /// Row `id` is open. `save` is its last Save: the row closes on its
    /// answer and stays open on its error.
    Open { id: i32, save: quill::state::Submit },
}

impl RowEdit {
    /// The id of the row being edited.
    pub(crate) fn id(&self) -> Option<i32> {
        match self {
            RowEdit::Closed => None,
            RowEdit::Open { id, .. } => Some(*id),
        }
    }

    /// The open row's Save.
    pub(crate) fn save(&self) -> Option<&quill::state::Submit> {
        match self {
            RowEdit::Closed => None,
            RowEdit::Open { save, .. } => Some(save),
        }
    }

    /// The open row's Save, to settle or replace.
    pub(crate) fn save_mut(&mut self) -> Option<&mut quill::state::Submit> {
        match self {
            RowEdit::Closed => None,
            RowEdit::Open { save, .. } => Some(save),
        }
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
            row: RowEdit::Closed,
            edit_input,
            add: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RowEdit;
    use quill::ids::RequestId;
    use quill::state::Submit;

    #[test]
    fn a_row_edit_carries_its_row_and_its_save_together() {
        let mut row = RowEdit::Closed;
        assert_eq!(row.id(), None);
        assert!(row.save().is_none());
        assert!(row.save_mut().is_none());
        row = RowEdit::Open {
            id: 4,
            save: Submit::Editing,
        };
        if let Some(save) = row.save_mut() {
            *save = Submit::Sending(vec![RequestId(9)]);
        }
        assert_eq!(row.id(), Some(4));
        assert_eq!(row.save(), Some(&Submit::Sending(vec![RequestId(9)])));
        // Another row replaces the value whole: no save of the old row stays.
        row = RowEdit::Open {
            id: 5,
            save: Submit::Editing,
        };
        assert_eq!(row.save(), Some(&Submit::Editing));
    }
}
