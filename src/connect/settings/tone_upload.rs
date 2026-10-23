//! Connect driver: "Upload Sound" and deleting a saved sound.
use super::*;
use crate::telegram::requests::{
    add_saved_notification_sound_local, remove_saved_notification_sound,
};
use crate::tone_upload::{ToneProblem, ToneUpload, check_file, display_name};

impl<S: JsonSender> ConnectDriver<S> {
    /// tdesktop `Api::Ringtones::upload` after the box's size check:
    /// `addSavedNotificationSound` with the file. The answer lands in
    /// `settings.tone_upload`. One file at a time.
    pub fn upload_notification_tone(
        &mut self,
        path: &std::path::Path,
    ) -> Result<RequestId, ToneProblem> {
        if !self.chats_path_active() {
            return Err(ToneProblem::Unreadable);
        }
        if matches!(
            self.session.settings.tone_upload,
            ToneUpload::Uploading { .. }
        ) {
            return Err(ToneProblem::Unreadable);
        }
        let size = std::fs::metadata(path)
            .map_err(|_| ToneProblem::Unreadable)?
            .len();
        check_file(path, size, self.session.messages.tone_limits)?;
        let path_text = path.to_str().ok_or(ToneProblem::InvalidFile)?;
        let extra = self
            .session
            .request(RequestPurpose::UploadSavedNotificationSound, None);
        let json = add_saved_notification_sound_local(extra, path_text);
        match self.sender.send_json(&json) {
            Ok(()) => {
                self.session.settings.tone_upload = ToneUpload::Uploading {
                    name: display_name(path),
                };
                Ok(extra)
            }
            Err(_) => {
                self.session.requests.take(extra);
                Err(ToneProblem::Unreadable)
            }
        }
    }

    /// The upload's outcome, once, when it has ended.
    pub fn take_tone_upload_outcome(&mut self) -> Option<ToneUpload> {
        if !self.session.settings.tone_upload.is_settled() {
            return None;
        }
        Some(std::mem::take(&mut self.session.settings.tone_upload))
    }

    /// A saved sound's "Delete" (`removeSavedNotificationSound`); the list
    /// refetches once TDLib confirms.
    pub fn remove_notification_tone(
        &mut self,
        sound_id: i64,
    ) -> Result<RequestId, ConnectSendError> {
        if !self.chats_path_active() || sound_id == 0 {
            return Err(ConnectSendError::InvalidRequest);
        }
        let extra = self
            .session
            .request(RequestPurpose::RemoveSavedNotificationSound, None);
        match self
            .sender
            .send_json(&remove_saved_notification_sound(extra, sound_id))
        {
            Ok(()) => Ok(extra),
            Err(err) => {
                self.session.requests.take(extra);
                Err(err)
            }
        }
    }
}
