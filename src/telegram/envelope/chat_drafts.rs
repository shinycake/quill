use super::*;
use crate::ids::MessageId;
use serde_json::Value;

/// What a draft holds: composer markup, or a rich message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftBody {
    /// Composer markup (empty when the draft only replies or suggests).
    Text(String),
    /// `draftMessageContentRichMessage`: tdesktop shows it in place of the
    /// message field (`RichDraftPreview`).
    Rich(crate::rich::RichDraft),
}

/// The message a draft replies to: `inputMessageReplyToMessage`, same chat
/// only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftReply {
    pub message_id: MessageId,
    /// Slice G1: partial-message quote carried by the draft's `reply_to`
    /// (`inputTextQuote`, schema 1.8.67 line 3056): text, formatting and
    /// `position` in UTF-16 code units.
    pub quote: Option<crate::composer::QuoteSelection>,
    /// The checklist task or poll option the draft replies to.
    pub item: crate::composer::ReplyItem,
}

/// `draftMessage` text or rich message this slice restores. Voice and
/// video note drafts are ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatDraft {
    pub body: DraftBody,
    pub reply: Option<DraftReply>,
    /// `draftMessage.suggested_post_info`: the suggest bar's terms in a
    /// channel's direct messages.
    pub suggested_post: Option<crate::suggested_posts::SuggestTerms>,
}

/// `updateChatAction` payloads. Kinds Quill does not label (such as
/// `chatActionWatchingAnimations`) stay `Other`, which clears the sender's
/// previous action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatAction {
    /// `chatActionTyping`
    Typing,
    /// `chatActionRecordingVideo`
    RecordingVideo,
    /// `chatActionUploadingVideo`
    UploadingVideo,
    /// `chatActionRecordingVoiceNote`
    RecordingVoice,
    /// `chatActionUploadingVoiceNote`
    UploadingVoice,
    /// `chatActionUploadingPhoto`
    UploadingPhoto,
    /// `chatActionUploadingDocument`
    UploadingDocument,
    /// `chatActionChoosingSticker` (TDLib 1.8.67, line 6380).
    ChoosingSticker,
    /// `chatActionChoosingLocation`
    ChoosingLocation,
    /// `chatActionChoosingContact`
    ChoosingContact,
    /// `chatActionStartPlayingGame`
    PlayingGame,
    /// `chatActionRecordingVideoNote`
    RecordingRound,
    /// `chatActionUploadingVideoNote`
    UploadingRound,
    /// `chatActionCancel`, or a null action (schema: null cancels).
    Cancel,
    Other,
}

impl ChatDraft {
    /// The draft `content` makes, replying to `reply_to` (what the driver
    /// stores after `setChatDraftMessage`).
    pub fn of(
        content: &crate::composer::DraftContent,
        reply_to: Option<&crate::telegram::requests::SendReply>,
        suggested_post: Option<crate::suggested_posts::SuggestTerms>,
    ) -> Self {
        Self {
            body: match content.rich() {
                Some(rich) => DraftBody::Rich(rich.clone()),
                None => DraftBody::Text(content.text().to_string()),
            },
            reply: reply_to.map(|reply| DraftReply {
                message_id: reply.message_id,
                quote: reply.quote.clone(),
                item: reply.item.clone(),
            }),
            suggested_post,
        }
    }

    /// A plain-text draft with no reply.
    pub fn text_only(text: impl Into<String>) -> Self {
        Self {
            body: DraftBody::Text(text.into()),
            reply: None,
            suggested_post: None,
        }
    }

    /// Composer markup; empty for a rich draft.
    pub fn text(&self) -> &str {
        match &self.body {
            DraftBody::Text(text) => text,
            DraftBody::Rich(_) => "",
        }
    }

    pub fn rich(&self) -> Option<&crate::rich::RichDraft> {
        match &self.body {
            DraftBody::Rich(rich) => Some(rich),
            DraftBody::Text(_) => None,
        }
    }

    pub fn reply_to_message_id(&self) -> Option<MessageId> {
        self.reply.as_ref().map(|reply| reply.message_id)
    }

    pub fn quote(&self) -> Option<&crate::composer::QuoteSelection> {
        self.reply.as_ref().and_then(|reply| reply.quote.as_ref())
    }

    /// The checklist task or poll option replied to (the message itself
    /// without one).
    pub fn item(&self) -> crate::composer::ReplyItem {
        self.reply
            .as_ref()
            .map(|reply| reply.item.clone())
            .unwrap_or_default()
    }

    /// The draft's text for a chat list row, one line per block of a rich
    /// draft (tdesktop `richMessageSummary`).
    pub fn preview_text(&self) -> String {
        match &self.body {
            DraftBody::Rich(rich) => rich.summary(),
            DraftBody::Text(text) => crate::composer::parse_format_markup(text).0,
        }
    }
}

/// `draftMessage` with `draftMessageContentText` or
/// `draftMessageContentRichMessage`. Voice and video note drafts are not
/// restored (Unigram only fills the field from text).
pub(crate) fn parse_chat_draft(value: Option<&Value>) -> Option<ChatDraft> {
    let value = value.filter(|v| !v.is_null())?;
    if value.get("@type").and_then(Value::as_str) != Some("draftMessage") {
        return None;
    }
    let content = value.get("content")?;
    let body = match content.get("@type").and_then(Value::as_str) {
        Some("draftMessageContentText") => {
            // Formatting comes back as composer markup, so a draft synced
            // from another client (or echoed after our own save) keeps it.
            let formatted = content.get("text");
            let plain = formatted
                .and_then(|formatted| formatted.get("text"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let text = crate::composer::entities_to_markup(
                plain,
                &super::message_content::parse_text_entities(plain, formatted),
            );
            DraftBody::Text(text)
        }
        Some("draftMessageContentRichMessage") => {
            let (blocks, _) =
                crate::rich::parse_rich_message(content.get("message").unwrap_or(&Value::Null));
            match crate::rich::RichDraft::from_message(&blocks) {
                Some(rich) => DraftBody::Rich(rich),
                None => DraftBody::Text(String::new()),
            }
        }
        _ => return None,
    };
    let reply = match value.get("reply_to") {
        Some(reply)
            if reply.get("@type").and_then(Value::as_str) == Some("inputMessageReplyToMessage") =>
        {
            let id = int53(reply.get("message_id")).ok().map(MessageId);
            let quote = reply.get("quote").and_then(|quote| {
                if quote.get("@type").and_then(Value::as_str) != Some("inputTextQuote") {
                    return None;
                }
                let position = quote.get("position").and_then(Value::as_i64)?.sat_i32();
                // A quote from a rich message lives in `rich_texts`; TDLib
                // leaves `text` empty. One Quill cannot read is dropped, so
                // it never goes back out as an ordinary quote.
                if let Some(values) = quote
                    .get("rich_texts")
                    .and_then(Value::as_array)
                    .filter(|values| !values.is_empty())
                {
                    let rich_texts = values
                        .iter()
                        .map(crate::rich::QuoteRichText::from_json)
                        .collect::<Option<Vec<_>>>()?;
                    let (text, entities) = crate::rich::join_quote_texts(&rich_texts);
                    return Some(crate::composer::QuoteSelection {
                        text,
                        position,
                        entities,
                        rich_texts,
                    });
                }
                let formatted = quote.get("text");
                let text = formatted
                    .and_then(|formatted| formatted.get("text"))
                    .and_then(Value::as_str)?;
                Some(crate::composer::QuoteSelection {
                    text: text.to_string(),
                    position,
                    entities: crate::text::quote::quote_entities(
                        &super::message_content::parse_text_entities(text, formatted),
                    ),
                    rich_texts: Vec::new(),
                })
            });
            let task = reply
                .get("checklist_task_id")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .sat_i32();
            let option = reply
                .get("poll_option_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let item = if task > 0 {
                crate::composer::ReplyItem::Task(task)
            } else if !option.is_empty() {
                crate::composer::ReplyItem::PollOption(option.to_string())
            } else {
                crate::composer::ReplyItem::Message
            };
            id.map(|message_id| DraftReply {
                message_id,
                quote,
                item,
            })
        }
        _ => None,
    };
    let suggested_post =
        crate::suggested_posts::SuggestTerms::parse_input(value.get("suggested_post_info"));
    if matches!(&body, DraftBody::Text(text) if text.trim().is_empty())
        && reply.is_none()
        && suggested_post.is_none()
    {
        return None;
    }
    Some(ChatDraft {
        body,
        reply,
        suggested_post,
    })
}

pub(crate) fn parse_chat_action(value: Option<&Value>) -> ChatAction {
    match value.and_then(|v| v.get("@type")).and_then(Value::as_str) {
        Some("chatActionTyping") => ChatAction::Typing,
        Some("chatActionRecordingVideo") => ChatAction::RecordingVideo,
        Some("chatActionUploadingVideo") => ChatAction::UploadingVideo,
        Some("chatActionRecordingVoiceNote") => ChatAction::RecordingVoice,
        Some("chatActionUploadingVoiceNote") => ChatAction::UploadingVoice,
        Some("chatActionUploadingPhoto") => ChatAction::UploadingPhoto,
        Some("chatActionUploadingDocument") => ChatAction::UploadingDocument,
        Some("chatActionChoosingSticker") => ChatAction::ChoosingSticker,
        Some("chatActionChoosingLocation") => ChatAction::ChoosingLocation,
        Some("chatActionChoosingContact") => ChatAction::ChoosingContact,
        Some("chatActionStartPlayingGame") => ChatAction::PlayingGame,
        Some("chatActionRecordingVideoNote") => ChatAction::RecordingRound,
        Some("chatActionUploadingVideoNote") => ChatAction::UploadingRound,
        None | Some("chatActionCancel") => ChatAction::Cancel,
        Some(_) => ChatAction::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::parse_chat_draft;
    use crate::composer::ReplyItem;

    fn draft(reply: serde_json::Value) -> Option<super::ChatDraft> {
        let value = serde_json::json!({
            "@type": "draftMessage",
            "reply_to": reply,
            "date": 1,
            "content": {
                "@type": "draftMessageContentText",
                "text": {"@type": "formattedText", "text": "hi", "entities": []}
            }
        });
        parse_chat_draft(Some(&value))
    }

    #[test]
    fn a_rich_draft_parses_into_its_blocks_and_summary() {
        let value = serde_json::json!({
            "@type": "draftMessage",
            "reply_to": {"@type": "inputMessageReplyToMessage", "message_id": 7},
            "date": 1,
            "content": {"@type": "draftMessageContentRichMessage", "message": {
                "@type": "richMessage", "is_rtl": false, "is_full": true, "blocks": [
                    {"@type": "pageBlockSectionHeading", "size": 1, "text": {"@type": "richTextPlain", "text": "Plan"}},
                    {"@type": "pageBlockParagraph", "text": {"@type": "richTextPlain", "text": "ship it"}}
                ]
            }}
        });
        let draft = parse_chat_draft(Some(&value)).expect("a rich draft");
        assert_eq!(draft.text(), "");
        assert_eq!(draft.reply_to_message_id(), Some(crate::ids::MessageId(7)));
        assert_eq!(
            draft.rich().map(|rich| rich.markup()).as_deref(),
            Some("# Plan\n\nship it")
        );
        assert_eq!(draft.preview_text(), "Plan\nship it");
    }

    #[test]
    fn a_draft_reply_keeps_its_task_or_option() {
        let base = |task: i64, option: &str| {
            serde_json::json!({
                "@type": "inputMessageReplyToMessage",
                "message_id": 5,
                "checklist_task_id": task,
                "poll_option_id": option
            })
        };
        assert_eq!(
            draft(base(3, "")).map(|d| d.item()),
            Some(ReplyItem::Task(3))
        );
        assert_eq!(
            draft(base(0, "AQ==")).map(|d| d.item()),
            Some(ReplyItem::PollOption("AQ==".into()))
        );
        assert_eq!(
            draft(base(0, "")).map(|d| d.item()),
            Some(ReplyItem::Message)
        );
    }
}
