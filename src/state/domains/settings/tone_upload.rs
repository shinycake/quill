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
        self.settings.saved_sounds.outdate();
    }

    /// A `setAccountTtl` or `deleteAccount` is out: one write at a time. A
    /// `getAccountTtl` in flight does not block a write.
    pub fn account_ttl_write_pending(&self) -> bool {
        self.requests.pending.values().any(|pending| {
            matches!(
                pending.purpose,
                RequestPurpose::Settings(SettingsPurpose::SetAccountTtl { .. })
                    | RequestPurpose::DeleteAccount
            )
        })
    }

    /// The saved-sound list is owed a fetch: it changed, and no request
    /// for it is out (`Fetch::Outdated`).
    pub fn saved_sounds_owed(&self) -> bool {
        self.settings.saved_sounds.is_outdated()
    }

    /// `notificationSound`: the new sound joins the front of the list
    /// (`_list.documents.insert(begin)`) until the refetch confirms it.
    pub(super) fn finish_tone_upload(
        &mut self,
        sounds: Vec<NotificationSound>,
        request: Option<RequestId>,
    ) {
        let Some(sound) = sounds.into_iter().next() else {
            if self.tone_upload_is(request) {
                self.settings.tone_upload = ToneUpload::Failed(ToneProblem::Unreadable);
            }
            return;
        };
        self.remember_files(std::slice::from_ref(&sound.sound));
        let sound_id = sound.id;
        self.settings.saved_sounds.insert_data_if_missing(Vec::new);
        if let Some(saved) = self.settings.saved_sounds.data_mut() {
            saved.retain(|saved| saved.id != sound_id);
            saved.insert(0, sound);
        }
        self.mark_saved_sounds_stale();
        // The sound is saved whichever upload this answers; the picker it
        // goes to is known only while this request is the one uploading.
        let target = match &self.settings.tone_upload {
            ToneUpload::Uploading {
                request: waiting,
                target,
                ..
            } if Some(*waiting) == request => Some(*target),
            _ => None,
        };
        self.settings.tone_upload = ToneUpload::Added { sound_id, target };
    }

    /// The upload `request` is the one the session is waiting on.
    fn tone_upload_is(&self, request: Option<RequestId>) -> bool {
        matches!(
            &self.settings.tone_upload,
            ToneUpload::Uploading { request: waiting, .. } if Some(*waiting) == request
        )
    }

    pub(super) fn apply_tone_upload_error(
        &mut self,
        err: &TdError,
        pending: Option<(RequestId, RequestPurpose)>,
    ) -> bool {
        let purpose = pending.map(|(_, purpose)| purpose);
        if purpose == Some(RequestPurpose::RemoveSavedNotificationSound) {
            // The sound is still saved: say so, and read the list again so
            // the row shows what the server has.
            self.chats_state
                .chat_action_error
                .push(crate::lang::tr(crate::lang::keys::Q_TONE_DELETE_FAILED).to_string());
            self.mark_saved_sounds_stale();
            return true;
        }
        if purpose != Some(RequestPurpose::UploadSavedNotificationSound) {
            return false;
        }
        if self.tone_upload_is(pending.map(|(id, _)| id)) {
            self.settings.tone_upload =
                ToneUpload::Failed(problem_for_server_type(err.server_type.as_deref()));
        }
        true
    }
}
