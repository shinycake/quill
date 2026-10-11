//! Methods moved out of `history.rs` to keep files under 1000 lines.

use super::*;

/// A custom-emoji reaction's image inside a chip; its fallback emoji (or a
/// blank disc) until the sticker resolves and downloads.
pub(super) fn custom_emoji_chip_glyph(
    id: i64,
    session: Option<&Session>,
    files: &HashMap<i32, ParsedFile>,
    media_roots: &[PathBuf],
) -> AnyElement {
    let size = super::super::reactions::CHIP_GLYPH;
    let sticker = session.and_then(|s| {
        s.stickers
            .emoji
            .custom_emoji_stickers
            .iter()
            .find(|item| item.custom_emoji_id == Some(id))
    });
    let path = sticker
        .and_then(|item| item.display_file_id())
        .and_then(|file| files.get(&file.0))
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots));
    match (path, sticker) {
        (Some(path), _) => img(path)
            .size(px(size))
            .aspect_square()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        (None, Some(item)) if !item.emoji.is_empty() => {
            div().child(item.emoji.clone()).into_any_element()
        }
        _ => div()
            .size(px(size))
            .rounded_full()
            .bg(text_muted().opacity(0.3))
            .into_any_element(),
    }
}

/// A reactor's avatar: name and (sandboxed) photo for a user or chat.
pub(in crate::ui) fn reactor_avatar(
    sender: &quill::telegram::envelope::MessageSender,
    session: Option<&Session>,
    media_roots: &[PathBuf],
) -> (String, Option<PathBuf>) {
    use quill::telegram::envelope::MessageSender;
    let Some(session) = session else {
        return (String::new(), None);
    };
    let (name, photo) = match sender {
        MessageSender::User { user_id } => (
            session
                .user(*user_id)
                .map(|u| u.display_name())
                .unwrap_or_default(),
            session.user_photo_path(*user_id),
        ),
        MessageSender::Chat { chat_id } => (
            session
                .chats
                .get(chat_id)
                .map(|c| c.title.clone())
                .unwrap_or_default(),
            session.chat_photo_path(ChatId(*chat_id)),
        ),
    };
    let photo = photo.and_then(|path| sandboxed_display_path(path, media_roots));
    (name, photo)
}
