//! Parses TDLib objects for files, downloads, uploads and the shared-media gallery.
use crate::telegram::envelope::*;
use serde_json::Value;

/// The media domain's TDLib types; `Ok(None)` leaves
/// `type_name` to the other domains.
pub(crate) fn parse_media_payload(
    type_name: &str,
    value: &Value,
) -> Result<Option<EnvelopePayload>, ParseError> {
    let payload = match type_name {
        "updateFileDownloads" => Ok(EnvelopePayload::Media(MediaPayload::UpdateFileDownloads {
            total_size: int53_or_zero(value.get("total_size")),
            total_count: json_i32(value.get("total_count"), 0),
            downloaded_size: int53_or_zero(value.get("downloaded_size")),
        })),
        "updateFileAddedToDownloads" => parse_file_download(value)
            .map(|download| {
                EnvelopePayload::Media(MediaPayload::UpdateFileAddedToDownloads(Box::new(download)))
            })
            .ok_or(ParseError::MissingField),
        "updateFileRemovedFromDownloads" => Ok(EnvelopePayload::Media(
            MediaPayload::UpdateFileRemovedFromDownloads {
                file_id: json_i32(value.get("file_id"), 0),
            },
        )),
        "updateFile" => Ok(EnvelopePayload::Media(MediaPayload::UpdateFile(
            parse_file(value.get("file"))?,
        ))),
        // Slice media-downloads-pause: `updateFileDownload` (schema 1.8.67,
        // line 10795) — pause state / completion for a listed download.
        "updateFileDownload" => Ok(EnvelopePayload::Media(MediaPayload::UpdateFileDownload {
            file_id: int53_or_zero(value.get("file_id")).sat_i32(),
            is_paused: value
                .get("is_paused")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            complete_date: int53_or_zero(value.get("complete_date")).sat_i32(),
        })),
        "file" => Ok(EnvelopePayload::Media(MediaPayload::File(parse_file(
            Some(value),
        )?))),
        _ => return Ok(None),
    };
    payload.map(Some)
}
