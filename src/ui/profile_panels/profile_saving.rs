//! The profile boxes that save with one request (edit contact, birthday,
//! personal channel and photo, share contact): once sent, the box stays
//! open in `ProfileDialog::Saving` until the request is answered, as
//! tdesktop's boxes do. The answer closes it with its toast; a refusal
//! puts the box back with the reason; closing it stops the wait.
use super::*;
use crate::ui::dialogs::ContactSave;
use crate::ui::status_note::StatusNote;
use quill::ids::RequestId;
use quill::lang::{keys, tr};
use quill::state::SaveOutcome;

impl QuillApp {
    /// The box's request went out. With a box open it waits, saving, for
    /// the answer and the caller keeps it open (`false`); with nothing to
    /// wait for (no request, no box) the toast shows now and the caller
    /// closes (`true`).
    pub(super) fn finish_profile_request(
        &mut self,
        request: Option<RequestId>,
        saved: StatusNote,
        cx: &mut Context<Self>,
    ) -> bool {
        let (Some(request), Some(dialog)) = (request, self.dialogs.profile_dialog.take()) else {
            self.set_note(saved);
            return true;
        };
        if let Some(live) = self.live.as_mut() {
            live.driver.session.users_state.saves.watch(request);
        }
        // box-host: the box is already open where it shows; this only changes its stage.
        self.dialogs.profile_dialog = Some(ProfileDialog::Saving {
            dialog: Box::new(dialog),
            request,
            saved,
        });
        cx.notify();
        false
    }

    /// Open the profile box on `dialog`: the box and the window that shows
    /// it are set together, so one cannot be left without the other.
    pub(in crate::ui) fn open_profile_dialog(&mut self, dialog: ProfileDialog) {
        // A box replaced while it saves no longer waits for its request.
        self.forget_profile_waits();
        self.dialogs.profile_dialog = Some(dialog);
        self.open_box(DialogKind::ProfilePanel);
    }

    /// The box goes: close it, with whatever it waited on forgotten. The one
    /// way a profile box is dropped (also when another replaces it).
    pub(in crate::ui) fn drop_profile_dialog(&mut self) {
        self.forget_profile_waits();
        self.forget_box_send();
        self.dialogs.profile_dialog = None;
    }

    /// The box is closed or replaced: a request it waits on is dropped. A
    /// `Saving` stage stops its watch; an Edit contact's pending `addContact`
    /// discards its answer if it has already landed.
    fn forget_profile_waits(&mut self) {
        self.forget_profile_saving();
        let Some(ProfileDialog::EditContact(EditContactDialog {
            save: ContactSave::Saving(request),
            ..
        })) = &self.dialogs.profile_dialog
        else {
            return;
        };
        let request = *request;
        if let Some(session) = self.session_mut() {
            session.chat_list.dialog_answers.take(request);
        }
    }

    /// The box is closed or replaced: its request is no longer waited on.
    pub(super) fn forget_profile_saving(&mut self) {
        let Some(ProfileDialog::Saving { request, .. }) = &self.dialogs.profile_dialog else {
            return;
        };
        let request = *request;
        if let Some(live) = self.live.as_mut() {
            live.driver.session.users_state.saves.forget(request);
        }
    }

    /// Each frame of the window that hosts the box: take the answer of the
    /// saving request. Saved closes the box with its toast; refused puts
    /// the box back and says why.
    pub(in crate::ui) fn profile_saving_upkeep(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ProfileDialog::Saving { request, .. }) = &self.dialogs.profile_dialog else {
            return;
        };
        let request = *request;
        let Some(live) = self.live.as_mut() else {
            return;
        };
        let session = &mut live.driver.session;
        let outcome = session
            .users_state
            .saves
            .take_outcome(request, &session.requests);
        match outcome {
            SaveOutcome::Saving => {}
            SaveOutcome::Saved => {
                let Some(ProfileDialog::Saving { saved, .. }) = self.dialogs.profile_dialog.take()
                else {
                    return;
                };
                self.set_note(saved);
                self.close_kit_dialog_if_done(DialogKind::ProfilePanel, window, cx);
                cx.notify();
            }
            // No answer to act on: the wait was dropped under it.
            SaveOutcome::Unknown => {
                if let Some(ProfileDialog::Saving { dialog, .. }) =
                    self.dialogs.profile_dialog.take()
                {
                    // box-host: the box is already open where it shows; this only changes its stage.
                    self.dialogs.profile_dialog = Some(*dialog);
                }
                cx.notify();
            }
            SaveOutcome::Failed(reason) => {
                if let Some(ProfileDialog::Saving { dialog, .. }) =
                    self.dialogs.profile_dialog.take()
                {
                    // box-host: the box is already open where it shows; this only changes its stage.
                    self.dialogs.profile_dialog = Some(*dialog);
                }
                self.set_note(StatusNote::error(reason));
                cx.notify();
            }
        }
    }

    /// The saving box: the box as it was, its buttons replaced by a
    /// disabled "Saving…".
    pub(super) fn saving_dialog_parts(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<(String, AnyElement, AnyElement)> {
        let Some(ProfileDialog::Saving {
            dialog,
            request,
            saved,
        }) = self.dialogs.profile_dialog.take()
        else {
            return None;
        };
        // box-host: the box is already open where it shows; this only changes its stage.
        self.dialogs.profile_dialog = Some(*dialog);
        let parts = self.profile_dialog_parts(cx);
        if let Some(inner) = self.dialogs.profile_dialog.take() {
            // box-host: the box is already open where it shows; this only changes its stage.
            self.dialogs.profile_dialog = Some(ProfileDialog::Saving {
                dialog: Box::new(inner),
                request,
                saved,
            });
        }
        let (title, body, _) = parts?;
        let body = block_while_saving(body);
        let footer = Button::new("profile-saving")
            .label(tr(keys::Q_MEDIA_SAVING))
            .disabled(true);
        Some((
            title,
            body,
            div().flex().gap_2().child(footer).into_any_element(),
        ))
    }
}

/// A saving box keeps its rows but takes no click: another box opened from
/// one of them would replace the box whose request is still out.
pub(super) fn block_while_saving(body: AnyElement) -> AnyElement {
    div()
        .relative()
        .child(body)
        .child(div().absolute().top_0().left_0().size_full().occlude())
        .into_any_element()
}
