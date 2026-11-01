//! A file or audio album: its files stacked in one bubble, the caption
//! under the last one (tdesktop `HistoryView::GroupedMedia` with
//! documents).

use super::*;

/// One file or audio album row. `seek_bars` holds each audio message's
/// seek bar, in `messages` order (`None` for a file).
pub(in crate::ui) struct FileAlbum<'a> {
    pub(in crate::ui) album_id: i64,
    pub(in crate::ui) messages: &'a [&'a HistoryMessage],
    pub(in crate::ui) seek_bars: Vec<Option<crate::ui::playback::SeekBarView>>,
    pub(in crate::ui) sender: Option<SenderLabel>,
    pub(in crate::ui) sender_status: Option<AnyElement>,
    pub(in crate::ui) receipt: OutboxReceipt,
    pub(in crate::ui) sender_avatar: Option<SenderAvatar>,
    pub(in crate::ui) look: BubbleLook,
}

pub(in crate::ui) fn file_album_history_row(
    album: FileAlbum,
    files: &quill::state::FileMap,
    downloading: &quill::state::DownloadIntents,
    media_roots: &[PathBuf],
    session: Option<&Session>,
    revealed: &std::collections::HashSet<(i64, u64, u64, bool)>,
    corner: CornerActions,
    cx: &mut Context<QuillApp>,
) -> AnyElement {
    let FileAlbum {
        album_id,
        messages,
        seek_bars,
        sender,
        sender_status,
        receipt,
        sender_avatar,
        look,
    } = album;
    let Some(first) = messages.first() else {
        return div().into_any_element();
    };
    let room = bubble_width::BubbleRoom::new(look.history, sender_avatar.is_some(), look.plain);
    let mut stack = div()
        .id(("file-album", album_id as u64))
        .debug_selector(move || format!("file-album-{album_id}"))
        .flex()
        .flex_col()
        .gap_2();
    for (message, seek_bar) in messages.iter().zip(seek_bars.iter()) {
        let row = match (&message.content, seek_bar) {
            (MessageContent::Document(doc), _) => Some(document_chip(
                message.id.0 as u64,
                doc,
                message.is_outgoing,
                files,
                downloading,
                media_roots,
                session
                    .filter(|s| s.media.downloads.is_user(doc.file_id.0))
                    .map(|s| s.media.downloads.is_paused(doc.file_id.0)),
                DocOrigin::Message(message.chat_id, message.id),
                room,
                cx,
            )),
            (MessageContent::Audio(audio), Some(seek_bar)) => Some(audio_row(
                message.id,
                message.is_outgoing,
                audio,
                files,
                downloading,
                media_roots,
                seek_bar,
                room,
                cx,
            )),
            _ => None,
        };
        let id = message.id.0;
        stack = stack.children(row.map(|row| {
            div()
                .debug_selector(move || format!("file-album-item-{id}"))
                .child(row)
        }));
    }
    let caption = messages.iter().rev().find_map(|message| {
        let (text, entities) = match &message.content {
            MessageContent::Document(doc) => (&doc.caption, &doc.caption_entities),
            MessageContent::Audio(audio) => (&audio.caption, &audio.caption_entities),
            _ => return None,
        };
        (!text.is_empty()).then(|| (message.chat_id, message.id, text.clone(), entities.clone()))
    });
    let emoji: HashMap<i64, ImageSource> = HashMap::new();
    let layered: HashMap<i64, super::sticker_player::StickerView> = HashMap::new();
    let stack = stack.when_some(caption, |this, (chat_id, message_id, text, entities)| {
        this.child(rich_text_reserving(
            &text,
            &entities,
            (chat_id.0, message_id.0 as u64),
            true,
            revealed,
            look.font,
            &emoji,
            &layered,
            None,
            cx,
        ))
    });
    let avatar_link = avatar_link(&sender_avatar, first, cx);
    let mut chrome = message_chrome(sender, receipt, sender_avatar, first.date, first.pending);
    if let Some(link) = avatar_link
        && let Some(avatar) = chrome.avatar.take()
    {
        chrome.avatar = Some(link.wrap(avatar));
    }
    chrome.sender_status = sender_status;
    chrome.actions = corner.mount(|| message_actions_button(first.chat_id, first.id, cx));
    session_bubble_quoted(
        album_id as u64,
        chrome,
        String::new(),
        first.is_outgoing,
        Some(stack.into_any_element()),
        None,
        look,
    )
}
