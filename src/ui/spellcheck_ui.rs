//! Spellcheck corrections UI (parity:platform-spellcheck): the "ABC n"
//! badge button in the composer input row and the corrections panel
//! above the composer.
//!
//! The kit Textarea is a closed component (no inline-underline hooks),
//! so corrections live in a panel: each misspelled word with suggestion
//! buttons, Ignore (session-only), and Add to dictionary (persisted).

use super::app::QuillApp;
use super::chat_theme::{bg_canvas, border, danger, text_muted, text_primary};
use super::*;
use gpui_kit::component::button::*;
use gpui_kit::component::*;
use gpui_kit::*;

/// Max misspellings rendered in the panel (the engine caps at
/// `MAX_MISSPELLINGS`; the badge counts all of them).
const PANEL_WORD_LIMIT: usize = 8;
/// Suggestions offered per misspelled word.
const SUGGESTION_LIMIT: usize = 5;

impl QuillApp {
    /// Cheap per-input-event sync: re-check the draft text, notify only
    /// when the misspelling list changed. Suggestions are NOT refreshed
    /// here (distance-2 generation is too slow for the input path) —
    /// only when the panel is open and the word set changed.
    pub(super) fn sync_spellcheck(&mut self, text: &str, cx: &mut Context<Self>) {
        if !self.chat_prefs.spellcheck_enabled {
            if !self.spell_misspellings.is_empty() || self.spellcheck_open {
                self.spell_misspellings.clear();
                self.spell_suggestions.clear();
                self.spellcheck_open = false;
                cx.notify();
            }
            return;
        }
        let next = self.spellchecker.check_words(text);
        if next == self.spell_misspellings {
            return;
        }
        let words: Vec<&str> = next.iter().map(|m| m.word.as_str()).collect();
        let old_words: Vec<&str> = self
            .spell_misspellings
            .iter()
            .map(|m| m.word.as_str())
            .collect();
        self.spell_misspellings = next;
        if self.spellcheck_open && words != old_words {
            self.refresh_spell_suggestions();
        }
        cx.notify();
    }

    /// Recompute suggestions for the current misspellings (panel-open
    /// path only — never per keystroke).
    fn refresh_spell_suggestions(&mut self) {
        self.spell_suggestions = self
            .spell_misspellings
            .iter()
            .take(PANEL_WORD_LIMIT)
            .map(|m| self.spellchecker.suggestions(&m.word, SUGGESTION_LIMIT))
            .collect();
    }

    /// Open/close the corrections panel (the badge button).
    pub(super) fn toggle_spellcheck_panel(&mut self, cx: &mut Context<Self>) {
        self.spellcheck_open = !self.spellcheck_open;
        if self.spellcheck_open {
            self.refresh_spell_suggestions();
        }
        cx.notify();
    }

    /// Replace the idx-th misspelling with `suggestion` in the draft,
    /// keeping the cursor just after the replacement (same pattern as
    /// the format-markup actions in composer.rs).
    pub(super) fn apply_spelling_suggestion(
        &mut self,
        idx: usize,
        suggestion: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (start, end, word) = match self.spell_misspellings.get(idx) {
            Some(m) => (m.start, m.end, m.word.clone()),
            None => return,
        };
        let text = self.composer.read(cx).value().to_string();
        // The draft may have changed since the check — verify the byte
        // range still holds the word; otherwise just re-sync.
        if text.get(start..end) != Some(word.as_str()) {
            self.sync_spellcheck(&text, cx);
            return;
        }
        let mut new_text = String::with_capacity(text.len() + suggestion.len());
        new_text.push_str(&text[..start]);
        new_text.push_str(suggestion);
        new_text.push_str(&text[end..]);
        let cursor = start + suggestion.len();
        self.composer.update(cx, |input, cx| {
            input.set_value(&new_text, window, cx);
            input.set_selected_range(cursor..cursor, cx);
        });
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        self.sync_spellcheck(&new_text, cx);
    }

    /// "Ignore" — session-only, cleared on send/chat switch.
    pub(super) fn ignore_misspelling(&mut self, idx: usize, cx: &mut Context<Self>) {
        let word = match self.spell_misspellings.get(idx) {
            Some(m) => m.word.clone(),
            None => return,
        };
        self.spellchecker.ignore_word(&word);
        let text = self.composer.read(cx).value().to_string();
        self.sync_spellcheck(&text, cx);
    }

    /// "Add to dictionary" — persisted to spellcheck_words.json.
    pub(super) fn add_misspelling_to_dictionary(&mut self, idx: usize, cx: &mut Context<Self>) {
        let word = match self.spell_misspellings.get(idx) {
            Some(m) => m.word.clone(),
            None => return,
        };
        if self.spellchecker.add_custom_word(&word) {
            let prefs = quill::settings::SpellcheckWords {
                words: self.spellchecker.custom_words(),
            };
            if let Err(err) =
                quill::settings::save_spellcheck_words(&Self::appearance_paths(), &prefs)
            {
                self.status_note = format!("Couldn't save dictionary: {err}");
            }
        }
        let text = self.composer.read(cx).value().to_string();
        self.sync_spellcheck(&text, cx);
    }

    /// The "ABC n" badge in the composer input row — only rendered while
    /// the draft has misspellings.
    pub(super) fn spellcheck_badge(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.chat_prefs.spellcheck_enabled || self.spell_misspellings.is_empty() {
            return None;
        }
        let n = self.spell_misspellings.len();
        Some(
            Button::new("composer-spellcheck")
                .label(format!("ABC {n}"))
                .tooltip(format!(
                    "Check spelling ({} flagged)",
                    if n == 1 {
                        "1 word"
                    } else {
                        &format!("{n} words")
                    }
                ))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.toggle_spellcheck_panel(cx);
                }))
                .into_any_element(),
        )
    }

    /// The corrections panel above the composer.
    pub(super) fn spellcheck_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut panel = div()
            .id("spellcheck-panel")
            .flex()
            .flex_col()
            .gap_2()
            .px_3()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(border())
            .bg(bg_canvas())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(text_primary())
                            .child("Spelling"),
                    )
                    .child(
                        Button::new("spellcheck-close")
                            .label("Close")
                            .ghost()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.spellcheck_open = false;
                                cx.notify();
                            })),
                    ),
            );
        if self.spell_misspellings.is_empty() {
            panel = panel.child(
                div()
                    .text_sm()
                    .text_color(text_muted())
                    .child("No spelling issues in this draft."),
            );
        }
        for (i, m) in self
            .spell_misspellings
            .iter()
            .enumerate()
            .take(PANEL_WORD_LIMIT)
        {
            let suggestions = self.spell_suggestions.get(i).cloned().unwrap_or_default();
            let mut row = div()
                .id(format!("spellcheck-row-{i}"))
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .text_sm()
                        .font_semibold()
                        .text_color(danger())
                        .child(m.word.clone()),
                );
            let mut sug_row = div().flex().flex_wrap().gap_1();
            for s in &suggestions {
                let sug = s.clone();
                sug_row = sug_row.child(
                    Button::new(format!("spellcheck-sug-{i}-{s}"))
                        .label(s.clone())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.apply_spelling_suggestion(i, &sug, window, cx);
                        })),
                );
            }
            row = row.child(sug_row);
            row = row.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new(format!("spellcheck-ignore-{i}"))
                            .label("Ignore")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.ignore_misspelling(i, cx);
                            })),
                    )
                    .child(
                        Button::new(format!("spellcheck-add-{i}"))
                            .label("Add to dictionary")
                            .ghost()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.add_misspelling_to_dictionary(i, cx);
                            })),
                    ),
            );
            panel = panel.child(row);
        }
        panel.into_any_element()
    }
}
