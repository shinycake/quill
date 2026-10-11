//! Attachment menu bots in the paperclip menu: which bots a chat shows
//! (tdesktop `PeerMatchesTypes`) and their menu icons.
//!
//! Each bot ships an SVG icon (`default_icon`); the driver downloads it
//! and the entry draws it from the file, falling back to the bot glyph.

use quill::telegram::envelope::{AttachChatKind, AttachmentMenuBot};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Icons larger than this are ignored; real ones are a few kilobytes.
const MAX_ICON_BYTES: u64 = 256 * 1024;

/// One bot row of the attach menu.
#[derive(Clone)]
pub(super) struct AttachMenuEntry {
    pub bot_id: i64,
    pub name: String,
    /// The bot's SVG, when it is downloaded and valid.
    pub icon_svg: Option<Arc<[u8]>>,
}

/// The bots a chat of `kind` offers. `own_bot_chat` is the bot whose
/// private chat is open: its own flag decides there (`SameBot` in
/// tdesktop), the other bots follow the bot-chat flag.
pub(super) fn bots_for_chat(
    bots: &[AttachmentMenuBot],
    kind: AttachChatKind,
    own_bot_chat: Option<i64>,
) -> Vec<&AttachmentMenuBot> {
    bots.iter()
        .filter(|bot| bot.is_added && bot.show_in_attachment_menu)
        .filter(|bot| {
            if kind == AttachChatKind::Bot && own_bot_chat == Some(bot.bot_user_id) {
                bot.supports_self_chat
            } else {
                bot.supports_chat(kind)
            }
        })
        .collect()
}

/// Whether `bytes` look like an SVG document.
pub(super) fn looks_like_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(512)];
    String::from_utf8_lossy(head).contains("<svg")
}

type IconCache = Mutex<HashMap<String, Option<Arc<[u8]>>>>;

fn cache() -> &'static IconCache {
    static CACHE: std::sync::OnceLock<IconCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The SVG at `path`, read once (the menu rebuilds with every render).
pub(super) fn icon_svg(path: &str) -> Option<Arc<[u8]>> {
    let mut cache = cache().lock().ok()?;
    cache
        .entry(path.to_string())
        .or_insert_with(|| {
            let small = std::fs::metadata(path).is_ok_and(|meta| meta.len() <= MAX_ICON_BYTES);
            let bytes = small.then(|| std::fs::read(path).ok()).flatten()?;
            looks_like_svg(&bytes).then(|| Arc::from(bytes))
        })
        .clone()
}

#[cfg(test)]
mod tests {
    // No `use super::*`: the kit's glob brings its own `test` attribute.
    use super::{bots_for_chat, icon_svg, looks_like_svg};
    use quill::telegram::envelope::{AttachChatKind, AttachmentMenuBot};

    fn bot(id: i64) -> AttachmentMenuBot {
        AttachmentMenuBot {
            bot_user_id: id,
            name: format!("Bot {id}"),
            supports_self_chat: false,
            supports_user_chats: false,
            supports_bot_chats: false,
            supports_group_chats: false,
            supports_channel_chats: false,
            request_write_access: false,
            is_added: true,
            show_in_attachment_menu: true,
            show_in_side_menu: false,
            icon: None,
        }
    }

    fn ids(bots: &[&AttachmentMenuBot]) -> Vec<i64> {
        bots.iter().map(|bot| bot.bot_user_id).collect()
    }

    #[test]
    fn chats_show_only_bots_that_support_their_kind() {
        let mut users = bot(1);
        users.supports_user_chats = true;
        let mut groups = bot(2);
        groups.supports_group_chats = true;
        let mut both = bot(3);
        both.supports_user_chats = true;
        both.supports_channel_chats = true;
        let list = [users, groups, both];
        assert_eq!(
            ids(&bots_for_chat(&list, AttachChatKind::User, None)),
            [1, 3]
        );
        assert_eq!(ids(&bots_for_chat(&list, AttachChatKind::Group, None)), [2]);
        assert_eq!(
            ids(&bots_for_chat(&list, AttachChatKind::Channel, None)),
            [3]
        );
        assert!(bots_for_chat(&list, AttachChatKind::SelfChat, None).is_empty());
    }

    #[test]
    fn a_bots_own_chat_uses_its_own_flag() {
        let mut same = bot(1);
        same.supports_self_chat = true;
        let mut other = bot(2);
        other.supports_bot_chats = true;
        let list = [same, other];
        assert_eq!(
            ids(&bots_for_chat(&list, AttachChatKind::Bot, Some(1))),
            [1, 2],
            "the open bot by its own flag, the other by the bot-chat flag"
        );
        assert!(bots_for_chat(&list, AttachChatKind::Bot, Some(2)).is_empty());
        assert_eq!(
            ids(&bots_for_chat(&list, AttachChatKind::Bot, Some(9))),
            [2]
        );
    }

    #[test]
    fn bots_not_added_or_hidden_are_left_out() {
        let mut inactive = bot(1);
        inactive.supports_user_chats = true;
        inactive.is_added = false;
        let mut side_only = bot(2);
        side_only.supports_user_chats = true;
        side_only.show_in_attachment_menu = false;
        assert!(bots_for_chat(&[inactive, side_only], AttachChatKind::User, None).is_empty());
    }

    #[test]
    fn only_svg_files_become_icons() {
        assert!(looks_like_svg(
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"
        ));
        assert!(looks_like_svg(b"  <?xml version=\"1.0\"?><svg/>"));
        assert!(!looks_like_svg(b"\x89PNG\r\n"));
        assert!(!looks_like_svg(b""));
        assert!(icon_svg("/nonexistent/quill-attach-icon.svg").is_none());
    }
}
