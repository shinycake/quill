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
    /// `searchFileDownloads` for the active downloads, to resume the ones a
    /// low or full disk paused in an earlier TDLib instance. Response is
    /// `foundFileDownloads`.
    SearchDiskPausedDownloads,
    /// The next older `searchChatMessages` page for a gallery tab, asked
    /// for while the media viewer pages toward the end of the list.
    GetSharedMediaMore {
        tab: SharedMediaTab,
        generation: u64,
    },
    /// The gallery's Gifts tab: `getReceivedGifts` for the open chat's
    /// user or channel. Response is `receivedGifts`.
    GetProfileGifts {
        /// The tab's stamp when the page was asked for: an answer for
        /// another stamp is from a gallery that was closed since.
        generation: u64,
        append: bool,
    },
    /// The gallery's Playlist tab: `getUserProfileAudios`. Response is
    /// `audios`.
    GetProfileAudios {
        append: bool,
    },
    /// "Save to... Profile" on a song: `addProfileAudio`. Response is `Ok`.
    AddProfileAudio {
        file_id: i32,
    },
    /// `isProfileAudio` for a song's menu: `Ok` means it is in the current
    /// user's saved music, a 404 error means it is not.
    IsProfileAudio {
        file_id: i32,
    },
    /// Removing a track from the Playlist page: `removeProfileAudio`.
    RemoveProfileAudio {
        file_id: i32,
    },
    /// Dragging a track on the Playlist page: `setProfileAudioPosition`.
    SetProfileAudioPosition {
        file_id: i32,
    },
    /// "Cancel Upload": `deleteMessages` on a message still being sent.
    CancelUpload,
    /// `getMapThumbnailFile` for a location or venue message. Response is
    /// `file`; the tile's file id is kept per place (`Session::map_thumbs`).
    GetMapThumbnailFile,
    /// Send Location's map picker: its venue bot, venues, tiles and icons.
    LocationPicker(crate::location_picker::LocationPickerPurpose),
}

flat_purposes!(Media(MediaPurpose) {
    DownloadFile,
    CancelDownloadFile,
    ToggleDownloadIsPaused,
    SearchDiskPausedDownloads,
    CancelUpload,
    GetMapThumbnailFile,
});
