//! Reply drafts and quote selections.

use super::*;

/// Message the composer is quoting (tdesktop `FieldHeader::replyToMessage`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposerReplyTo {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub preview: String,
    /// Slice G1: partial-message quote (`inputTextQuote`, schema 1.8.67
    /// line 3056) — `text` is a verbatim substring of the original
    /// message and `position` its UTF-16 code-unit offset.
    pub quote: Option<QuoteSelection>,
    /// "Reply in Another Chat": the chat this reply will be sent into when
    /// it is not `chat_id`. `None` for the usual same-chat reply.
    pub target_chat: Option<ChatId>,
}

/// Slice G1: a quoted part of the replied-to message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuoteSelection {
    pub text: String,
    pub position: i32,
}

impl ComposerReplyTo {
    pub fn new(chat_id: ChatId, message_id: MessageId, preview: impl Into<String>) -> Self {
        Self {
            chat_id,
            message_id,
            preview: preview.into(),
            quote: None,
            target_chat: None,
        }
    }

    /// Slice G1: reply carrying a validated partial quote.
    pub fn with_quote(
        chat_id: ChatId,
        message_id: MessageId,
        preview: impl Into<String>,
        quote: QuoteSelection,
    ) -> Self {
        Self {
            chat_id,
            message_id,
            preview: preview.into(),
            quote: Some(quote),
            target_chat: None,
        }
    }

    /// Aim this reply at another chat: the reply stays attached while that
    /// chat is open and goes out as `inputMessageReplyToExternalMessage`.
    pub fn into_chat(mut self, target: ChatId) -> Self {
        self.target_chat = (target != self.chat_id).then_some(target);
        self
    }

    /// The reply belongs in `chat`: it replies there, or was aimed there.
    pub fn belongs_to(&self, chat: ChatId) -> bool {
        self.chat_id == chat || self.target_chat == Some(chat)
    }

    /// Send-pipeline view for a send into `chat`: a same-chat reply, or an
    /// external one when this reply was aimed at `chat`. `None` when the
    /// reply is not for that chat. Drafts keep using [`Self::send_reply`],
    /// which only knows same-chat replies.
    pub fn send_target(&self, chat: ChatId) -> Option<crate::telegram::SendReply> {
        let quote = self
            .quote
            .as_ref()
            .map(|quote| (quote.text.clone(), quote.position));
        if self.chat_id == chat {
            Some(crate::telegram::SendReply {
                message_id: self.message_id,
                quote,
                source_chat: None,
            })
        } else if self.target_chat == Some(chat) {
            Some(crate::telegram::SendReply::external(
                self.chat_id,
                self.message_id,
                quote,
            ))
        } else {
            None
        }
    }

    /// Replace the quote (`None` replies to the whole message).
    pub fn with_new_quote(mut self, quote: Option<QuoteSelection>) -> Self {
        self.quote = quote;
        self
    }

    /// Slice G1: draft/send-pipeline view of this reply — the replied-to
    /// message id plus the optional validated partial quote, mirroring
    /// `telegram::SendReply`. `None` when this reply targets another chat.
    pub fn send_reply(&self, chat_id: ChatId) -> Option<crate::telegram::SendReply> {
        (self.chat_id == chat_id && self.target_chat.is_none()).then(|| {
            crate::telegram::SendReply {
                message_id: self.message_id,
                quote: self
                    .quote
                    .as_ref()
                    .map(|quote| (quote.text.clone(), quote.position)),
                source_chat: None,
            }
        })
    }
}

/// Slice G1: find `quote` in `full_text` and return its UTF-16 code-unit
/// offset, as `inputTextQuote.position` requires (schema 1.8.67, line
/// 3056). `None` when the quote is empty or not a verbatim substring —
/// the quote dialog only submits validated quotes, so this is a guard,
/// not the validation itself.
pub fn quote_position(full_text: &str, quote: &str) -> Option<i32> {
    if quote.is_empty() {
        return None;
    }
    let byte_offset = full_text.find(quote)?;
    Some(full_text[..byte_offset].encode_utf16().count() as i32)
}

/// tdesktop `FieldHeader` Escape / `replyCancelled`: drop the reply header
/// and keep the typed field. `ComposeControls::clear` wipes text on send,
/// not on cancel.
pub fn cancel_reply_draft(
    reply: Option<ComposerReplyTo>,
    text: String,
) -> (Option<ComposerReplyTo>, String) {
    let _ = reply;
    (None, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_reply_keeps_typed_text() {
        let reply = ComposerReplyTo::new(ChatId(11), MessageId(101), "Hello from injected JSON.");
        let (cleared, text) = cancel_reply_draft(Some(reply), "keep this draft".into());
        assert_eq!(cleared, None);
        assert_eq!(text, "keep this draft");
    }

    #[test]
    fn aimed_reply_belongs_to_both_chats_but_drafts_stay_same_chat() {
        let reply = ComposerReplyTo::new(ChatId(12), MessageId(40), "x").into_chat(ChatId(11));
        assert!(reply.belongs_to(ChatId(12)));
        assert!(reply.belongs_to(ChatId(11)));
        assert!(!reply.belongs_to(ChatId(13)));
        assert!(reply.send_target(ChatId(13)).is_none());
        // Drafts only know replies that stay in their own chat.
        assert!(reply.send_reply(ChatId(11)).is_none());
        assert!(reply.send_reply(ChatId(12)).is_none());
        let plain = ComposerReplyTo::new(ChatId(12), MessageId(40), "x");
        assert!(plain.send_reply(ChatId(12)).is_some());
        // Aiming a reply at its own chat is no detour.
        let home = ComposerReplyTo::new(ChatId(12), MessageId(40), "x").into_chat(ChatId(12));
        assert_eq!(home.target_chat, None);
    }

    #[test]
    fn updating_the_quote_keeps_the_target() {
        let reply = ComposerReplyTo::new(ChatId(12), MessageId(40), "x")
            .into_chat(ChatId(11))
            .with_new_quote(Some(QuoteSelection {
                text: "q".into(),
                position: 0,
            }));
        assert_eq!(reply.target_chat, Some(ChatId(11)));
        assert!(reply.quote.is_some());
        assert!(reply.with_new_quote(None).quote.is_none());
    }

    /// Slice G1: `quote_position` — UTF-16 code-unit offsets for
    /// `inputTextQuote.position`.
    #[test]
    fn quote_position_finds_substring_utf16_offset() {
        assert_eq!(quote_position("hello world", "world"), Some(6));
        assert_eq!(quote_position("hello world", "hello"), Some(0));
        // Non-BMP characters count as 2 UTF-16 code units.
        assert_eq!(quote_position("a😀b", "b"), Some(3));
        assert_eq!(quote_position("hello", ""), None);
        assert_eq!(quote_position("hello", "xyz"), None);
        // First occurrence wins.
        assert_eq!(quote_position("aa aa", "aa"), Some(0));
    }
}
