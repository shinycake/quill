//! The profile boxes that save with one request (edit contact, birthday,
//! personal channel and photo, share contact): once sent, the box stays
//! open in `ProfileDialog::Saving` until the request is answered, as
//! tdesktop's boxes do. The answer closes it with its toast; a refusal
//! puts the box back with the reason; closing it stops the wait.
use super::*;
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
        let in_flight = session.requests.get(request).is_some();
        let outcome = session.users_state.saves.take_outcome(request, in_flight);
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
