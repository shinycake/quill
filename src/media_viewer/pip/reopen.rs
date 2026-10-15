//! Where "back to the viewer" lands (tdesktop `closeAndContinue`, which
//! reopens the viewer on the clip's document in its message).

use crate::ids::{FileId, MessageId};
use crate::media_viewer::MediaViewerItem;

/// The clip a Picture-in-Picture window plays: its message, its place in
/// the message (a bought paid-media album holds several) and its file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipRef {
    pub message_id: MessageId,
    pub offset: usize,
    pub file_id: Option<FileId>,
}

impl ClipRef {
    /// Whether `item` is this clip: the same message and file.
    pub fn is(&self, item: &MediaViewerItem) -> bool {
        item.message_id == self.message_id && item.play_file_id == self.file_id
    }
}

/// The index among `items` to reopen the viewer on: the item at the
/// clip's offset in its message when it is still the same file, else the
/// item of that message holding the file, else the file anywhere. `None`
/// when the list no longer has it.
pub fn reopen_index(items: &[MediaViewerItem], clip: &ClipRef) -> Option<usize> {
    let first = items
        .iter()
        .position(|item| item.message_id == clip.message_id);
    let at_offset = first
        .and_then(|first| first.checked_add(clip.offset))
        .filter(|index| items.get(*index).is_some_and(|item| clip.is(item)));
    at_offset
        .or_else(|| items.iter().position(|item| clip.is(item)))
        .or_else(|| {
            clip.file_id.and_then(|file| {
                items
                    .iter()
                    .position(|item| item.play_file_id == Some(file))
            })
        })
}

#[cfg(test)]
mod tests {
    use super::{ClipRef, reopen_index};
    use crate::ids::{ChatId, FileId, MessageId};
    use crate::media_viewer::{MediaViewerItem, MediaViewerKind};

    fn item(message: i64, file: i32) -> MediaViewerItem {
        MediaViewerItem {
            chat_id: ChatId(1),
            message_id: MessageId(message),
            kind: MediaViewerKind::Video,
            display_file_ids: Vec::new(),
            download_file_id: FileId(file),
            play_file_id: Some(FileId(file)),
            duration_secs: None,
            mime_type: None,
            start_timestamp: None,
            caption: String::new(),
            caption_entities: Vec::new(),
            duration_label: None,
            natural_size: None,
            quality_options: Vec::new(),
            file_name: None,
        }
    }

    fn clip(message: i64, offset: usize, file: i32) -> ClipRef {
        ClipRef {
            message_id: MessageId(message),
            offset,
            file_id: Some(FileId(file)),
        }
    }

    #[test]
    fn the_album_item_at_its_offset_comes_back() {
        let items = [item(5, 50), item(7, 70), item(7, 71), item(9, 90)];
        assert_eq!(reopen_index(&items, &clip(7, 1, 71)), Some(2));
        assert_eq!(reopen_index(&items, &clip(7, 0, 70)), Some(1));
        assert_eq!(reopen_index(&items, &clip(9, 0, 90)), Some(3));
    }

    #[test]
    fn a_changed_album_is_matched_by_file() {
        // The album lost its first item: offset 1 is now another file.
        let items = [item(7, 71), item(7, 72)];
        assert_eq!(reopen_index(&items, &clip(7, 1, 71)), Some(0));
        // The file moved to another message (a resent copy).
        let moved = [item(8, 71)];
        assert_eq!(reopen_index(&moved, &clip(7, 0, 71)), Some(0));
        // Gone from the list.
        assert_eq!(reopen_index(&[item(8, 80)], &clip(7, 0, 71)), None);
        assert_eq!(reopen_index(&[], &clip(7, 0, 71)), None);
    }
}
