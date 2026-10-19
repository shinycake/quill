//! The round fast share button beside channel posts (tdesktop
//! `Message::displayFastShare` / `displayGoToOriginal`).
#![deny(clippy::indexing_slicing)]

use super::{QuillApp, corner_button};
use gpui_kit::component::button::Button;
use gpui_kit::*;
use quill::corner_buttons::{FastShare, FastShareChat, FastShareFacts};
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};
use quill::state::{HistoryMessage, Session};
use quill::telegram::envelope::{ChatKind, MessageContent, MessageOrigin, effective_content};

/// The "Replies" system user (tdesktop `isRepliesChat`).
/// The "Verification Codes" system user (tdesktop `isVerifyCodes`).
const VERIFY_CODES_USER_ID: i64 = 489_000;

/// What the message and its chat say about the fast share button; without a
/// session nothing is known and no button shows.
pub(super) fn facts(message: &HistoryMessage, session: Option<&Session>) -> Option<FastShareFacts> {
    let session = session?;
    let kind = session
        .chats
        .get(&message.chat_id.0)
        .map(|chat| &chat.kind)?;
    let chat = match kind {
        ChatKind::Supergroup {
            is_channel: true, ..
        } => FastShareChat::Channel,
        ChatKind::Private { user_id } => FastShareChat::Private {
            bot: session.is_bot_user(user_id.0),
        },
        _ => FastShareChat::Other,
    };
    let media = message.media_album_id != 0
        || match effective_content(&message.content, message.ephemeral.as_ref()) {
            MessageContent::Text(text) => text.link_preview.is_some(),
            MessageContent::Photo(_)
            | MessageContent::Document(_)
            | MessageContent::Animation(_)
            | MessageContent::Video(_)
            | MessageContent::VideoNote(_)
            | MessageContent::VoiceNote(_)
            | MessageContent::Audio(_)
            | MessageContent::Game(_) => true,
            _ => false,
        };
    // tdesktop `showForwardsFromSender`: chats that show who forwarded.
    let shows_forwarder = match kind {
        ChatKind::Private { user_id } => {
            session.is_saved_messages(message.chat_id)
                || session.is_replies_chat(message.chat_id)
                || user_id.0 == VERIFY_CODES_USER_ID
        }
        _ => false,
    };
    let properties_forwardable = session
        .messages
        .message_menu_actions
        .filter(|(chat, id, _)| *chat == message.chat_id && *id == message.id)
        .is_none_or(|(_, _, actions)| actions.can_be_forwarded);
    Some(FastShareFacts {
        chat,
        sent: !message.pending && message.id.0 > 0,
        outgoing: message.is_outgoing,
        forwardable: !session.chat_has_protected_content(message.chat_id)
            && message.ephemeral.is_none()
            && properties_forwardable,
        forwarded_from_channel: !shows_forwarder
            && message
                .forward_info
                .as_ref()
                .is_some_and(|info| matches!(info.origin, MessageOrigin::Channel { .. })),
        reply_thread: session
            .thread_for_chat(message.chat_id)
            .is_some_and(|thread| thread.forum_topic_id.is_none()),
        forward_source: message.forward_info.as_ref().and_then(|info| info.source),
        media,
    })
}

/// The button for `what`: forward icon for the picker, an arrow out for the
/// jump to the original.
pub(super) fn button(
    chat_id: ChatId,
    message_id: MessageId,
    pending: bool,
    what: FastShare,
    cx: &mut Context<QuillApp>,
) -> Button {
    match what {
        FastShare::Share => corner_button(
            format!("message-fast-share-{}", message_id.0),
            gpui_kit::assets::IconName::Forward,
            tr(keys::Q_CHAT_FORWARD),
            cx,
        )
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.begin_forward_one(chat_id, message_id, pending, window, cx);
        })),
        FastShare::GoToOriginal(source_chat, source_message) => corner_button(
            format!("message-go-to-original-{}", message_id.0),
            gpui_kit::assets::IconName::ArrowUpRight,
            tr(keys::Q_CHAT_GO_TO_MESSAGE),
            cx,
        )
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.go_to_message_in(source_chat, source_message, window, cx);
        })),
    }
}
