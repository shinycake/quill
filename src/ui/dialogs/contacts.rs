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
    /// New Contact box (`user_id == 0`): the first name of the import in
    /// flight or answered.
    pub(crate) sent_name: Option<String>,
    /// New Contact box: the phone is not on Telegram (tdesktop's
    /// `_retrying` state).
    pub(crate) not_joined: bool,
}

/// Slice A6: vCard import dialog — the user chooses a `.vcf` file and
/// `parse_vcard` (telegram::requests) turns it into `ImportedContact`
/// cards for `importContacts` (schema 1.8.67, line 14517). The box renders
/// from `state` alone.
#[derive(Default)]
pub struct ImportContactsDialog {
    pub(crate) state: quill::vcard_import::VcardImport,
    /// Which file choice the pending read belongs to; a read that lands
    /// after another choice (or after closing) is dropped.
    pub(crate) pick: u64,
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
            sent_name: None,
            not_joined: false,
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
