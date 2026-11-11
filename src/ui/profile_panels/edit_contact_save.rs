//! The Edit Contact box's save: `addContact` (add-or-edit) is a stage the
//! box waits out (`ContactSave`), settled by the answer to its own request
//! (`DialogAnswers`), like the Add Contact box. It closes with the toast
//! only when the contact is saved, and shows the refusal inline otherwise.

use super::*;
use crate::ui::dialogs::{ContactNote, ContactSave};
use crate::ui::user_error::user_error_for;
use quill::state::DialogAnswer;

impl QuillApp {
    /// Save the edit-contact box. Only the note changed: `setUserNote`;
    /// otherwise `addContact` with the current note so the note is not
    /// cleared. Returns whether the box is done and closes now; a save
    /// that waits for its answer returns `false` and stays in `Saving`.
    pub(super) fn submit_edit_contact(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(ProfileDialog::EditContact(dialog)) = &self.dialogs.profile_dialog else {
            return false;
        };
        if matches!(dialog.save, ContactSave::Saving(_)) {
            // The first Save is still waiting for its answer.
            return false;
        }
        let read = |input: &Entity<TextareaState>| {
            input
                .read(cx)
                .value()
                .replace(['\n', '\r'], " ")
                .trim()
                .to_string()
        };
        let (user_id, first, last, note, share) = (
            dialog.user_id,
            read(&dialog.first_name_input),
            read(&dialog.last_name_input),
            dialog.note_input.read(cx).value().trim().to_string(),
            dialog.share_phone,
        );
        // An empty field before the note is known is not "no note": saving
        // it would erase the contact's note. It is sent as "keep the note"
        // (null), so Save works even when the note never loads (a failed
        // full-info fetch has no stage to wait out).
        let note = (dialog.note == ContactNote::Seeded || !note.is_empty()).then_some(note);
        if first.is_empty() && last.is_empty() {
            self.set_note(StatusNote::silent(tr(keys::Q_PEOPLE_ENTER_A_NAME_FOR_THE)));
            cx.notify();
            return false;
        }
        let Some((phone, old_first, old_last)) = self.session().and_then(|s| {
            s.user(user_id).map(|u| {
                (
                    u.phone_number.clone(),
                    u.first_name.clone(),
                    u.last_name.clone(),
                )
            })
        }) else {
            return false;
        };
        let Some(live) = self.live.as_mut() else {
            self.set_note(StatusNote::info(tr(
                keys::Q_PEOPLE_DEMO_MODE_CONTACT_EDITS_NEED,
            )));
            cx.notify();
            return true;
        };
        let names_unchanged = first == old_first && last == old_last;
        let sent = match (&note, names_unchanged && !share) {
            (Some(note), true) => live.driver.set_user_note(user_id, note).map(|_| None),
            // Nothing to change: the note is kept and the names are the same.
            (None, true) => Ok(None),
            (note, false) => live
                .driver
                .edit_contact(user_id, &phone, &first, &last, note.as_deref(), share),
        };
        match sent {
            // The box stays until its own request answers.
            Ok(Some(request)) => {
                if let Some(ProfileDialog::EditContact(dialog)) = &mut self.dialogs.profile_dialog {
                    dialog.save = ContactSave::Saving(request);
                }
                cx.notify();
                false
            }
            Ok(None) => {
                self.set_note(StatusNote::silent(tr(keys::Q_PEOPLE_CONTACT_SAVED)));
                true
            }
            Err(err) => {
                if let Some(ProfileDialog::EditContact(dialog)) = &mut self.dialogs.profile_dialog {
                    dialog.save = ContactSave::Failed(user_error_for(
                        keys::Q_PEOPLE_COULDNT_REACH_TELEGRAM_TRY_AGAIN,
                        &err,
                    ));
                }
                cx.notify();
                false
            }
        }
    }

    /// Poll-loop half: the Edit Contact box's `addContact` answered. The
    /// box closes with the toast when the contact is saved, and shows the
    /// refusal (Save tries again) when it is not.
    pub(in crate::ui) fn settle_edit_contact(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(ProfileDialog::EditContact(EditContactDialog {
            save: ContactSave::Saving(request),
            ..
        })) = &self.dialogs.profile_dialog
        else {
            return false;
        };
        let request = *request;
        let answer = self
            .session_mut()
            .and_then(|s| s.chat_list.dialog_answers.take(request));
        let Some(answer) = answer else {
            return false;
        };
        match answer {
            DialogAnswer::Failed(line) => {
                if let Some(ProfileDialog::EditContact(dialog)) = &mut self.dialogs.profile_dialog {
                    dialog.save = ContactSave::Failed(line);
                }
            }
            DialogAnswer::Done | DialogAnswer::Checked(_) | DialogAnswer::LimitHit => {
                self.dialogs.profile_dialog = None;
                self.set_note(StatusNote::silent(tr(keys::Q_PEOPLE_CONTACT_SAVED)));
            }
        }
        cx.notify();
        true
    }
}
