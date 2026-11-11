//! The "Upload Sound" request: its answer and its failure.
use crate::state::*;
use crate::telegram::envelope::NotificationSound;
use crate::tone_upload::{ToneProblem, ToneUpload, problem_for_server_type};

impl Session {
    /// The saved-sound list changed on the server. A list request already
    /// out was sent before that, so its answer is dropped (it would clear
    /// the mark with older data); the refetch asks again (02-209).
    pub(crate) fn mark_saved_sounds_stale(&mut self) {
        self.requests
            .take_purpose(RequestPurpose::GetSavedNotificationSounds);
        self.settings.saved_sounds_stale = true;
    }

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
        self.mark_saved_sounds_stale();
        let target = match &self.settings.tone_upload {
            ToneUpload::Uploading { target, .. } => Some(*target),
            _ => None,
        };
        self.settings.tone_upload = ToneUpload::Added { sound_id, target };
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
