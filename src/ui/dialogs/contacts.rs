use super::super::*;

/// Phase 6: add-contact dialog opened from the user info panel. The phone
/// number is required — `addContact` needs an `importedContact` and Quill
/// does not offer adding by bare user id.
pub struct AddContactDialog {
    pub(crate) user_id: i64,
    pub(crate) phone_input: Entity<TextareaState>,
    pub(crate) first_name_input: Entity<TextareaState>,
    pub(crate) last_name_input: Entity<TextareaState>,
}

/// Slice A6: vCard import dialog — the user pastes the file's text and
/// `parse_vcard` (telegram::requests) turns it into `ImportedContact`
/// cards for `importContacts` (schema 1.8.67, line 14517).
pub struct ImportContactsDialog {
    pub(crate) input: Entity<TextareaState>,
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
                .placeholder("Phone number")
                .auto_grow(1, 1)
                .submit_on_enter(false);
            state.set_value(phone, window, cx);
            state
        });
        let first_name_input = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .placeholder("First name")
                .auto_grow(1, 1)
                .submit_on_enter(false);
            state.set_value(first_name, window, cx);
            state
        });
        let last_name_input = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .placeholder("Last name")
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
        }
    }

    /// `None` when the phone field is empty (the Add button no-ops then).
    pub(crate) fn draft(&self, cx: &App) -> Option<(i64, String, String, String)> {
        let phone = self.phone_input.read(cx).value().to_string();
        if phone.trim().is_empty() {
            return None;
        }
        Some((
            self.user_id,
            phone,
            self.first_name_input.read(cx).value().to_string(),
            self.last_name_input.read(cx).value().to_string(),
        ))
    }
}
