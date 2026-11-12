use super::super::app::QuillApp;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use quill::lang::{keys, tr};
/// Phase 6: add-contact dialog opened from the user info panel or the chat
/// action bar. A first name or a phone number is required — `addContact`
/// needs an `importedContact`; the phone may stay empty for a known user
/// whose number is hidden (a stranger from the action bar).
pub struct AddContactDialog {
    pub(crate) user_id: i64,
    pub(crate) phone_input: Entity<TextareaState>,
    pub(crate) first_name_input: Entity<TextareaState>,
    pub(crate) last_name_input: Entity<TextareaState>,
    /// Where the box stands: the New Contact import (`user_id == 0`) or the
    /// Add Contact save (`user_id != 0`).
    pub(crate) stage: AddContactStage,
}

/// Where the add-contact box stands. Every stage has a way out: the
/// request's answer (closed, or `NotJoined` / `Failed` with the form back),
/// "Try another contact", or closing the box. The two boxes share it; the
/// New Contact one uses `Importing` / `NotJoined`, the Add Contact one
/// `Saving` / `Failed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddContactStage {
    /// The form is filled in.
    Form,
    /// New Contact: the import it sent, whose answer it applies, and the
    /// first name it was sent with.
    Importing {
        request: quill::ids::RequestId,
        name: String,
    },
    /// New Contact: the phone is not on Telegram (tdesktop's `_retrying`
    /// state).
    NotJoined { name: String },
    /// Add Contact: the `addContact` it waits on; the box closes on its own
    /// answer.
    Saving(quill::ids::RequestId),
    /// Add Contact: the refusal to show above the form.
    Failed(String),
}

/// Where the Edit Contact box's save stands: the box closes on the answer
/// to its own request, and shows the refusal when there is one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContactSave {
    Idle,
    Saving(quill::ids::RequestId),
    Failed(String),
}

/// Slice A6: vCard import dialog — the user chooses a `.vcf` file and
/// `parse_vcard` (telegram::requests) turns it into `ImportedContact`
/// cards for `importContacts` (schema 1.8.67, line 14517). The box renders
/// from `stage` alone; the file picker and the read are tasks of the
/// stage, so closing or replacing the box ends them.
#[derive(Default)]
pub struct ImportContactsDialog {
    pub(crate) stage: crate::ui::contacts_import::ImportStage,
}

impl ImportContactsDialog {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

impl AddContactDialog {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<QuillApp>,
        user_id: i64,
        phone: &str,
        first_name: &str,
        last_name: &str,
    ) -> Self {
        let phone_input = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .placeholder(tr(keys::NEW_CONTACT_PHONE_NUMBER))
                .auto_grow(1, 1)
                .submit_on_enter(false);
            state.set_value(phone, window, cx);
            state
        });
        let first_name_input = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_OPS_FIRST_NAME))
                .auto_grow(1, 1)
                .submit_on_enter(false);
            state.set_value(first_name, window, cx);
            state
        });
        let last_name_input = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .placeholder(tr(keys::Q_OPS_LAST_NAME))
                .auto_grow(1, 1)
                .submit_on_enter(false);
            state.set_value(last_name, window, cx);
            state
        });
        Self {
            user_id,
            phone_input,
            first_name_input,
            last_name_input,
            stage: AddContactStage::Form,
        }
    }

    /// `None` when both the phone and the first name are empty (the Add
    /// button no-ops then).
    pub(crate) fn draft(&self, cx: &App) -> Option<(i64, String, String, String)> {
        let phone = self.phone_input.read(cx).value().to_string();
        let first = self.first_name_input.read(cx).value().to_string();
        if phone.trim().is_empty() && first.trim().is_empty() {
            return None;
        }
        Some((
            self.user_id,
            phone,
            first,
            self.last_name_input.read(cx).value().to_string(),
        ))
    }
}
