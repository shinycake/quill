//! Failed requests for files, downloads, uploads and the shared-media gallery.
use crate::state::*;

impl Session {
    /// Reacts to a failed media request; called by
    /// [`Session::apply_error`] after the shared handling.
    pub(crate) fn apply_media_error(
        &mut self,
        err: &TdError,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        // A place without a tile keeps the coordinate card.
        if let Some(pending) = pending
            && pending.purpose == RequestPurpose::GetMapThumbnailFile
        {
            self.map_thumbs.failed(pending.id);
        }
        // Slice media-shared-gallery: failed gallery-tab fetch — the
        // tab shows the failed state with Retry, never the spinner
        // or the empty state.
        if let Some(RequestPurpose::GetSharedMedia { tab, generation }) = pending.map(|p| p.purpose)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.shared_media.fail(
                chat_id,
                tab,
                generation,
                call_request_error_line(err, "Could not load shared media"),
            );
        }
        if let Some(RequestPurpose::Media(MediaPurpose::GetSharedMediaMore { tab, generation })) =
            pending.map(|p| p.purpose)
            && let Some(chat_id) = pending.and_then(|p| p.chat_id)
        {
            self.shared_media.fail_more(chat_id, tab, generation);
        }
    }
}
