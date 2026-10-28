//! The text of a message a reply can quote, with its formatting: how it
//! splits into quotable parts and how a selected piece of it becomes a
//! [`QuoteSelection`] (tdesktop `HistoryView::Controls::EditDraftOptions`
//! and `Data::MessageHighlight`'s quote).
#![deny(clippy::indexing_slicing)]

use crate::composer::QuoteSelection;
use crate::reply_options::QUOTE_MAX_CHARS;
use crate::rich::QuoteRichText;
use crate::telegram::envelope::MessageContent;
use crate::text::TextEntity;
use crate::text::quote::{quote_entities, slice_entities};
use std::ops::Range;

/// Most texts one quote of a rich message may cover (`inputTextQuote`).
const MAX_RICH_TEXTS: usize = 32;

/// One quotable text of a message: a whole message text, or one quotable
/// text of a rich message (TDLib `for_each_quote_text`).
#[derive(Debug, Clone, PartialEq, Eq)]
struct QuoteNode {
    text: String,
    /// Only the formatting a quote keeps.
    entities: Vec<TextEntity>,
    /// The text as a `RichText`, for a quotable text of a rich message.
    rich: Option<QuoteRichText>,
}

/// Everything of a message that can be quoted. For a rich message a quote's
/// position counts in the nodes joined by line feeds, and the quote also
/// goes out as `rich_texts` (TDLib 1.8.69 `inputTextQuote`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotableText {
    nodes: Vec<QuoteNode>,
}

impl QuotableText {
    /// Unformatted text of an ordinary message.
    pub fn plain(text: impl Into<String>) -> Self {
        Self::formatted(text, Vec::new())
    }

    /// The text of an ordinary message with its entities; only those a
    /// quote keeps stay.
    pub fn formatted(text: impl Into<String>, entities: Vec<TextEntity>) -> Self {
        Self {
            nodes: vec![QuoteNode {
                text: text.into(),
                entities: quote_entities(&entities),
                rich: None,
            }],
        }
    }

    /// The quotable text of `content` where its formatting is known: a text
    /// message, a media caption, or a rich message whose quotable texts
    /// Quill could read. `None` otherwise.
    pub fn of(content: &MessageContent) -> Option<Self> {
        let (text, entities) = match content {
            MessageContent::Text(text) => (&text.text, &text.entities),
            MessageContent::Photo(media) => (&media.caption, &media.caption_entities),
            MessageContent::Video(media) => (&media.caption, &media.caption_entities),
            MessageContent::Animation(media) => (&media.caption, &media.caption_entities),
            MessageContent::Document(media) => (&media.caption, &media.caption_entities),
            MessageContent::Audio(media) => (&media.caption, &media.caption_entities),
            MessageContent::VoiceNote(media) => (&media.caption, &media.caption_entities),
            _ => return Self::of_rich(content),
        };
        (!text.trim().is_empty()).then(|| Self::formatted(text, entities.clone()))
    }

    fn of_rich(content: &MessageContent) -> Option<Self> {
        let MessageContent::RichMessage(rich) = content else {
            return None;
        };
        let nodes: Vec<QuoteNode> = rich
            .quote_texts
            .iter()
            .map(|rich_text| {
                let (text, entities) = rich_text.parts();
                QuoteNode {
                    text,
                    entities,
                    rich: Some(rich_text.clone()),
                }
            })
            .collect();
        (!nodes.is_empty()).then_some(Self { nodes })
    }

    /// What a reply to `content` can quote. `copy_text` is the message's
    /// copyable text, used for content with no known formatting. A rich
    /// message never falls back to it: an ordinary quote of a rich message is
    /// not allowed (`schema/td_api.tl:3483`), so one Quill cannot read offers
    /// no parts and the reply goes to the whole message.
    pub fn for_reply(content: &MessageContent, copy_text: Option<String>) -> Option<Self> {
        if matches!(content, MessageContent::RichMessage(_)) {
            return Self::of(content);
        }
        Self::of(content).or_else(|| {
            copy_text
                .filter(|text| !text.trim().is_empty())
                .map(Self::plain)
        })
    }

    /// The parts to pick from: one per line, and inside a line one per
    /// sentence, each with its formatting. Positions are UTF-16 code-unit
    /// offsets as `inputTextQuote.position` needs. Parts longer than
    /// [`QUOTE_MAX_CHARS`], and parts that would cut custom emoji or math
    /// of a rich message, are left out.
    pub fn segments(&self) -> Vec<QuoteSelection> {
        let mut parts = Vec::new();
        for (node, _, base16) in self.nodes_with_bases() {
            let mut line8 = 0usize;
            for line in node.text.split_inclusive('\n') {
                let mut start = 0usize;
                let mut chars = line.char_indices().peekable();
                while let Some((index, ch)) = chars.next() {
                    let ends = matches!(ch, '.' | '!' | '?' | '…')
                        && chars.peek().is_none_or(|(_, next)| next.is_whitespace());
                    if ends {
                        let end = index + ch.len_utf8();
                        parts.extend(sentence(node, base16, line, line8, start..end));
                        start = end;
                    }
                }
                parts.extend(sentence(node, base16, line, line8, start..line.len()));
                line8 += line.len();
            }
        }
        parts
    }

    /// The quote for `selected`, verbatim text of the message. In a rich
    /// message it may run over several quotable texts, separated by line
    /// feeds; each becomes its own `RichText`. The first occurrence wins.
    pub fn quote_for(&self, selected: &str) -> Option<QuoteSelection> {
        if selected.is_empty() {
            return None;
        }
        let joined = self
            .nodes
            .iter()
            .map(|node| node.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let start = joined.find(selected)?;
        let end = start + selected.len();
        let mut quote = QuoteSelection::plain(selected, 0);
        let mut first = true;
        for (node, base8, base16) in self.nodes_with_bases() {
            let from = start.max(base8);
            let to = end.min(base8 + node.text.len());
            if from >= to {
                continue;
            }
            let piece = node_quote(node, base16, from - base8..to - base8)?;
            if first {
                quote.position = piece.position;
                first = false;
            }
            let shift = from - start;
            quote
                .entities
                .extend(piece.entities.into_iter().map(|entity| TextEntity {
                    utf8_start: entity.utf8_start + shift,
                    utf8_end: entity.utf8_end + shift,
                    kind: entity.kind,
                }));
            quote.rich_texts.extend(piece.rich_texts);
        }
        let too_long = !quote.rich_texts.is_empty()
            && (quote.rich_texts.len() > MAX_RICH_TEXTS
                || selected.chars().count() > QUOTE_MAX_CHARS);
        (!first && !too_long).then_some(quote)
    }

    /// Each text with the UTF-8 and UTF-16 offsets where it starts in the
    /// position text: the texts joined by line feeds for a rich message,
    /// the text itself otherwise.
    fn nodes_with_bases(&self) -> impl Iterator<Item = (&QuoteNode, usize, usize)> {
        let (mut base8, mut base16) = (0usize, 0usize);
        self.nodes.iter().map(move |node| {
            let at = (node, base8, base16);
            base8 += node.text.len() + 1;
            base16 += node.text.encode_utf16().count() + 1;
            at
        })
    }
}

/// The part for the sentence `range` of `line`, which starts at byte `line8`
/// of the node: trimmed, and left out when empty or too long.
fn sentence(
    node: &QuoteNode,
    base16: usize,
    line: &str,
    line8: usize,
    range: Range<usize>,
) -> Option<QuoteSelection> {
    let raw = line.get(range.clone())?;
    let lead = raw.len() - raw.trim_start().len();
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().count() > QUOTE_MAX_CHARS {
        return None;
    }
    let from = line8 + range.start + lead;
    node_quote(node, base16, from..from + trimmed.len())
}

/// The quote of bytes `range` of `node`, whose text starts at UTF-16 offset
/// `base16` of the position text. `None` when the range is not on char
/// boundaries or would cut custom emoji or math, which a quote keeps whole.
fn node_quote(node: &QuoteNode, base16: usize, range: Range<usize>) -> Option<QuoteSelection> {
    let text = node.text.get(range.clone())?;
    let before = node.text.get(..range.start)?;
    let rich_texts = match &node.rich {
        None => Vec::new(),
        Some(rich) => {
            let begin = before.chars().count();
            let fragment = rich.fragment(begin, begin + text.chars().count())?;
            if fragment.quote_text() != text {
                return None;
            }
            vec![fragment]
        }
    };
    Some(QuoteSelection {
        text: text.to_string(),
        position: i32::try_from(base16 + before.encode_utf16().count()).ok()?,
        entities: slice_entities(&node.entities, range),
        rich_texts,
    })
}

#[cfg(test)]
mod tests {
    use super::QuotableText;
    use crate::telegram::envelope::{MessageContent, TextContent};
    use crate::text::{TextEntity, TextEntityKind};

    fn entity(start: usize, end: usize, kind: TextEntityKind) -> TextEntity {
        TextEntity {
            utf8_start: start,
            utf8_end: end,
            kind,
        }
    }

    #[test]
    fn a_part_keeps_the_formatting_inside_it() {
        // "one bold. two italic"
        let source = QuotableText::of(&MessageContent::Text(TextContent {
            text: "one bold. two italic".into(),
            entities: vec![
                entity(4, 8, TextEntityKind::Bold),
                entity(14, 20, TextEntityKind::Italic),
            ],
            link_preview: None,
            animated: None,
        }))
        .expect("text quotes");
        let parts = source.segments();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].entities, vec![entity(4, 8, TextEntityKind::Bold)]);
        assert_eq!(parts[1].text, "two italic");
        assert_eq!(
            parts[1].entities,
            vec![entity(4, 10, TextEntityKind::Italic)]
        );
        assert!(parts[1].rich_texts.is_empty());
        let picked = source.quote_for("bold. two italic").expect("verbatim");
        assert_eq!(picked.position, 4);
        assert_eq!(picked.entities.len(), 2);
        assert!(picked.rich_texts.is_empty());
    }

    #[test]
    fn rich_message_positions_count_the_texts_joined_by_line_feeds() {
        let source = QuotableText::of(&parsed(serde_json::json!([
            {"@type": "pageBlockTitle", "title": plain_json("Title")},
            {"@type": "pageBlockDivider"},
            paragraph_json(serde_json::json!({"@type": "richTexts", "texts": [
                plain_json("some "),
                {"@type": "richTextBold", "text": plain_json("bold")},
                plain_json(" text"),
            ]})),
            {"@type": "pageBlockList", "items": [
                item_json("\u{2022}", vec![paragraph_json(plain_json("item"))]),
            ]},
        ])))
        .expect("a rich message of text blocks");
        let parts = source.segments();
        let positions: Vec<_> = parts.iter().map(|part| part.position).collect();
        // "Title\nsome bold text\nitem": 0, 6, 21.
        assert_eq!(positions, [0, 6, 21]);
        assert!(parts.iter().all(|part| part.rich_texts.len() == 1));
        assert_eq!(parts[1].entities, vec![entity(5, 9, TextEntityKind::Bold)]);
        let picked = source.quote_for("bold").expect("inside the paragraph");
        assert_eq!(picked.position, 11);
        assert_eq!(picked.entities, vec![entity(0, 4, TextEntityKind::Bold)]);
        assert_eq!(
            crate::telegram::requests::input_text_quote_json(&picked)["rich_texts"],
            serde_json::json!([{"@type": "richTexts", "texts": [
                {"@type": "richTextBold", "text": plain_json("bold")},
            ]}])
        );
    }

    #[test]
    fn a_selection_cannot_cut_math_in_two() {
        let source = QuotableText::of(&parsed(serde_json::json!([paragraph_json(
            serde_json::json!({"@type": "richTexts", "texts": [
                plain_json("so "),
                {"@type": "richTextMathematicalExpression", "expression": "a+b"},
            ]})
        ),])))
        .expect("quotable");
        assert!(source.quote_for("so a+").is_none());
        let whole = source.quote_for("so a+b").expect("the math whole");
        assert_eq!(whole.rich_texts.len(), 1);
    }

    fn parsed(blocks: serde_json::Value) -> MessageContent {
        crate::telegram::envelope::parse_message_rich_message(&serde_json::json!({
            "@type": "messageRichMessage",
            "message": {"@type": "richMessage", "blocks": blocks, "is_full": true},
        }))
        .0
    }

    fn plain_json(text: &str) -> serde_json::Value {
        serde_json::json!({"@type": "richTextPlain", "text": text})
    }

    fn paragraph_json(text: serde_json::Value) -> serde_json::Value {
        serde_json::json!({"@type": "pageBlockParagraph", "text": text})
    }

    fn item_json(label: &str, blocks: Vec<serde_json::Value>) -> serde_json::Value {
        serde_json::json!({"@type": "pageBlockListItem", "label": label, "blocks": blocks,
            "has_checkbox": false, "is_checked": false, "value": 0, "type": ""})
    }

    #[test]
    fn list_labels_are_not_quoted_and_block_quote_credits_are() {
        // TDLib counts a list item's blocks without its label, and a block
        // quote's credit after its blocks (`WebPageBlock.cpp`
        // `for_each_quote_text`).
        let content = parsed(serde_json::json!([
            {"@type": "pageBlockList", "items": [
                item_json("1.", vec![paragraph_json(serde_json::json!(
                    {"@type": "richTextBold", "text": plain_json("first")}))]),
                item_json("\u{2022}", vec![
                    paragraph_json(plain_json("second")),
                    paragraph_json(plain_json("nested")),
                ]),
            ]},
            {"@type": "pageBlockBlockQuote",
             "blocks": [paragraph_json(plain_json("quoted"))],
             "credit": plain_json("Ada")},
        ]));
        let source = QuotableText::of(&content).expect("lists and block quotes quote");
        let parts = source.segments();
        let found: Vec<_> = parts
            .iter()
            .map(|part| (part.text.as_str(), part.position))
            .collect();
        // "first\nsecond\nnested\nquoted\nAda"
        assert_eq!(
            found,
            [
                ("first", 0),
                ("second", 6),
                ("nested", 13),
                ("quoted", 20),
                ("Ada", 27)
            ]
        );
        assert_eq!(parts[0].entities, vec![entity(0, 5, TextEntityKind::Bold)]);
        // A selection across two quotable texts goes out as two RichTexts.
        let picked = source.quote_for("second\nnested").expect("two texts");
        assert_eq!(picked.position, 6);
        let sent = crate::telegram::requests::input_text_quote_json(&picked);
        assert_eq!(
            sent["rich_texts"],
            serde_json::json!([plain_json("second"), plain_json("nested")])
        );
    }

    #[test]
    fn parts_keep_only_the_formatting_a_quote_may_hold() {
        let source = QuotableText::formatted(
            "run code now",
            vec![
                entity(0, 3, TextEntityKind::Bold),
                entity(4, 8, TextEntityKind::Code),
                entity(
                    9,
                    12,
                    TextEntityKind::TextUrl {
                        url: "https://example.com".into(),
                    },
                ),
            ],
        );
        let parts = source.segments();
        assert_eq!(parts[0].entities, vec![entity(0, 3, TextEntityKind::Bold)]);
        let picked = source.quote_for("code now").expect("verbatim");
        assert!(picked.entities.is_empty());
    }

    #[test]
    fn a_media_caption_quotes_with_its_formatting() {
        let (content, _) = crate::telegram::envelope::parse_content(Some(&serde_json::json!({
            "@type": "messagePhoto",
            "photo": {"@type": "photo", "has_stickers": false, "minithumbnail": null, "sizes": []},
            "caption": {"@type": "formattedText", "text": "a bold view", "entities": [
                {"@type": "textEntity", "offset": 2, "length": 4,
                 "type": {"@type": "textEntityTypeBold"}},
            ]},
            "show_caption_above_media": false, "has_spoiler": false, "is_secret": false,
        })));
        let source = QuotableText::for_reply(&content, None).expect("captions quote");
        let picked = source.quote_for("bold view").expect("verbatim");
        assert_eq!(picked.position, 2);
        assert_eq!(picked.entities, vec![entity(0, 4, TextEntityKind::Bold)]);
    }

    #[test]
    fn a_rich_message_never_falls_back_to_its_copy_text() {
        // A block Quill cannot place: quoting its copy text would send an
        // ordinary quote of a rich message, which the schema forbids.
        let content = parsed(serde_json::json!([
            paragraph_json(plain_json("text")),
            {"@type": "pageBlockFromTheFuture"},
        ]));
        assert!(QuotableText::for_reply(&content, Some("text".into())).is_none());
        let other = MessageContent::Unsupported {
            type_name: "messageSomething".into(),
        };
        assert!(QuotableText::for_reply(&other, Some("caption".into())).is_some());
    }
}
