//! The "Upload Sound" request: its answer and its failure.
use crate::state::*;
use crate::telegram::envelope::NotificationSound;
use crate::tone_upload::{ToneProblem, ToneUpload, problem_for_server_type};

impl Session {
    /// `notificationSound`: the new sound joins the front of the list
    /// (`_list.documents.insert(begin)`) until the refetch confirms it.
    pub(super) fn finish_tone_upload(&mut self, sounds: Vec<NotificationSound>) {
        let Some(sound) = sounds.into_iter().next() else {
            self.settings.tone_upload = ToneUpload::Failed(ToneProblem::Unreadable);
            return;
        };
        self.remember_files(std::slice::from_ref(&sound.sound));
        let sound_id = sound.id;
        self.settings
            .saved_notification_sounds
            .retain(|saved| saved.id != sound_id);
        self.settings.saved_notification_sounds.insert(0, sound);
        self.settings.saved_sounds_stale = true;
        self.settings.tone_upload = ToneUpload::Added { sound_id };
    }

    pub(super) fn apply_tone_upload_error(
        &mut self,
        err: &TdError,
        purpose: Option<RequestPurpose>,
    ) -> bool {
        if purpose != Some(RequestPurpose::UploadSavedNotificationSound) {
            return false;
        }
        self.settings.tone_upload =
            ToneUpload::Failed(problem_for_server_type(err.server_type.as_deref()));
        true
    }
}
