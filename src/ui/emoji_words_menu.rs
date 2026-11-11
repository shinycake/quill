//! The composer's right-click items for a word's emoji, and the list of
//! words without them in Settings › Chats (docs/decisions/
//! emoji-word-highlights.md): "Suggest Emoji" opens a word's bar whether
//! or not it is highlighted, "Don't Suggest Emoji for This Word" stops
//! highlighting it, and "Turn Off Emoji Suggestions" turns the highlights
//! off with a note on where to turn them back on.
#![deny(clippy::indexing_slicing)]

use std::ops::Range;

use super::actions::{EmojiWordBlock, EmojiWordSuggest, EmojiWordsOff};
use super::app::QuillApp;
use super::emoji_words_ui::AskedWord;
use super::status_note::StatusNote;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::*;
use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::emoji::keyword_lookup::{keyword_emoji, normalize_query};
use quill::emoji_words::Highlight;
use quill::lang::{keys, tr, tr_with};
use quill::spellcheck::word_at;

impl QuillApp {
    /// The word items for the right-click menu at `selection` (the click
    /// moves the caret there), then a separator; `menu` unchanged when no
    /// word is there or emoji suggestions are off.
    pub(super) fn emoji_word_menu_items(
        &self,
        menu: NativeMenu,
        text: &str,
        selection: Range<usize>,
    ) -> NativeMenu {
        let entries = self.emoji_word_menu_entries(text, selection);
        if entries.is_empty() {
            return menu;
        }
        entries
            .into_iter()
            .fold(menu, |menu, (label, action)| menu.menu(label, action))
            .separator()
    }

    /// The labels and actions of [`Self::emoji_word_menu_items`].
    pub(super) fn emoji_word_menu_entries(
        &self,
        text: &str,
        selection: Range<usize>,
    ) -> Vec<(&'static str, Box<dyn Action>)> {
        let mut out: Vec<(&'static str, Box<dyn Action>)> = Vec::new();
        if !self.chat_prefs.suggest_emoji {
            return out;
        }
        let Some(range) = word_at(text, selection.start) else {
            return out;
        };
        if !selection.is_empty() && selection != range {
            return out;
        }
        let Some(word) = text.get(range.clone()) else {
            return out;
        };
        out.push((
            tr(keys::Q_CHAT_EMOJI_WORD_SUGGEST),
            Box::new(EmojiWordSuggest {
                start: range.start,
                end: range.end,
                word: word.to_string(),
            }),
        ));
        if self.chat_prefs.highlight_emoji_words {
            let normalized = normalize_query(word);
            // The click put the caret in the word, so it is not lit now:
            // offer this for any word that would light up.
            let lights = self.session().is_some_and(|s| {
                s.stickers
                    .emoji
                    .keyword_lookups
                    .answer(&normalized)
                    .is_some_and(|answer| !keyword_emoji(&normalized, true, answer).is_empty())
            });
            if lights && !self.chat_prefs.emoji_word_blocklist.contains(&normalized) {
                out.push((
                    tr(keys::Q_CHAT_EMOJI_WORD_BLOCK),
                    Box::new(EmojiWordBlock { word: normalized }),
                ));
            }
            out.push((tr(keys::Q_CHAT_EMOJI_WORDS_OFF), Box::new(EmojiWordsOff)));
        }
        out
    }

    /// "Suggest Emoji": look the word up and open its bar when TDLib
    /// answers (at once when the answer is cached).
    pub(super) fn on_emoji_word_suggest(
        &mut self,
        action: &EmojiWordSuggest,
        cx: &mut Context<Self>,
    ) {
        let text = self.composer.read(cx).value().to_string();
        if text.get(action.start..action.end) != Some(action.word.as_str()) {
            return;
        }
        let normalized = normalize_query(&action.word);
        let sent = match self.live.as_mut() {
            Some(live) => live
                .driver
                .lookup_emoji_keywords(std::iter::once(normalized.as_str()))
                .is_ok(),
            None => true,
        };
        if !sent {
            // Not signed in, or the request could not go: say so now.
            self.set_note(StatusNote::info(tr(keys::Q_CHAT_EMOJI_WORD_LOOKUP_FAILED)));
            cx.notify();
            return;
        }
        self.composer_ui.emoji_words.asked = AskedWord::Waiting {
            range: action.start..action.end,
            word: normalized,
        };
        self.answer_asked_word(cx);
    }

    /// Open the bar for the word "Suggest Emoji" asked about once its
    /// keyword answer is in; a note when it has no emoji.
    pub(super) fn answer_asked_word(&mut self, cx: &mut Context<Self>) {
        let AskedWord::Waiting {
            range,
            word: normalized,
        } = self.composer_ui.emoji_words.asked.clone()
        else {
            return;
        };
        let lookups = self.session().map(|s| &s.stickers.emoji.keyword_lookups);
        let answered = lookups.is_none_or(|l| l.answer(&normalized).is_some());
        if !answered {
            // Out of attempts: it will not answer, so stop waiting and say so
            // instead of opening the bar unprompted if a later lookup of the
            // word lands.
            if lookups.is_some_and(|l| l.gave_up(&normalized)) {
                self.composer_ui.emoji_words.asked = AskedWord::Idle;
                self.set_note(StatusNote::info(tr(keys::Q_CHAT_EMOJI_WORD_LOOKUP_FAILED)));
                cx.notify();
            }
            return;
        }
        self.composer_ui.emoji_words.asked = AskedWord::Idle;
        let word = Highlight {
            range,
            normalized,
            emoji: Vec::new(),
        };
        let items = self.word_strip_items(&word);
        if items.is_empty() {
            self.set_note(StatusNote::info(tr(keys::Q_CHAT_EMOJI_WORD_NONE)));
        } else {
            let ui = &mut self.screen.composer_ui.emoji_words;
            ui.unhover(&mut self.screen.composer_ui.suggest);
            ui.hovered = Some((word, items));
            ui.pinned = true;
        }
        cx.notify();
    }

    /// "Don't Suggest Emoji for This Word".
    pub(super) fn on_emoji_word_block(&mut self, action: &EmojiWordBlock, cx: &mut Context<Self>) {
        let word = action.word.clone();
        if !self.chat_prefs.emoji_word_blocklist.contains(&word) {
            self.set_chat_prefs(cx, |c| c.emoji_word_blocklist.push(word));
        }
        let ui = &mut self.screen.composer_ui.emoji_words;
        if ui
            .hovered
            .as_ref()
            .is_some_and(|(h, _)| h.normalized == action.word)
        {
            ui.unhover(&mut self.screen.composer_ui.suggest);
        }
        self.sync_emoji_words(cx);
        cx.notify();
    }

    /// "Turn Off Emoji Suggestions": the highlights go, and a note says
    /// where to turn them back on.
    pub(super) fn on_emoji_words_off(&mut self, cx: &mut Context<Self>) {
        self.set_chat_prefs(cx, |c| c.highlight_emoji_words = false);
        self.emoji_words_turned_off(cx);
    }

    /// After the highlights were turned off from the menu (the tests call
    /// it straight, so the shared test prefs file keeps them on).
    pub(super) fn emoji_words_turned_off(&mut self, cx: &mut Context<Self>) {
        self.screen
            .composer_ui
            .emoji_words
            .unhover(&mut self.screen.composer_ui.suggest);
        self.sync_emoji_words(cx);
        self.set_note(StatusNote::info(tr(keys::Q_CHAT_EMOJI_WORDS_OFF_NOTE)));
        cx.notify();
    }

    /// Settings › Chats: the words without emoji suggestions, each with a
    /// button that brings them back. None while the list is empty.
    pub(in crate::ui) fn emoji_word_blocklist_row(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let words = &self.chat_prefs.emoji_word_blocklist;
        if words.is_empty() || !self.chat_prefs.highlight_emoji_words {
            return None;
        }
        let theme = cx.theme();
        let chips = words.iter().enumerate().map(|(index, word)| {
            let remove = word.clone();
            div()
                .flex()
                .items_center()
                .gap_0p5()
                .pl_2()
                .pr_0p5()
                .py_0p5()
                .rounded_full()
                .bg(theme.secondary)
                .child(div().text_sm().child(word.clone()))
                .child(
                    Button::new(("emoji-word-unblock", index as u64))
                        .icon(IconName::Close)
                        .xsmall()
                        .ghost()
                        .accessibility_label(tr_with(
                            keys::Q_PREFS_EMOJI_WORD_UNBLOCK,
                            &[("word", word)],
                        ))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let word = remove.clone();
                            this.set_chat_prefs(cx, |c| {
                                c.emoji_word_blocklist.retain(|w| *w != word)
                            });
                            cx.notify();
                        })),
                )
        });
        Some(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .pl_4()
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(tr(keys::Q_PREFS_EMOJI_WORD_BLOCKLIST)),
                )
                .child(div().flex().flex_wrap().gap_1().children(chips))
                .into_any_element(),
        )
    }
}
