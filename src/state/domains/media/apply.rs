//! Applies TDLib updates and answers for files, downloads, uploads and the shared-media gallery.
use crate::state::*;
use crate::telegram::envelope::MediaPayload;

impl Session {
    /// Applies one media payload; called by
    /// [`Session::apply_payload`].
    pub(crate) fn apply_media_payload(
        &mut self,
        payload: MediaPayload,
        pending: Option<&PendingRequest>,
        _extra: Option<RequestId>,
        _seq: u64,
    ) {
        match payload {
            MediaPayload::UpdateFileDownloads {
                total_size,
                total_count,
                downloaded_size,
            } => self.sync.set_download_totals(DownloadTotals {
                total_size,
                total_count,
                downloaded_size,
            }),
            MediaPayload::UpdateFileAddedToDownloads(download) => {
                self.apply_download_added(*download)
            }
            MediaPayload::UpdateFileRemovedFromDownloads { file_id } => {
                self.apply_download_removed(file_id)
            }
            MediaPayload::UpdateFile(file) | MediaPayload::File(file) => {
                if let Some(pending) = pending
                    && pending.purpose == RequestPurpose::GetMapThumbnailFile
                {
                    self.media.map_thumbs.answered(pending.id, file.id.0);
                }
                let paths = self.media.files.paths_revision();
                self.upsert_file(file, true);
                if self.media.files.paths_revision() == paths {
                    self.media.files.note_quiet_update();
                }
            }
            MediaPayload::UpdateFileDownload {
                file_id,
                is_paused,
                complete_date,
            } => {
                // Slice media-downloads-pause: the list API's pause/completion
                // channel. Pause state is tracked only for user-initiated
                // (listed) downloads; completion mirrors the `updateFile`
                // path (recent list + unstick).
                // The idle file update can precede the list's pause event.
                // A later authoritative list update restores that transfer.
                if (is_paused || complete_date != 0) && self.media.downloads.is_failed(file_id) {
                    self.media.downloads.adopt(file_id, false);
                }
                if complete_date != 0 {
                    self.record_completed_user_download(file_id);
                    self.unstick_download(file_id);
                } else if self.media.downloads.is_user(file_id) {
                    self.media.downloads.set_paused(file_id, is_paused);
                }
            }
            MediaPayload::ProfileAudios(page) => self.apply_profile_audios(page, pending),
        }
    }
}
