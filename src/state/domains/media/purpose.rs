//! Request purposes for files, downloads, uploads and the shared-media gallery.
use crate::state::request_purpose::flat_purposes;
use crate::state::*;

/// In-flight requests for files, downloads, uploads and the shared-media gallery; wrapped as
/// [`RequestPurpose::Media`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaPurpose {
    DownloadFile,
    /// MED3: `cancelDownloadFile`. Response is `Ok`.
    CancelDownloadFile,
    /// Slice media-downloads-pause: `toggleDownloadIsPaused`. Response is
    /// `Ok`; the pause state itself arrives on `updateFileDownload`.
    ToggleDownloadIsPaused,
    /// The next older `searchChatMessages` page for a gallery tab, asked
    /// for while the media viewer pages toward the end of the list.
    GetSharedMediaMore {
        tab: SharedMediaTab,
        generation: u64,
    },
    /// "Cancel Upload": `deleteMessages` on a message still being sent.
    CancelUpload,
    /// `getMapThumbnailFile` for a location or venue message. Response is
    /// `file`; the tile's file id is kept per place (`Session::map_thumbs`).
    GetMapThumbnailFile,
}

flat_purposes!(Media(MediaPurpose) {
    DownloadFile,
    CancelDownloadFile,
    ToggleDownloadIsPaused,
    CancelUpload,
    GetMapThumbnailFile,
});
