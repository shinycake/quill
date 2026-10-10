//! Test fixtures shared by the media viewer test modules.

use super::*;

pub(super) fn item(kind: MediaViewerKind, message_id: i64) -> MediaViewerItem {
    MediaViewerItem {
        chat_id: ChatId(7),
        message_id: MessageId(message_id),
        kind,
        display_file_ids: vec![FileId(1)],
        download_file_id: FileId(1),
        play_file_id: None,
        duration_secs: None,
        mime_type: None,
        start_timestamp: None,
        caption: String::new(),
        caption_entities: Vec::new(),
        duration_label: None,
        natural_size: None,
    }
}
