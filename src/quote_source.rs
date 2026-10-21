//! The text of a message a reply can quote, with its formatting: how it
//! splits into quotable parts and how a selected piece of it becomes a
//! [`QuoteSelection`] (tdesktop `HistoryView::Controls::EditDraftOptions`
//! and `Data::MessageHighlight`'s quote).
#![deny(clippy::indexing_slicing)]

use crate::composer::QuoteSelection;
use crate::reply_options::QUOTE_MAX_CHARS;
use crate::rich::RichBlock;
use crate::telegram::envelope::MessageContent;
use crate::text::TextEntity;
use crate::text::quote::{slice_entities, trim_with_entities};

/// One quotable text of a message: a whole message text, or one paragraph,
/// heading or list item of a rich message.
#[derive(Debug, Clone, PartialEq, Eq)]
struct QuoteNode {
    text: String,
    entities: Vec<TextEntity>,
}

/// Everything of a message that can be quoted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotableText {
    nodes: Vec<QuoteNode>,
    /// The nodes are the quotable texts of a rich message, so a quote's
    /// position counts in them joined by line feeds and a quote also goes out
    /// as `rich_texts` (TDLib 1.8.69 `inputTextQuote`).
    rich: bool,
}

impl QuotableText {
    /// Unformatted text of an ordinary message.
    pub fn plain(text: impl Into<String>) -> Self {
        Self::formatted(text, Vec::new())
    }

    /// The text of an ordinary message with its entities.
    pub fn formatted(text: impl Into<String>, entities: Vec<TextEntity>) -> Self {
        Self {
            nodes: vec![QuoteNode {
                text: text.into(),
                entities,
            }],
            rich: false,
        }
    }

    /// The quotable text of `content` where its formatting is known: a text
    /// message, or a rich message made only of paragraphs, headings and
    /// lists (any other block holds quotable text Quill cannot place, which
    /// would put the position of a quote off). `None` otherwise.
    pub fn of(content: &MessageContent) -> Option<Self> {
        match content {
            MessageContent::Text(text) if !text.text.trim().is_empty() => {
                Some(Self::formatted(&text.text, text.entities.clone()))
            }
            MessageContent::RichMessage(rich) => {
                let mut nodes = Vec::new();
                for block in &rich.blocks {
                    match block {
                        RichBlock::Paragraph { text, entities, .. }
                        | RichBlock::Heading { text, entities, .. } => nodes.push(QuoteNode {
                            text: text.clone(),
                            entities: entities.clone(),
                        }),
                        RichBlock::List { items, .. } => {
                            nodes.extend(items.iter().map(|item| QuoteNode {
                                text: item.text.clone(),
                                entities: Vec::new(),
                            }))
                        }
                        RichBlock::ButtonRow { .. } | RichBlock::Divider | RichBlock::Empty => {}
                        _ => return None,
                    }
                }
                nodes.retain(|node| !node.text.is_empty());
                (!nodes.is_empty()).then_some(Self { nodes, rich: true })
            }
            _ => None,
        }
    }

    /// The parts to pick from: one per line, and inside a line one per
    /// sentence, each with its formatting. Positions are UTF-16 code-unit
    /// offsets as `inputTextQuote.position` needs. Parts longer than
    /// [`QUOTE_MAX_CHARS`] are left out.
    pub fn segments(&self) -> Vec<QuoteSelection> {
        let mut parts = Vec::new();
        for (node, base) in self.nodes_with_base() {
            let mut offset16 = base;
            let mut offset8 = 0usize;
            for line in node.text.split_inclusive('\n') {
                self.push_sentences(node, line, offset8, offset16, &mut parts);
                offset8 += line.len();
                offset16 += line.encode_utf16().count();
            }
        }
        parts
    }

    /// The quote for `selected`, a verbatim piece of one text of the message.
    pub fn quote_for(&self, selected: &str) -> Option<QuoteSelection> {
        if selected.is_empty() {
            return None;
        }
        self.nodes_with_base().find_map(|(node, base)| {
            let start = node.text.find(selected)?;
            let before = node.text.get(..start)?.encode_utf16().count();
            Some(QuoteSelection {
                text: selected.to_string(),
                position: i32::try_from(base + before).ok()?,
                entities: slice_entities(&node.entities, start..start + selected.len()),
                from_rich: self.rich,
            })
        })
    }

    /// Each text with the UTF-16 offset where it starts in the position
    /// text: the texts joined by line feeds for a rich message, the text
    /// itself otherwise.
    fn nodes_with_base(&self) -> impl Iterator<Item = (&QuoteNode, usize)> {
        let mut base = 0usize;
        self.nodes.iter().map(move |node| {
            let start = base;
            base += node.text.encode_utf16().count() + 1;
            (node, start)
        })
    }

    fn push_sentences(
        &self,
        node: &QuoteNode,
        line: &str,
        line8: usize,
        base16: usize,
        parts: &mut Vec<QuoteSelection>,
    ) {
        let mut start = 0usize;
        let mut chars = line.char_indices().peekable();
        while let Some((index, ch)) = chars.next() {
            let ends = matches!(ch, '.' | '!' | '?' | '…')
                && chars.peek().is_none_or(|(_, next)| next.is_whitespace());
            if ends {
                let end = index + ch.len_utf8();
                self.push_part(node, line, line8, start, end, base16, parts);
                start = end;
            }
        }
        self.push_part(node, line, line8, start, line.len(), base16, parts);
    }

    #[allow(clippy::too_many_arguments)]
    fn push_part(
        &self,
        node: &QuoteNode,
        line: &str,
        line8: usize,
        start: usize,
        end: usize,
        base16: usize,
        parts: &mut Vec<QuoteSelection>,
    ) {
        let raw = line.get(start..end).unwrap_or_default();
        let lead = raw.len() - raw.trim_start().len();
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.chars().count() > QUOTE_MAX_CHARS {
            return;
        }
        let position = base16
            + line
                .get(..start + lead)
                .unwrap_or_default()
                .encode_utf16()
                .count();
        let from = line8 + start;
        let (_, entities) = trim_with_entities(
            node.text.get(from..line8 + end).unwrap_or_default(),
            &slice_entities(&node.entities, from..line8 + end),
        );
        parts.push(QuoteSelection {
            text: trimmed.to_string(),
            position: i32::try_from(position).unwrap_or(i32::MAX),
            entities,
            from_rich: self.rich,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::QuotableText;
    use crate::rich::{RichBlock, RichListItem};
    use crate::telegram::envelope::{MessageContent, RichMessageContent, TextContent};
    use crate::text::{TextEntity, TextEntityKind};

    fn entity(start: usize, end: usize, kind: TextEntityKind) -> TextEntity {
        TextEntity {
            utf8_start: start,
            utf8_end: end,
            kind,
        }
    }

    fn rich(blocks: Vec<RichBlock>) -> MessageContent {
        MessageContent::RichMessage(RichMessageContent {
            blocks,
            is_full: true,
        })
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
        assert!(!parts[1].from_rich);
        let picked = source.quote_for("bold. two italic").expect("verbatim");
        assert_eq!(picked.position, 4);
        assert_eq!(picked.entities.len(), 2);
    }

    #[test]
    fn rich_message_positions_count_the_texts_joined_by_line_feeds() {
        let source = QuotableText::of(&rich(vec![
            RichBlock::Heading {
                level: 1,
                text: "Title".into(),
                entities: Vec::new(),
            },
            RichBlock::Divider,
            RichBlock::Paragraph {
                text: "some bold text".into(),
                entities: vec![entity(5, 9, TextEntityKind::Bold)],
                buttons: Vec::new(),
            },
            RichBlock::List {
                ordered: false,
                items: vec![RichListItem {
                    text: "item".into(),
                    checked: None,
                }],
            },
        ]))
        .expect("a rich message of text blocks");
        let parts = source.segments();
        let positions: Vec<_> = parts.iter().map(|part| part.position).collect();
        // "Title\nsome bold text\nitem": 0, 6, 21.
        assert_eq!(positions, [0, 6, 21]);
        assert!(parts.iter().all(|part| part.from_rich));
        assert_eq!(parts[1].entities, vec![entity(5, 9, TextEntityKind::Bold)]);
        let picked = source.quote_for("bold").expect("inside the paragraph");
        assert_eq!(picked.position, 11);
        assert_eq!(picked.entities, vec![entity(0, 4, TextEntityKind::Bold)]);
    }

    #[test]
    fn a_rich_message_with_blocks_that_cannot_be_placed_is_not_quotable() {
        let content = rich(vec![
            RichBlock::Paragraph {
                text: "text".into(),
                entities: Vec::new(),
                buttons: Vec::new(),
            },
            RichBlock::Table {
                rows: vec![vec!["cell".into()]],
            },
        ]);
        assert!(QuotableText::of(&content).is_none());
    }
}
