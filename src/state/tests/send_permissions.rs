use super::common::*;
use super::*;
use serde_json::json;

#[test]
fn sticker_gif_denials_surface_for_requests_and_async_failures_without_raw_text() {
    let (mut session, sink) = session();
    let seq = AtomicU64::new(0);
    for (message, notice) in [
        (
            "Not enough rights to send stickers to the chat",
            "You don't have permission to send stickers in this chat.",
        ),
        (
            "CHAT_SEND_STICKERS_FORBIDDEN",
            "You don't have permission to send stickers in this chat.",
        ),
        (
            "Not enough rights to send animations to the chat",
            "You don't have permission to send GIFs in this chat.",
        ),
        (
            "CHAT_SEND_GIFS_FORBIDDEN",
            "You don't have permission to send GIFs in this chat.",
        ),
    ] {
        for purpose in [
            RequestPurpose::SendMessage,
            RequestPurpose::ForwardMessages,
            RequestPurpose::SendInlineQueryResult,
            RequestPurpose::ResendMessages,
        ] {
            let extra = session.request(purpose, Some(ChatId(1)));
            apply_json(
                &mut session,
                &seq,
                &sink,
                &json!({"@type":"error","@extra":extra.as_extra(),"code":400,"message":message})
                    .to_string(),
            );
            assert_eq!(
                session.messages.send_permission_error.take().as_deref(),
                Some(notice)
            );
            assert!(session.requests.get(extra).is_none());
        }
        apply_json(&mut session,&seq,&sink,&json!({"@type":"updateMessageSendFailed","old_message_id":-1,"message":{"id":88,"chat_id":1,"is_outgoing":true,"sending_state":{"@type":"messageSendingStateFailed","can_retry":false},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"send failed","entities":[]}}},"error":{"code":403,"message":message}}).to_string());
        assert_eq!(
            session.messages.send_permission_error.take().as_deref(),
            Some(notice)
        );
        let row = &session.histories[&1].messages[&88];
        assert!(row.failed);
        assert!(!row.can_retry);
    }
    let extra = session.request(RequestPurpose::SendMessage, Some(ChatId(1)));
    apply_json(
        &mut session,
        &seq,
        &sink,
        &json!({"@type":"error","@extra":extra.as_extra(),"code":400,"message":"private raw body"})
            .to_string(),
    );
    assert!(session.messages.send_permission_error.is_none());
    let extra = session.request(RequestPurpose::GetStickerSet, None);
    apply_json(&mut session,&seq,&sink,&json!({"@type":"error","@extra":extra.as_extra(),"code":403,"message":"CHAT_SEND_STICKERS_FORBIDDEN"}).to_string());
    assert!(session.messages.send_permission_error.is_none()); // Fetch errors aren't send permission notices.
}
