//! TDLib updates and answers for files, downloads, uploads and the shared-media gallery.
mod parse;

use crate::telegram::envelope::*;
pub(crate) use parse::parse_media_payload;

/// Payloads for files, downloads, uploads and the shared-media gallery; wrapped as
/// [`EnvelopePayload::Media`].
#[derive(Debug, Clone, PartialEq)]
pub enum MediaPayload {
    /// `updateFileDownloads` (schema line 11148): totals of the download list.
    UpdateFileDownloads {
        total_size: i64,
        total_count: i32,
        downloaded_size: i64,
    },
    /// `updateFileAddedToDownloads` (schema line 11151).
    UpdateFileAddedToDownloads(Box<ParsedFileDownload>),
    /// `updateFileRemovedFromDownloads` (schema line 11161).
    UpdateFileRemovedFromDownloads {
        file_id: i32,
    },
    UpdateFile(ParsedFile),
    File(ParsedFile),
    /// Slice media-downloads-pause: `updateFileDownload` — pause state and
    /// completion for a file in the persistent download list (schema
    /// 1.8.67, line 10795). `counts` is not kept (no list-wide UI).
    UpdateFileDownload {
        file_id: i32,
        is_paused: bool,
        complete_date: i32,
    },
}
