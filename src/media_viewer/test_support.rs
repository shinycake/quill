//! Test fixtures shared by the media viewer test modules.

use super::*;

pub(super) fn item(kind: MediaViewerKind, message_id: i64) -> MediaViewerItem {
    MediaViewerItem {
        file_name: None,
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
        quality_options: Vec::new(),
    }
}

/// `SearchMessageHit` construction in `media_viewer.rs` (test fixture).
pub(super) fn test_message(chat: i64, id: i64, content: MessageContent) -> HistoryMessage {
    HistoryMessage {
        sender: None,
        id: MessageId(id),
        chat_id: ChatId(chat),
        is_outgoing: false,
        date: 0,
        content,
        pending: false,
        reply_to: None,
        forward_info: None,
        extras: Default::default(),
        interaction_info: None,
        is_pinned: false,
        media_album_id: 0,
        reply_markup: None,
        self_destruct: None,
        auto_delete: None,
        author_signature: None,
        failed: false,
        can_retry: false,
        ephemeral: None,
    }
}
