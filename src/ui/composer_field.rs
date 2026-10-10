//! The composer's formatted message field (codex:composer-input), after
//! Telegram Desktop's `InputField` with `MessageField` tags: bold, italic and
//! the other formats show in the field as they will be sent, a mention of a
//! user without a username is a tag over their name, and a custom emoji is
//! drawn inline and moves the caret as one character.
//!
//! The field is the kit `Textarea` whose engine (`third_party/gpui-base`)
//! keeps formatting spans and atomic inline tokens as part of the document
//! (see `docs/decisions/codex-composer-input.md`). `quill::composer_doc` is
//! the document model and its conversion to composer markup, which is what
//! the send, draft and edit paths keep using: [`QuillApp::composer_markup`]
//! reads the field as markup, [`QuillApp::set_composer_markup`] loads markup
//! into it.
//!
//! While the rich editor is open the field holds raw block markup instead,
//! so every helper here passes text through unchanged in that mode.

use super::app::QuillApp;
use super::chat_theme::{accent_info, fill_muted, text_muted};
use gpui_base::input::{InlineToken, InputContent, TextSpan, TextSpanStyle, TextSpanStyler};
use gpui_kit::component::input::{InlineTokenContext, TextareaState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer_doc::{ComposerDoc, ComposerTag, DocEmoji, DocSpan};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

/// Token id prefix of a custom emoji in the field.
const EMOJI_TOKEN: &str = "custom-emoji:";

fn emoji_token(id: i64, fallback: &str) -> InlineToken {
    InlineToken::new(format!("{EMOJI_TOKEN}{id}"), fallback.to_string())
}

fn token_emoji_id(token_id: &str) -> Option<i64> {
    token_id.strip_prefix(EMOJI_TOKEN)?.parse().ok()
}

/// What the field holds, as a document.
pub(super) fn field_doc(state: &TextareaState) -> ComposerDoc {
    let mut doc = ComposerDoc::plain(state.value().to_string());
    doc.spans = state
        .spans()
        .iter()
        .filter_map(|span| {
            Some(DocSpan::new(
                span.range(),
                ComposerTag::from_tag(span.tag())?,
            ))
        })
        .collect();
    doc.emoji = state
        .tokens()
        .iter()
        .filter_map(|token| {
            Some(DocEmoji {
                range: token.range(),
                custom_emoji_id: token_emoji_id(token.token().id())?,
            })
        })
        .collect();
    doc
}

/// The field content for a document: its text, emoji tokens and spans.
pub(super) fn doc_content(doc: &ComposerDoc) -> InputContent {
    let mut content = InputContent::new(doc.text.clone());
    for emoji in &doc.emoji {
        let Some(fallback) = doc.text.get(emoji.range.clone()) else {
            continue;
        };
        let token = emoji_token(emoji.custom_emoji_id, fallback);
        if let Ok(next) = content.clone().with_token(emoji.range.clone(), token) {
            content = next;
        }
    }
    for span in &doc.spans {
        if let Ok(next) = content
            .clone()
            .with_span(span.range.clone(), span.tag.to_tag())
        {
            content = next;
        }
    }
    content
}

/// Engine spans for document spans.
fn engine_spans(spans: &[DocSpan]) -> Vec<TextSpan> {
    spans
        .iter()
        .map(|span| TextSpan::new(span.range.clone(), span.tag.to_tag()))
        .collect()
}

/// How each tag looks in the field (Telegram Desktop's `PrepareTagFormat`:
/// bold and italic fonts, link colour for links and mentions, monospace in
/// the code colour for code; spoilers and quotes get a tint so they read as
/// such while typing). Colours are read when a frame is drawn, so the field
/// follows the theme.
pub(super) fn span_styler() -> TextSpanStyler {
    Rc::new(|tag| {
        let tag = ComposerTag::from_tag(tag)?;
        let mut style = TextSpanStyle::default();
        let highlight = &mut style.highlight;
        match tag {
            ComposerTag::Bold => highlight.font_weight = Some(FontWeight::BOLD),
            ComposerTag::Italic => highlight.font_style = Some(FontStyle::Italic),
            ComposerTag::Underline => {
                highlight.underline = Some(UnderlineStyle {
                    thickness: px(1.),
                    ..Default::default()
                });
            }
            ComposerTag::Strikethrough => {
                highlight.strikethrough = Some(StrikethroughStyle {
                    thickness: px(1.),
                    ..Default::default()
                });
            }
            ComposerTag::Spoiler => {
                highlight.background_color = Some(Hsla::from(text_muted()).opacity(0.35));
            }
            ComposerTag::Code | ComposerTag::Pre(_) => {
                highlight.background_color = Some(fill_muted().into());
                style.font_family = Some(super::message_text::MONO_FONT.into());
            }
            ComposerTag::Quote => {
                highlight.background_color = Some(Hsla::from(accent_info()).opacity(0.12));
            }
            ComposerTag::Link(_) | ComposerTag::Mention(_) => {
                highlight.color = Some(accent_info().into());
            }
        }
        Some(style)
    })
}

/// The tag a formatting action toggles.
pub(super) fn tag_for_action(action: &quill::composer::FormatAction) -> ComposerTag {
    use quill::composer::FormatAction as A;
    match action {
        A::Bold => ComposerTag::Bold,
        A::Italic => ComposerTag::Italic,
        A::Underline => ComposerTag::Underline,
        A::Strikethrough => ComposerTag::Strikethrough,
        A::Code => ComposerTag::Code,
        A::Pre => ComposerTag::Pre(String::new()),
        A::Spoiler => ComposerTag::Spoiler,
        A::BlockQuote => ComposerTag::Quote,
        A::Link(url) => ComposerTag::Link(url.clone()),
    }
}

/// A typed markdown replacement that Backspace can still take back
/// (Telegram Desktop's `_reverseMarkdownReplacement`): the field text right
/// after it and the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MarkdownRevert {
    pub text: String,
    pub caret: usize,
}

impl QuillApp {
    /// The composer's content as markup, for sending, drafts and edits.
    pub(super) fn composer_markup(&self, cx: &App) -> String {
        let state = self.composer.read(cx);
        if self.rich_editor_open {
            return state.value().to_string();
        }
        field_doc(state).to_markup()
    }

    /// Load markup into the composer (formatting shown, history cleared).
    pub(super) fn set_composer_markup(
        &mut self,
        markup: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let content = if self.rich_editor_open {
            InputContent::new(markup.to_string())
        } else {
            doc_content(&ComposerDoc::from_markup(markup))
        };
        self.markdown_revert = None;
        self.composer
            .update(cx, |input, cx| input.set_value(content, window, cx));
    }

    /// Replace a byte range of the field with plain text, keeping the
    /// formatting around it, as one undo step.
    pub(super) fn replace_composer_text(
        &mut self,
        range: Range<usize>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_composer_content(range, InputContent::new(text.to_string()), window, cx);
    }

    fn replace_composer_content(
        &mut self,
        range: Range<usize>,
        content: InputContent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |input, cx| {
            let len = input.value().len();
            let range = range.start.min(len)..range.end.min(len);
            if input
                .replace_range_with_content(range.clone(), content.clone(), window, cx)
                .is_err()
            {
                // An IME composition is open: insert the text alone.
                let text = content.text().to_string();
                input.set_selected_range(range, cx);
                input.replace(text, window, cx);
            }
        });
    }

    /// Insert a custom emoji at the caret (the emoji panel).
    pub(super) fn insert_composer_custom_emoji(
        &mut self,
        fallback: &str,
        custom_emoji_id: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = self.composer.read(cx).selected_range();
        let fallback = if fallback.is_empty() { "⭐" } else { fallback };
        let content = if self.rich_editor_open {
            InputContent::new(quill::composer::custom_emoji_markup(
                fallback,
                custom_emoji_id,
            ))
        } else {
            let text = fallback.to_string();
            let len = text.len();
            InputContent::new(text.clone())
                .with_token(0..len, emoji_token(custom_emoji_id, &text))
                .unwrap_or_else(|_| InputContent::new(text))
        };
        self.replace_composer_content(range, content, window, cx);
    }

    /// Replace the `@query` being typed with a mention (Telegram Desktop's
    /// `InputField::insertTag`): `@username ` for a user with a username,
    /// otherwise their first name tagged as a mention, then a space.
    pub(super) fn insert_composer_mention(
        &mut self,
        user_id: i64,
        name: &str,
        username: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let text = self.composer.read(cx).value().to_string();
        let Some(query) = quill::composer::mention_trigger(&text) else {
            return;
        };
        let range = text.len() - query.len() - 1..text.len();
        let content = if !username.is_empty() {
            InputContent::new(format!("@{username} "))
        } else if self.rich_editor_open {
            InputContent::new(quill::moderation::mention_text(user_id, "", name))
        } else {
            mention_content(user_id, name)
        };
        self.replace_composer_content(range, content, window, cx);
    }

    /// Bold, italic and the rest from the menu or a shortcut: toggle the
    /// format over the selection; with nothing selected, toggle what the
    /// next typed text gets (Telegram Desktop's `toggleCurrentMarkdownTag`).
    pub(super) fn toggle_composer_tag(
        &mut self,
        tag: ComposerTag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.markdown_revert = None;
        self.composer.update(cx, |input, cx| {
            let doc = field_doc(input);
            let range = input.selected_range();
            if range.is_empty() && !matches!(tag, ComposerTag::Quote) {
                if matches!(tag, ComposerTag::Link(_)) {
                    return;
                }
                let mut tags: Vec<ComposerTag> = match input.typing_spans() {
                    Some(tags) => tags
                        .iter()
                        .filter_map(|tag| ComposerTag::from_tag(tag))
                        .collect(),
                    None => quill::composer_doc::tags_at(&doc.spans, range.start),
                };
                if let Some(at) = tags.iter().position(|t| t.same_kind(&tag)) {
                    tags.remove(at);
                } else {
                    tags.push(tag);
                }
                input.set_typing_spans(tags.iter().map(|t| t.to_tag().into()).collect());
                return;
            }
            let spans = quill::composer_doc::toggle_tag(&doc.text, &doc.spans, range, tag);
            input.set_spans(engine_spans(&spans), window, cx);
        });
        cx.notify();
    }

    /// "Clear formatting": the selection loses every format; with nothing
    /// selected, the next typed text is plain.
    pub(super) fn clear_composer_tags(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.markdown_revert = None;
        self.composer.update(cx, |input, cx| {
            let range = input.selected_range();
            if range.is_empty() {
                input.set_typing_spans(Vec::new());
                return;
            }
            let doc = field_doc(input);
            let spans = quill::composer_doc::clear_tags(&doc.spans, &range);
            input.set_spans(engine_spans(&spans), window, cx);
        });
        cx.notify();
    }

    /// The code block around the caret: its range and language.
    pub(super) fn composer_code_block_at_caret(&self, cx: &App) -> Option<(Range<usize>, String)> {
        let state = self.composer.read(cx);
        let caret = state.selected_range().start;
        field_doc(state)
            .spans
            .into_iter()
            .find_map(|span| match span.tag {
                ComposerTag::Pre(language)
                    if span.range.start <= caret && caret <= span.range.end =>
                {
                    Some((span.range, language))
                }
                _ => None,
            })
    }

    /// Give the code block at `range` a new language.
    pub(super) fn set_composer_code_language(
        &mut self,
        range: Range<usize>,
        language: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |input, cx| {
            let mut doc = field_doc(input);
            for span in &mut doc.spans {
                if span.range == range && matches!(span.tag, ComposerTag::Pre(_)) {
                    span.tag = ComposerTag::Pre(language.to_string());
                }
            }
            input.set_spans(engine_spans(&doc.spans), window, cx);
        });
    }

    /// After a keystroke: turn a typed `**markdown**` span whose closing
    /// marker was just typed into formatting (Telegram Desktop's
    /// `processMarkdownReplaces`). `prev` is the text before the keystroke.
    /// True when it replaced something.
    pub(super) fn apply_markdown_replacement(
        &mut self,
        prev: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.rich_editor_open {
            return false;
        }
        let revert = self
            .composer
            .update(cx, |input, cx| apply_markdown(input, prev, window, cx));
        let replaced = revert.is_some();
        if replaced {
            self.markdown_revert = revert;
        }
        replaced
    }

    /// Backspace right after a markdown replacement puts the markers back
    /// (undoes the replacement). True when it did.
    pub(super) fn try_revert_markdown(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(revert) = self.markdown_revert.take() else {
            return false;
        };
        let state = self.composer.read(cx);
        if !state.focus_handle(cx).is_focused(window)
            || state.value().as_ref() != revert.text
            || state.selected_range() != (revert.caret..revert.caret)
        {
            return false;
        }
        window.dispatch_action(Box::new(gpui_kit::component::input::Undo), cx);
        true
    }

    /// Draws the field's custom emoji: the animated or still image when it
    /// is loaded, the fallback emoji meanwhile.
    pub(super) fn composer_token_renderer(
        &self,
        cx: &mut Context<Self>,
    ) -> impl Fn(&InlineTokenContext, &mut Window, &mut App) -> AnyElement + 'static {
        let entities: Vec<quill::text::TextEntity> = self
            .composer
            .read(cx)
            .tokens()
            .iter()
            .filter_map(|token| {
                Some(quill::text::TextEntity {
                    utf8_start: token.range().start,
                    utf8_end: token.range().end,
                    kind: quill::text::TextEntityKind::CustomEmoji {
                        custom_emoji_id: token_emoji_id(token.token().id())?,
                    },
                })
            })
            .collect();
        let images: Rc<HashMap<i64, ImageSource>> = Rc::new(if entities.is_empty() {
            HashMap::new()
        } else {
            self.custom_emoji_images(&entities, cx)
        });
        move |token, _window, _cx| {
            let height = token.line_height();
            let size = (height * 0.9).round();
            let image = token_emoji_id(token.token().id()).and_then(|id| images.get(&id).cloned());
            let selected = token.is_selected();
            div()
                .h(height)
                .flex()
                .items_center()
                .when(selected, |el| el.bg(Hsla::from(accent_info()).opacity(0.3)))
                .child(match image {
                    Some(source) => img(source).size(size).into_any_element(),
                    None => div().child(token.token().text().clone()).into_any_element(),
                })
                .into_any_element()
        }
    }
}

/// [`QuillApp::apply_markdown_replacement`] on the field itself: when the
/// keystroke that turned `prev` into the current text typed the closing
/// marker of a `**markdown**` span, replace the span by its text with the
/// format, as one undo step, and return what Backspace would take back.
pub(super) fn apply_markdown(
    input: &mut TextareaState,
    prev: &str,
    window: &mut Window,
    cx: &mut Context<TextareaState>,
) -> Option<MarkdownRevert> {
    let range = input.selected_range();
    if !range.is_empty() {
        return None;
    }
    let doc = field_doc(input);
    let caret = range.start;
    // Exactly one character was typed just before the caret.
    let typed = doc.text.get(..caret)?.chars().next_back()?;
    let before = caret - typed.len_utf8();
    if prev.len() + typed.len_utf8() != doc.text.len()
        || prev.get(..before) != doc.text.get(..before)
        || prev.get(before..) != doc.text.get(caret..)
    {
        return None;
    }
    let code: Vec<Range<usize>> = doc
        .spans
        .iter()
        .filter(|s| matches!(s.tag, ComposerTag::Code | ComposerTag::Pre(_)))
        .map(|s| s.range.clone())
        .collect();
    let found = quill::composer_doc::markdown_replacement(&doc.text, caret, &code)?;
    // The inner text keeps its own formatting and emoji, and gets the new
    // format on top.
    let inner = found.inner.clone();
    let mut inner_doc = ComposerDoc::plain(&doc.text[inner.clone()]);
    let shift = |r: &Range<usize>| {
        r.start.max(inner.start) - inner.start..r.end.min(inner.end) - inner.start
    };
    inner_doc.spans = doc
        .spans
        .iter()
        .filter(|s| s.range.start < inner.end && inner.start < s.range.end)
        .map(|s| DocSpan::new(shift(&s.range), s.tag.clone()))
        .chain(std::iter::once(DocSpan::new(
            0..inner.len(),
            found.tag.clone(),
        )))
        .collect();
    inner_doc.emoji = doc
        .emoji
        .iter()
        .filter(|e| inner.start <= e.range.start && e.range.end <= inner.end)
        .map(|e| DocEmoji {
            range: shift(&e.range),
            custom_emoji_id: e.custom_emoji_id,
        })
        .collect();
    inner_doc.normalize();
    input
        .replace_range_with_content(found.outer.clone(), doc_content(&inner_doc), window, cx)
        .ok()?;
    // Typing goes on in plain text.
    input.set_typing_spans(Vec::new());
    Some(MarkdownRevert {
        text: input.value().to_string(),
        caret: input.selected_range().start,
    })
}

/// A mention of a user without a username: their first name tagged as a
/// mention, then an untagged space.
fn mention_content(user_id: i64, name: &str) -> InputContent {
    let name = name.trim();
    let name = if name.is_empty() { "user" } else { name };
    let text = format!("{name} ");
    InputContent::new(text.clone())
        .with_span(0..name.len(), ComposerTag::Mention(user_id).to_tag())
        .unwrap_or_else(|_| InputContent::new(text))
}

/// The first name to show for a mention tag (Telegram Desktop uses the first
/// name, the full name when there is none).
pub(super) fn mention_name(first_name: &str, display_name: &str) -> String {
    if first_name.trim().is_empty() {
        display_name.to_string()
    } else {
        first_name.to_string()
    }
}
