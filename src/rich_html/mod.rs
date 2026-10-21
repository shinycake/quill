//! "Save as HTML" for one selected rich message
//! (`parity:menu-save-selection-html`), after tdesktop's
//! `Iv::RichMessageHtmlExport` (`iv/iv_rich_message_html_export.cpp`).
//!
//! The export writes `<base>/<title>/<title>.html` with the message's page
//! blocks, and the photos, videos, audio and inline images it shows in a
//! `media/` folder next to it, linked by relative paths. The HTML uses
//! tdesktop's tags and classes (`figure.media`, `tg-spoiler`,
//! `tg-collage`, `pre.code`, ...) with a stylesheet written for Quill.
//!
//! The page is rendered from TDLib's own `richMessage` JSON
//! (`schema/td_api.tl` `richMessage`, `pageBlock*`, `richText*`), not from
//! [`crate::rich::RichBlock`], which keeps only what the chat bubble draws.
//! Everything runs on a worker thread ([`handle`]) that talks to TDLib
//! through the connect driver like the chat export does.
#![deny(clippy::indexing_slicing)]

mod blocks;
pub mod handle;
mod media;
mod page;
mod style;
mod text;
pub mod worker;

#[cfg(test)]
mod tests;

pub use handle::{RichHtmlExport, RichHtmlOutcome, RichHtmlRequest};
pub use page::{PageLabels, page_html, title_from_page};

use crate::ids::{ChatId, MessageId};
use crate::state::Session;
use crate::telegram::envelope::{MessageContent, effective_content};

/// The message "Save as HTML" applies to, when the selection allows it:
/// exactly one sent rich message, in a chat whose content may be saved
/// (tdesktop `AddSaveRichMessageHtmlAction`: one selected item with a rich
/// page, no copy restriction, `!forbidsForward()`).
pub fn selection_target(
    session: &Session,
    chat_id: ChatId,
    selected: &[MessageId],
) -> Option<MessageId> {
    let [message_id] = selected else {
        return None;
    };
    if session.chat_has_protected_content(chat_id) {
        return None;
    }
    let message = session
        .histories
        .get(&chat_id.0)?
        .messages
        .get(&message_id.0)?;
    let sent = !message.pending && !message.failed && message.id.0 > 0;
    let rich = matches!(
        effective_content(&message.content, message.ephemeral.as_ref()),
        MessageContent::RichMessage(_)
    );
    // Per-message protection (`messageProperties.can_be_saved`) is known
    // once the message's menu asked for it; the worker checks it again on
    // the message itself, so a selection made without the menu is safe too.
    let saveable = session
        .messages
        .message_menu_actions
        .as_ref()
        .filter(|(chat, message, _)| *chat == chat_id && message == message_id)
        .is_none_or(|(_, _, actions)| actions.can_be_saved);
    (sent && rich && saveable && message.self_destruct.is_none()).then_some(*message_id)
}
