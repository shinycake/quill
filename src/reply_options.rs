//! The reply options of the composer's reply bar (tdesktop
//! `HistoryView::Controls::EditDraftOptions`,
//! `history/view/controls/history_view_draft_options.cpp`): what the
//! bar's menu offers, how a message splits into quotable parts, and the
//! wording of a reply that comes from another chat.
#![deny(clippy::indexing_slicing)]

/// Longest quote Telegram accepts by default (`quote_length_max`).
pub const QUOTE_MAX_CHARS: usize = 1024;

/// An entry of the reply bar's options menu, in tdesktop's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyOption {
    /// Pick the part of the message to quote (`lng_reply_options_quote`).
    UpdateQuote,
    /// Choose another chat to send the reply in (`lng_reply_in_another_chat`).
    ReplyInAnotherChat,
    /// Jump to the message (`lng_reply_show_in_chat`).
    ShowInChat,
    /// Drop the reply (`lng_reply_remove`).
    DoNotReply,
}

impl ReplyOption {
    pub fn label(self, has_quote: bool) -> &'static str {
        match self {
            Self::UpdateQuote if has_quote => "Update Quote",
            Self::UpdateQuote => "Quote Part of Message",
            Self::ReplyInAnotherChat => "Reply in Another Chat",
            Self::ShowInChat => "Show in Chat",
            Self::DoNotReply => "Do Not Reply",
        }
    }
}

/// What a reply can offer. `has_text`: the message has text to quote;
/// `can_reply_elsewhere`: TDLib's `can_be_replied_in_another_chat` and a
/// chat that is not secret; `same_chat`: the reply is aimed at the chat
/// it is from (a reply headed elsewhere cannot jump back without losing
/// the open chat).
pub fn reply_options(
    has_text: bool,
    can_reply_elsewhere: bool,
    same_chat: bool,
) -> Vec<ReplyOption> {
    let mut options = Vec::new();
    if has_text {
        options.push(ReplyOption::UpdateQuote);
    }
    if can_reply_elsewhere {
        options.push(ReplyOption::ReplyInAnotherChat);
    }
    if same_chat {
        options.push(ReplyOption::ShowInChat);
    }
    options.push(ReplyOption::DoNotReply);
    options
}

/// Title of the reply bar: "Reply to Ada", or "Reply to Ada from Design"
/// when the message belongs to another chat than the one being written in.
pub fn reply_bar_title(sender: Option<&str>, from_chat: Option<&str>) -> String {
    match (sender, from_chat) {
        (Some(name), Some(chat)) => format!("Reply to {name} from {chat}"),
        (None, Some(chat)) => format!("Reply to a message from {chat}"),
        (Some(name), None) => format!("Reply to {name}"),
        (None, None) => "Reply".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use crate::composer::{QuoteSelection, quote_position};
    use crate::quote_source::QuotableText;
    use crate::reply_options::{ReplyOption, reply_bar_title, reply_options};

    fn quote_segments(text: &str) -> Vec<QuoteSelection> {
        QuotableText::plain(text).segments()
    }

    #[test]
    fn options_follow_the_message() {
        assert_eq!(
            reply_options(true, true, true),
            vec![
                ReplyOption::UpdateQuote,
                ReplyOption::ReplyInAnotherChat,
                ReplyOption::ShowInChat,
                ReplyOption::DoNotReply
            ]
        );
        assert_eq!(
            reply_options(false, false, false),
            vec![ReplyOption::DoNotReply]
        );
        assert_eq!(ReplyOption::UpdateQuote.label(true), "Update Quote");
        assert_eq!(
            ReplyOption::UpdateQuote.label(false),
            "Quote Part of Message"
        );
    }

    #[test]
    fn bar_title_names_the_source_chat_only_for_other_chats() {
        assert_eq!(reply_bar_title(Some("Ada"), None), "Reply to Ada");
        assert_eq!(
            reply_bar_title(Some("Ada"), Some("Design")),
            "Reply to Ada from Design"
        );
        assert_eq!(
            reply_bar_title(None, Some("Design")),
            "Reply to a message from Design"
        );
        assert_eq!(reply_bar_title(None, None), "Reply");
    }

    #[test]
    fn segments_split_lines_and_sentences_with_utf16_positions() {
        let text = "Hello \u{1F600} there. Second one!\n\nThird line";
        let parts = quote_segments(text);
        let texts: Vec<_> = parts.iter().map(|part| part.text.as_str()).collect();
        assert_eq!(
            texts,
            ["Hello \u{1F600} there.", "Second one!", "Third line"]
        );
        for part in &parts {
            assert_eq!(quote_position(text, &part.text), Some(part.position));
        }
        // The emoji is two UTF-16 units: "Second" starts after 16 units.
        assert_eq!(parts[1].position, 16);
    }

    #[test]
    fn dots_inside_words_do_not_split() {
        let parts = quote_segments("See v1.2.3 and example.com now.");
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn repeated_text_keeps_its_own_position() {
        let text = "Yes.\nYes.";
        let parts = quote_segments(text);
        assert_eq!(parts[0].position, 0);
        assert_eq!(parts[1].position, 5);
    }

    #[test]
    fn overlong_parts_are_skipped() {
        let long = "a".repeat(2000);
        assert!(quote_segments(&long).is_empty());
    }
}
