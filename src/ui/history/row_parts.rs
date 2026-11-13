//! Parts every message bubble shares, single message or album: the
//! headers above it (via bot, forwarded from, reply quote), the time line
//! and the comments or replies bar under it.

use super::*;
use quill::lang::{keys, tr_with};

/// The headers that open the bubble of `message`: "via @bot", "Forwarded
/// from" and the reply quote, top to bottom; `None` without any.
#[allow(clippy::too_many_arguments)]
pub(in crate::ui) fn row_header(
    message: &HistoryMessage,
    reply_header: Option<quill::state::ReplyHeader>,
    forward_header: Option<quill::state::ForwardHeader>,
    via_bot: Option<String>,
    on_fill: bool,
    session: Option<&Session>,
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    let quote = reply_header.map(|header| {
        let thumb = QuillApp::reply_thumb_path(&header, files, media_roots);
        let pattern = QuillApp::reply_pattern_path(&header, session, files, media_roots);
        reply_header_strip(message.id, header, thumb, pattern, on_fill, cx)
    });
    let has_forward = forward_header.is_some();
    let forward_strip = forward_header.map(|header| {
        let original = (header.original_date > 0 && message.forward_info.is_some()).then(|| {
            tr_with(
                keys::Q_CHAT_ORIGINAL,
                &[(
                    "into",
                    &(super::super::message_text::format_unix_date_time(
                        header.original_date.into(),
                    ))
                    .to_string(),
                )],
            )
        });
        forward_header_line(message.id, header, via_bot.clone(), original, on_fill, cx)
    });
    let via_strip = via_bot
        .filter(|_| !has_forward)
        .map(|bot| via_bot_line(message.id, bot, on_fill));
    let header_parts: Vec<AnyElement> = [via_strip, forward_strip, quote]
        .into_iter()
        .flatten()
        .collect();
    let id = message.id;
    (!header_parts.is_empty()).then(|| {
        div()
            .id(("row-headers", id.0 as u64))
            .debug_selector(move || format!("row-headers-{}", id.0))
            .flex()
            .flex_col()
            .children(header_parts)
            .into_any_element()
    })
}

/// The time line of `message`: channel views and the author signature
/// ride with the time ("Demo Admin · 👁 12.4K · 16:44"), with the
/// message's effect badge and its forward drag.
pub(super) fn row_footer_meta(
    message: &HistoryMessage,
    receipt: OutboxReceiptView,
    session: Option<&Session>,
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
) -> FooterMeta {
    let views = message
        .interaction_info
        .as_ref()
        .map(|info| info.view_count)
        .filter(|&count| count > 0);
    // A forwarded post names its author in the forward header instead.
    let signature = message
        .author_signature
        .clone()
        .filter(|_| message.forward_info.is_none());
    FooterMeta::of(message, receipt, views, signature)
        .with_effect(super::super::message_effect_badge::footer_effect(
            message,
            session,
            files,
            media_roots,
        ))
        .with_forward_drag(crate::ui::forward_drag_source::drag_for_message(
            session, message,
        ))
}

/// Whether `chat_id` is a channel (not a supergroup).
pub(super) fn is_channel(session: Option<&Session>, chat_id: ChatId) -> bool {
    session
        .and_then(|s| s.chats.get(&chat_id.0))
        .is_some_and(|chat| {
            matches!(
                chat.kind,
                quill::telegram::envelope::ChatKind::Supergroup {
                    is_channel: true,
                    ..
                }
            )
        })
}

/// The bar under `message`: a channel post opens its comments, a group
/// message its replies (tdesktop `paintCommentsButton`). Inside the open
/// thread itself the bar would only point back at the view.
pub(super) fn row_reply_bar(
    message: &HistoryMessage,
    session: Option<&Session>,
    media_roots: &[PathBuf],
    cx: &mut Context<QuillApp>,
) -> Option<AnyElement> {
    let info = message
        .interaction_info
        .as_ref()
        .and_then(|info| info.reply_info.as_ref())?;
    let in_thread = session.is_some_and(|s| s.thread_for_chat(message.chat_id).is_some());
    if in_thread || message.id.0 <= 0 || message.pending() {
        return None;
    }
    let in_group = session
        .and_then(|s| s.chats.get(&message.chat_id.0))
        .is_some_and(|chat| {
            matches!(
                chat.kind,
                quill::telegram::envelope::ChatKind::BasicGroup { .. }
                    | quill::telegram::envelope::ChatKind::Supergroup {
                        is_channel: false,
                        ..
                    }
            )
        });
    let kind = if is_channel(session, message.chat_id) {
        super::super::threads::ReplyBarKind::Comments
    } else if in_group && info.reply_count > 0 {
        super::super::threads::ReplyBarKind::Replies
    } else {
        return None;
    };
    Some(super::super::threads::reply_bar(
        message.chat_id,
        message.id,
        kind,
        info,
        message.is_outgoing,
        super::super::threads::replier_avatars(info, session, media_roots),
        cx,
    ))
}
