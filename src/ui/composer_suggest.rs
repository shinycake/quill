//! Composer `#hashtag` and `:emoji` autocomplete popups (Telegram Desktop
//! `FieldAutocomplete` hashtags / `Ui::Emoji::SuggestionsController`).
//!
//! The trigger rules and data live in `quill::suggest`; this file owns the
//! popup state, keyboard stepping, insertion and drawing. Keys are routed
//! from `app_demo.rs` next to the `@` mention menu.

use super::app::QuillApp;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::suggest::{
    EMOJI_LIMIT, RecentHashtags, SuggestKind, SuggestQuery, detect, search_emoji,
};

/// One row of the popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SuggestItem {
    /// Text that replaces the typed trigger and query.
    pub insert: String,
    /// The emoji, empty for hashtags.
    pub glyph: String,
    /// Hashtag without `#`, or the emoji's name.
    pub name: String,
}

/// The popup currently offered for the text at the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ActiveSuggest {
    pub query: SuggestQuery,
    pub items: Vec<SuggestItem>,
}

/// Popup state kept on [`QuillApp`].
#[derive(Default)]
pub(super) struct SuggestUi {
    pub active: Option<ActiveSuggest>,
    pub selected: usize,
    /// `(kind, trigger offset, query)` the user closed with Esc; stays
    /// closed until the query changes.
    pub dismissed: Option<(SuggestKind, usize, String)>,
    pub hashtags: RecentHashtags,
}

impl SuggestUi {
    pub fn load() -> Self {
        Self {
            hashtags: quill::settings::load_recent_hashtags(&QuillApp::appearance_paths()),
            ..Self::default()
        }
    }
}

/// The rows for `query`: recent hashtags or matching emoji.
pub(super) fn suggest_items(query: &SuggestQuery, hashtags: &RecentHashtags) -> Vec<SuggestItem> {
    match query.kind {
        SuggestKind::Hashtag => hashtags
            .matching(&query.query)
            .into_iter()
            .map(|tag| SuggestItem {
                insert: format!("#{tag}"),
                glyph: String::new(),
                name: tag.to_string(),
            })
            .collect(),
        SuggestKind::Emoji => search_emoji(&query.query, EMOJI_LIMIT)
            .into_iter()
            .map(|hit| SuggestItem {
                insert: hit.emoji.to_string(),
                glyph: hit.emoji.to_string(),
                name: hit.name.to_string(),
            })
            .collect(),
    }
}

/// `text` with the trigger+query at `query.range` replaced by `item`;
/// also the caret after it. A hashtag gets a trailing space (tdesktop
/// `insertTag`) unless whitespace already follows.
pub(super) fn apply_suggestion(
    text: &str,
    query: &SuggestQuery,
    item: &SuggestItem,
) -> Option<(String, usize)> {
    let typed = text.get(query.range.clone())?;
    if typed.chars().skip(1).collect::<String>() != query.query {
        return None;
    }
    let mut insert = item.insert.clone();
    if query.kind == SuggestKind::Hashtag
        && !text[query.range.end..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
    {
        insert.push(' ');
    }
    let caret = query.range.start + insert.len();
    let mut out = String::with_capacity(text.len() + insert.len());
    out.push_str(&text[..query.range.start]);
    out.push_str(&insert);
    out.push_str(&text[query.range.end..]);
    Some((out, caret))
}

impl QuillApp {
    /// Re-detect the trigger at the caret and refresh the rows. Runs on
    /// every composer input event; cheap (the emoji index is built once).
    pub(super) fn sync_suggest_menu(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        let range = self.composer.read(cx).selected_range();
        let mention = quill::composer::mention_trigger(&text).is_some();
        let query = (range.is_empty() && !mention && self.pending_edit.is_none())
            .then(|| detect(&text, range.start, self.chat_prefs.suggest_emoji))
            .flatten()
            .filter(|q| {
                self.suggest.dismissed.as_ref() != Some(&(q.kind, q.range.start, q.query.clone()))
            });
        let active = query.and_then(|query| {
            let items = suggest_items(&query, &self.suggest.hashtags);
            (!items.is_empty()).then_some(ActiveSuggest { query, items })
        });
        if self.suggest.active.as_ref().map(|a| &a.query) != active.as_ref().map(|a| &a.query) {
            self.suggest.selected = 0;
        }
        if self.suggest.active != active {
            self.suggest.active = active;
            cx.notify();
        }
    }

    /// "Replace emoji automatically": after a keystroke, swap a finished
    /// trigger (`:-)`, `<3`, `:rocket:`) for its emoji. Returns the new
    /// text when it replaced something. `set_value` emits no Change, so
    /// this does not recurse.
    pub(super) fn apply_instant_replace(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let prev = std::mem::replace(&mut self.composer_prev_text, text.to_string());
        let range = self.composer.read(cx).selected_range();
        if !range.is_empty() || self.pending_edit.is_some() {
            return None;
        }
        let replace = quill::emoji_replace::instant_replacement(
            &prev,
            text,
            range.start,
            self.chat_prefs.replace_emoji,
        )?;
        // A range edit keeps the formatting around it (codex:composer-input).
        self.replace_composer_text(replace.range.clone(), &replace.with, window, cx);
        let new_text = self.composer.read(cx).value().to_string();
        self.composer_prev_text = new_text.clone();
        Some(new_text)
    }

    /// Esc / blur: close the popup. True when it was showing.
    pub(super) fn close_suggest_menu(&mut self, remember: bool, cx: &mut Context<Self>) -> bool {
        let Some(active) = self.suggest.active.take() else {
            return false;
        };
        if remember {
            self.suggest.dismissed = Some((
                active.query.kind,
                active.query.range.start,
                active.query.query,
            ));
        }
        self.suggest.selected = 0;
        cx.notify();
        true
    }

    /// Up/Down (and Left/Right for the emoji strip) through the rows,
    /// wrapping. True when consumed.
    pub(super) fn step_suggest_menu(
        &mut self,
        delta: i32,
        horizontal: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(active) = self.suggest.active.as_ref() else {
            return false;
        };
        if horizontal && active.query.kind != SuggestKind::Emoji {
            return false;
        }
        let rows = active.items.len() as i32;
        self.suggest.selected = (self.suggest.selected as i32 + delta).rem_euclid(rows) as usize;
        cx.notify();
        true
    }

    /// Enter / Tab: insert the highlighted row. True when consumed.
    pub(super) fn pick_suggest_selection(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(active) = self.suggest.active.as_ref() else {
            return false;
        };
        let index = self.suggest.selected.min(active.items.len() - 1);
        self.pick_suggest_index(index, window, cx);
        true
    }

    pub(super) fn pick_suggest_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(active) = self.suggest.active.take() else {
            return;
        };
        self.suggest.selected = 0;
        let Some(item) = active.items.get(index) else {
            return;
        };
        let text = self.composer.read(cx).value().to_string();
        let Some((new_text, caret)) = apply_suggestion(&text, &active.query, item) else {
            cx.notify();
            return;
        };
        // A range edit keeps the formatting around it (codex:composer-input).
        let insert = new_text[active.query.range.start..caret].to_string();
        self.replace_composer_text(active.query.range.clone(), &insert, window, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        let new_text = self.composer.read(cx).value().to_string();
        self.sync_composer_typing(&new_text);
        cx.notify();
    }

    /// Remember the hashtags of a message that is being sent.
    pub(super) fn remember_sent_hashtags(&mut self, text: &str) {
        if !self.suggest.hashtags.record_message(text) {
            return;
        }
        if let Err(err) =
            quill::settings::save_recent_hashtags(&Self::appearance_paths(), &self.suggest.hashtags)
        {
            self.status_note = format!("Couldn't save recent hashtags: {err}");
        }
    }

    /// The popup above the composer: a list of hashtags, or a strip of
    /// emoji tiles with the highlighted one's name.
    pub(super) fn suggest_menu_dropdown(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let active = self.suggest.active.as_ref()?;
        let selected = self.suggest.selected.min(active.items.len() - 1);
        let panel = div()
            .id("suggest-menu")
            .role(gpui_kit::Role::ListBox)
            .flex()
            .mx_4()
            .mb_2()
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .shadow_md();
        let typed = active.query.query.chars().count();
        Some(IntoElement::into_any_element(match active.query.kind {
            SuggestKind::Hashtag => {
                let mut list = panel
                    .aria_label("Hashtag suggestions")
                    .flex_col()
                    .max_w(px(440.));
                for (index, item) in active.items.iter().enumerate() {
                    let head: String = item.name.chars().take(typed).collect();
                    let tail: String = item.name.chars().skip(typed).collect();
                    list = list.child(
                        div()
                            .id(("suggest-item", index as u64))
                            .flex()
                            .items_center()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .role(gpui_kit::Role::ListBoxOption)
                            .aria_label(format!("#{}", item.name))
                            .cursor_pointer()
                            .when(index == selected, |this| this.bg(cx.theme().selection))
                            .hover(|style| style.bg(cx.theme().accent))
                            // On press: the composer's blur closes the
                            // popup before a click could land.
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    window.prevent_default();
                                    this.pick_suggest_index(index, window, cx);
                                }),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(cx.theme().primary)
                                    .child(format!("#{head}")),
                            )
                            .child(div().text_sm().child(tail)),
                    );
                }
                list
            }
            SuggestKind::Emoji => {
                let mut row = div().flex().items_center().gap_1();
                for (index, item) in active.items.iter().enumerate() {
                    row = row.child(
                        div()
                            .id(("suggest-item", index as u64))
                            .size(px(36.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .text_size(px(22.))
                            .role(gpui_kit::Role::ListBoxOption)
                            .aria_label(item.name.clone())
                            .cursor_pointer()
                            .when(index == selected, |this| this.bg(cx.theme().selection))
                            .hover(|style| style.bg(cx.theme().accent))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    window.prevent_default();
                                    this.pick_suggest_index(index, window, cx);
                                }),
                            )
                            .child(item.glyph.clone()),
                    );
                }
                panel
                    .aria_label("Emoji suggestions")
                    .flex_col()
                    .max_w(px(440.))
                    .child(row)
                    .child(
                        div()
                            .px_2()
                            .pb_1()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(format!(":{}", active.items[selected].name)),
                    )
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{SuggestItem, apply_suggestion, suggest_items};
    use quill::suggest::{RecentHashtags, SuggestKind, detect};

    fn pick(
        text: &str,
        caret: usize,
        index: usize,
        tags: &RecentHashtags,
    ) -> Option<(String, usize)> {
        let query = detect(text, caret, true)?;
        let items = suggest_items(&query, tags);
        apply_suggestion(text, &query, items.get(index)?)
    }

    #[test]
    fn emoji_replaces_the_typed_colon_query() {
        let tags = RecentHashtags::default();
        let (out, caret) = pick("hi :fire", 8, 0, &tags).unwrap();
        assert_eq!(out, "hi 🔥");
        assert_eq!(caret, out.len());
        // Mid-text: the tail stays and the caret lands after the emoji.
        let (out, caret) = pick("a :fire b", 7, 0, &tags).unwrap();
        assert_eq!(out, "a 🔥 b");
        assert_eq!(&out[caret..], " b");
    }

    #[test]
    fn hashtag_insert_adds_a_space_unless_one_follows() {
        let mut tags = RecentHashtags::default();
        tags.record_message("#rustacean");
        assert_eq!(pick("go #ru", 6, 0, &tags).unwrap().0, "go #rustacean ");
        assert_eq!(pick("go #ru x", 6, 0, &tags).unwrap().0, "go #rustacean x");
        assert!(pick("go #zz", 6, 0, &tags).is_none());
    }

    #[test]
    fn stale_queries_do_not_apply() {
        let mut tags = RecentHashtags::default();
        tags.record_message("#rustacean");
        let query = detect("go #ru", 6, true).unwrap();
        let item: SuggestItem = suggest_items(&query, &tags).remove(0);
        assert_eq!(query.kind, SuggestKind::Hashtag);
        assert!(apply_suggestion("go #xyz", &query, &item).is_none());
    }
}
