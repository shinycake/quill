//! Generated-once classification of every `MediaPurpose` variant.
use super::RequestKind;
use crate::state::MediaPurpose;

impl MediaPurpose {
    /// What this request is. Exhaustive: a new variant must be classified here.
    pub(crate) fn kind(&self) -> RequestKind {
        match self {
            Self::LocationPicker(inner) => inner.kind(),
            Self::DownloadFile
            | Self::SearchDiskPausedDownloads
            | Self::GetSharedMediaMore { .. }
            | Self::GetProfileGifts { .. }
            | Self::GetProfileAudios { .. }
            | Self::IsProfileAudio { .. }
            | Self::GetMapThumbnailFile => RequestKind::Lookup,
            Self::CancelDownloadFile
            | Self::ToggleDownloadIsPaused
            | Self::AddProfileAudio { .. }
            | Self::RemoveProfileAudio { .. }
            | Self::SetProfileAudioPosition { .. }
            | Self::CancelUpload => RequestKind::UserAction,
        }
    }
}
