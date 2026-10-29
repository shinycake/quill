//! Composer `#hashtag` and `:emoji` autocomplete popups (Telegram Desktop
//! `FieldAutocomplete` hashtags / `Ui::Emoji::SuggestionsController`).
//!
//! The trigger rules and data live in `quill::suggest`; this file owns the
//! popup state, keyboard stepping, insertion and drawing. Keys are routed
//! from `app_demo.rs` next to the `@` mention menu.
#![deny(clippy::indexing_slicing)]

use super::app::QuillApp;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::composer_doc::ComposerTag;
use quill::emoji::keyword_lookup::{
    keyword_emoji, normalize_query, prioritize_recent, push_unique, skip_exact_keyword,
};
use quill::lang::{keys, tr, tr_with};
use quill::suggest::{
    EMOJI_LIMIT, RecentHashtags, SuggestKind, SuggestQuery, detect, emoji_name, search_emoji,
};
use quill::telegram::envelope_emoji::EmojiKeyword;
use std::time::Duration;

/// Emoji tiles shown before the strip scrolls (tdesktop
/// `kMaxNonScrolledEmoji`), their size and the gap between them.
const STRIP_VISIBLE: usize = 7;
const TILE: f32 = 36.;
const GAP: f32 = 4.;

/// Emoji one strip holds; a one-letter `:` query can match hundreds.
const MAX_ROWS: usize = 48;

/// tdesktop `kShowExactDelay`: a lone word waits this long before its
/// emoji show, so typing a first word does not flash the strip.
const SHOW_EXACT_DELAY: Duration = Duration::from_millis(300);

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
    /// The query at the caret, shown or not (its emoji may still be on
    /// the way, or a lone word may be waiting out [`SHOW_EXACT_DELAY`]).
    pub query: Option<SuggestQuery>,
    pub active: Option<ActiveSuggest>,
    /// The highlighted row; an exact-word strip starts with none, so
    /// Enter still sends (tdesktop `consumesEnter`).
    pub selected: Option<usize>,
    /// `(kind, trigger offset, query)` the user closed with Esc; stays
    /// closed until the query changes.
    pub dismissed: Option<(SuggestKind, usize, String)>,
    pub hashtags: RecentHashtags,
    /// The keyword answers' generation `active` was built from.
    built_generation: u64,
    /// The lone word whose delay ran out.
    word_ready: Option<SuggestQuery>,
    word_timer: Option<Task<()>>,
    pub strip_scroll: ScrollHandle,
}

impl SuggestUi {
    pub fn load() -> Self {
        Self {
            hashtags: quill::settings::load_recent_hashtags(&QuillApp::appearance_paths()),
            ..Self::default()
        }
    }
}

/// The text the keyword packs are searched with, and whether only whole
/// keywords count: a lone word, or `:D` / `:P` (tdesktop's "simple"
/// queries, one character with no lower case).
pub(super) fn keyword_query(query: &SuggestQuery) -> Option<(String, bool)> {
    let exact = match query.kind {
        SuggestKind::Hashtag => return None,
        SuggestKind::EmojiWord => true,
        SuggestKind::Emoji => {
            query.query.chars().count() == 1 && !query.query.chars().any(char::is_lowercase)
        }
    };
    Some((normalize_query(&query.query), exact))
}

fn has_skin_tone(emoji: &str) -> bool {
    emoji
        .chars()
        .any(|c| ('\u{1F3FB}'..='\u{1F3FF}').contains(&c))
}

/// The rows for `query`: recent hashtags, or the keyword matches (then,
/// for `:` queries, the catalog's names, tdesktop's legacy suggestions)
/// with recently used emoji first.
pub(super) fn suggest_items(
    query: &SuggestQuery,
    hashtags: &RecentHashtags,
    keywords: Option<&[EmojiKeyword]>,
    recent: &[String],
) -> Vec<SuggestItem> {
    if query.kind == SuggestKind::Hashtag {
        return hashtags
            .matching(&query.query)
            .into_iter()
            .map(|tag| SuggestItem {
                insert: format!("#{tag}"),
                glyph: String::new(),
                name: tag.to_string(),
            })
            .collect();
    }
    let Some((normalized, exact)) = keyword_query(query) else {
        return Vec::new();
    };
    let mut list = keyword_emoji(&normalized, exact, keywords.unwrap_or_default());
    let legacy = query
        .query
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | ' '));
    if !exact && legacy {
        for hit in search_emoji(&query.query, EMOJI_LIMIT) {
            push_unique(&mut list, hit.emoji);
        }
    }
    list.retain(|emoji| !has_skin_tone(emoji));
    prioritize_recent(list, recent)
        .into_iter()
        .take(MAX_ROWS)
        .map(|emoji| SuggestItem {
            name: emoji_name(&emoji).unwrap_or_default().to_string(),
            insert: emoji.clone(),
            glyph: emoji,
        })
        .collect()
}

/// tdesktop `SuggestionsWidget::handleKeyEvent` for the arrows: nothing
/// highlighted and stepping back is not consumed; stepping back off the
/// first row clears the highlight, forward off the last wraps to the first.
pub(super) fn step_selection(
    selected: Option<usize>,
    delta: i32,
    rows: usize,
) -> Option<Option<usize>> {
    if rows == 0 || (delta < 0 && selected.is_none()) {
        return None;
    }
    let rows = rows as i64;
    let start = selected
        .map(|s| s as i64)
        .filter(|s| *s < rows)
        .unwrap_or(if delta > 0 { rows - 1 } else { 0 });
    let next = start + i64::from(delta);
    Some(if next < 0 {
        None
    } else {
        Some((next % rows) as usize)
    })
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
    let skip = usize::from(query.kind != SuggestKind::EmojiWord);
    if typed.chars().skip(skip).collect::<String>() != query.query {
        return None;
    }
    let mut insert = item.insert.clone();
    if query.kind == SuggestKind::Hashtag
        && !text
            .get(query.range.end..)
            .unwrap_or_default()
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
    {
        insert.push(' ');
    }
    let caret = query.range.start + insert.len();
    let mut out = String::with_capacity(text.len() + insert.len());
    out.push_str(text.get(..query.range.start).unwrap_or_default());
    out.push_str(&insert);
    out.push_str(text.get(query.range.end..).unwrap_or_default());
    Some((out, caret))
}

impl QuillApp {
    /// Re-detect the trigger at the caret and refresh the rows. Runs on
    /// every composer input event (`typed`) and after programmatic edits,
    /// which only keep a popup already open (tdesktop shows suggestions
    /// for text changed by a key press).
    pub(super) fn sync_suggest_menu(&mut self, typed: bool, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).value().to_string();
        let range = self.composer.read(cx).selected_range();
        let mention = self.mention_trigger(cx).is_some();
        let ui = &self.composer_ui.suggest;
        let query = (range.is_empty() && !mention && self.composer_ui.pending_edit.is_none())
            .then(|| detect(&text, range.start, self.chat_prefs.suggest_emoji))
            .flatten()
            .filter(|q| ui.dismissed.as_ref() != Some(&(q.kind, q.range.start, q.query.clone())))
            .filter(|q| typed || ui.query.as_ref() == Some(q));
        if self.composer_ui.suggest.query != query {
            let shown = self.composer_ui.suggest.active.is_some();
            let ui = &mut self.composer_ui.suggest;
            ui.selected = None;
            ui.word_timer = None;
            if ui.word_ready.as_ref() != query.as_ref() {
                ui.word_ready = None;
            }
            if let Some(q) = query.as_ref().filter(|q| q.kind == SuggestKind::EmojiWord) {
                if shown {
                    ui.word_ready = Some(q.clone());
                } else {
                    let waiting = q.clone();
                    ui.word_timer = Some(cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(SHOW_EXACT_DELAY).await;
                        let _ = this.update(cx, |this, cx| {
                            let ui = &mut this.composer_ui.suggest;
                            if ui.query.as_ref() == Some(&waiting) {
                                ui.word_ready = Some(waiting);
                                this.ask_suggest_keywords();
                                this.rebuild_suggest_menu(cx);
                            }
                        });
                    }));
                }
            }
            ui.query = query;
            self.ask_suggest_keywords();
        }
        self.rebuild_suggest_menu(cx);
    }

    /// Ask TDLib for the keywords of the query at the caret, unless they
    /// are known or on the way. A lone word asks only once its delay ran
    /// out, so typing a short message does not search every prefix.
    fn ask_suggest_keywords(&mut self) {
        let ui = &self.composer_ui.suggest;
        let Some(query) = ui.query.as_ref() else {
            return;
        };
        if query.kind == SuggestKind::EmojiWord && ui.word_ready.as_ref() != Some(query) {
            return;
        }
        let Some((normalized, exact)) = keyword_query(query) else {
            return;
        };
        if exact && skip_exact_keyword(&normalized) {
            return;
        }
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.lookup_emoji_keywords([normalized.as_str()]);
        }
    }

    /// The keyword answers changed since the popup was built: rebuild it.
    /// Called every frame; an integer compare while nothing is pending.
    pub(super) fn refresh_suggest_menu(&mut self, cx: &mut Context<Self>) {
        let generation = self.keyword_generation();
        if self.composer_ui.suggest.query.is_some()
            && generation != self.composer_ui.suggest.built_generation
        {
            // A slot freed: a query the in-flight cap turned away goes now.
            self.ask_suggest_keywords();
            self.rebuild_suggest_menu(cx);
        }
    }

    fn keyword_generation(&self) -> u64 {
        self.session()
            .map_or(0, |s| s.stickers.emoji.keyword_lookups.generation)
    }

    /// `active` from the query at the caret and what is known about it.
    fn rebuild_suggest_menu(&mut self, cx: &mut Context<Self>) {
        let generation = self.keyword_generation();
        let ui = &self.composer_ui.suggest;
        let active = ui
            .query
            .as_ref()
            .filter(|q| q.kind != SuggestKind::EmojiWord || ui.word_ready.as_ref() == Some(q))
            .and_then(|query| {
                let session = self.session();
                let keywords = keyword_query(query).and_then(|(normalized, _)| {
                    session.and_then(|s| s.stickers.emoji.keyword_lookups.answer(&normalized))
                });
                let recent = session
                    .map(|s| s.settings.media_prefs.recent_emoji.as_slice())
                    .unwrap_or_default();
                let items = suggest_items(query, &ui.hashtags, keywords, recent);
                (!items.is_empty()).then(|| ActiveSuggest {
                    query: query.clone(),
                    items,
                })
            });
        let ui = &mut self.composer_ui.suggest;
        ui.built_generation = generation;
        // `:` and `#` queries highlight their first row
        // (`selectFirstResult`); a lone word starts with none.
        let first = active
            .as_ref()
            .is_some_and(|a| a.query.kind != SuggestKind::EmojiWord);
        // A new list for the same query keeps the highlighted emoji, not
        // its index, so Enter inserts what the user picked.
        let kept = ui
            .selected
            .and_then(|at| ui.active.as_ref()?.items.get(at))
            .and_then(|old| active.as_ref()?.items.iter().position(|i| i == old));
        ui.selected = match (&active, kept) {
            (None, _) => None,
            (Some(_), Some(at)) => Some(at),
            (Some(_), None) => first.then_some(0),
        };
        if ui.active != active {
            ui.active = active;
            ui.strip_scroll.set_offset(point(px(0.), px(0.)));
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
        let prev = std::mem::replace(&mut self.composer_ui.prev_text, text.to_string());
        let range = self.composer.read(cx).selected_range();
        if !range.is_empty() {
            return None;
        }
        let replace = quill::emoji_replace::instant_replacement(
            &prev,
            text,
            range.start,
            self.chat_prefs.replace_emoji,
        )?;
        if self.composer_caret_in_code(range.start, cx) {
            return None;
        }
        let original = text.get(replace.range.clone())?.to_string();
        // A range edit keeps the formatting around it (codex:composer-input).
        self.replace_composer_text(replace.range.clone(), &replace.with, window, cx);
        let new_text = self.composer.read(cx).value().to_string();
        self.composer_ui.prev_text = new_text.clone();
        // Backspace right after puts the typed text back
        // (`revertFormatReplace`).
        self.composer_ui.markdown_revert = Some(super::composer_field::MarkdownRevert {
            text: new_text.clone(),
            caret: self.composer.read(cx).selected_range().start,
            restore: Some((
                replace.range.start..replace.range.start + replace.with.len(),
                original,
            )),
        });
        Some(new_text)
    }

    /// Whether `caret` is inside inline code or a code block: no instant
    /// or system replacement happens there (`processInstantReplaces` and
    /// `processSystemTextReplaces` return early over `kTagCode` and
    /// `IsTagPre`).
    pub(super) fn composer_caret_in_code(&self, caret: usize, cx: &App) -> bool {
        super::composer_field::field_doc(self.composer.read(cx))
            .spans
            .iter()
            .any(|span| {
                matches!(span.tag, ComposerTag::Code | ComposerTag::Pre(_))
                    && span.range.start < caret
                    && caret <= span.range.end
            })
    }

    /// Esc / blur: close the popup. True when it was showing.
    pub(super) fn close_suggest_menu(&mut self, remember: bool, cx: &mut Context<Self>) -> bool {
        let ui = &mut self.composer_ui.suggest;
        ui.word_timer = None;
        let Some(active) = ui.active.take() else {
            ui.query = None;
            return false;
        };
        if remember {
            ui.dismissed = Some((
                active.query.kind,
                active.query.range.start,
                active.query.query,
            ));
        }
        ui.query = None;
        ui.selected = None;
        cx.notify();
        true
    }

    /// Up/Down (and Left/Right for the emoji strip) through the rows.
    /// True when consumed. Hashtags wrap both ways; the emoji strip
    /// follows tdesktop ([`step_selection`]).
    pub(super) fn step_suggest_menu(
        &mut self,
        delta: i32,
        horizontal: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let ui = &mut self.composer_ui.suggest;
        let Some(active) = ui.active.as_ref() else {
            return false;
        };
        let rows = active.items.len();
        let next = if active.query.kind == SuggestKind::Hashtag {
            if horizontal {
                return false;
            }
            let at = ui.selected.unwrap_or(0) as i64 + i64::from(delta);
            Some(at.rem_euclid(rows as i64) as usize)
        } else {
            match step_selection(ui.selected, delta, rows) {
                Some(next) => next,
                None => return false,
            }
        };
        ui.selected = next;
        if let Some(at) = next {
            ui.strip_scroll.scroll_to_item(at);
        }
        cx.notify();
        true
    }

    /// Enter: insert the highlighted row; Tab: the highlighted or the
    /// first. True when consumed.
    pub(super) fn pick_suggest_selection(
        &mut self,
        tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(active) = self.composer_ui.suggest.active.as_ref() else {
            return false;
        };
        let index = match self.composer_ui.suggest.selected {
            // `active` is never built without items.
            Some(at) => at.min(active.items.len().saturating_sub(1)),
            None if tab => 0,
            None => return false,
        };
        self.pick_suggest_index(index, window, cx);
        true
    }

    pub(super) fn pick_suggest_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ui = &mut self.composer_ui.suggest;
        ui.query = None;
        ui.word_timer = None;
        let Some(active) = ui.active.take() else {
            return;
        };
        ui.selected = None;
        let Some(item) = active.items.get(index) else {
            return;
        };
        let text = self.composer.read(cx).value().to_string();
        let Some((new_text, caret)) = apply_suggestion(&text, &active.query, item) else {
            cx.notify();
            return;
        };
        // A range edit keeps the formatting around it (codex:composer-input).
        let insert = new_text
            .get(active.query.range.start..caret)
            .unwrap_or_default()
            .to_string();
        self.replace_composer_text(active.query.range.clone(), &insert, window, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
        let new_text = self.composer.read(cx).value().to_string();
        self.sync_composer_typing(&new_text);
        cx.notify();
    }

    /// Remember the hashtags of a message that is being sent.
    pub(super) fn remember_sent_hashtags(&mut self, text: &str) {
        if !self.composer_ui.suggest.hashtags.record_message(text) {
            return;
        }
        if let Err(err) = quill::settings::save_recent_hashtags(
            &Self::appearance_paths(),
            &self.composer_ui.suggest.hashtags,
        ) {
            self.connection.set_note(StatusNote::error(tr_with(
                keys::Q_CHAT_COULDNT_SAVE_RECENT_HASHTAGS,
                &[("err", &err.to_string())],
            )));
        }
    }

    /// The popup above the composer: a list of hashtags, or a strip of
    /// emoji tiles with the highlighted one's name.
    pub(super) fn suggest_menu_dropdown(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.composer_ui.suggest.active.as_ref()?;
        self.suggest_menu_dropdown_body(cx)
    }

    /// `suggest_menu_dropdown` past its cheap check. A debug build reserves this
    /// builder's whole frame on entry, so frames that show nothing skip
    /// it (docs/decisions/main-thread-stack.md).
    /// The outer function repeats this body's first check; keep the two in step.
    #[inline(never)]
    fn suggest_menu_dropdown_body(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let active = self.composer_ui.suggest.active.as_ref()?;
        let selected = self.composer_ui.suggest.selected;
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
                    .aria_label(tr(keys::Q_CHAT_HASHTAG_SUGGESTIONS))
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
                            .when(Some(index) == selected, |this| {
                                this.bg(cx.theme().selection)
                            })
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
            SuggestKind::Emoji | SuggestKind::EmojiWord => {
                let mut row = div()
                    .id("suggest-strip")
                    .flex()
                    .items_center()
                    .gap_1()
                    .max_w(px(STRIP_VISIBLE as f32 * (TILE + GAP)))
                    .overflow_x_scroll()
                    .track_scroll(&self.composer_ui.suggest.strip_scroll);
                for (index, item) in active.items.iter().enumerate() {
                    row = row.child(
                        div()
                            .id(("suggest-item", index as u64))
                            .flex_none()
                            .size(px(TILE))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_md()
                            .text_size(px(22.))
                            .role(gpui_kit::Role::ListBoxOption)
                            .aria_label(if item.name.is_empty() {
                                item.glyph.clone()
                            } else {
                                item.name.clone()
                            })
                            .cursor_pointer()
                            .when(Some(index) == selected, |this| {
                                this.bg(cx.theme().selection)
                            })
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
                    .aria_label(tr(keys::Q_CHAT_EMOJI_SUGGESTIONS))
                    .self_start()
                    .child(row)
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::string_slice, reason = "test code slices known-good literals")]
    use super::{SuggestItem, apply_suggestion, step_selection, suggest_items};
    use quill::suggest::{RecentHashtags, SuggestKind, detect};
    use quill::telegram::envelope_emoji::EmojiKeyword;

    fn kw(emoji: &str, keyword: &str) -> EmojiKeyword {
        EmojiKeyword {
            emoji: emoji.into(),
            keyword: keyword.into(),
        }
    }

    fn glyphs(text: &str, keywords: &[EmojiKeyword], recent: &[String]) -> Vec<String> {
        let query = detect(text, text.len(), true).unwrap();
        suggest_items(&query, &RecentHashtags::default(), Some(keywords), recent)
            .into_iter()
            .map(|item| item.glyph)
            .collect()
    }

    fn pick(
        text: &str,
        caret: usize,
        index: usize,
        tags: &RecentHashtags,
    ) -> Option<(String, usize)> {
        let query = detect(text, caret, true)?;
        let items = suggest_items(&query, tags, None, &[]);
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
        let item: SuggestItem = suggest_items(&query, &tags, None, &[]).remove(0);
        assert_eq!(query.kind, SuggestKind::Hashtag);
        assert!(apply_suggestion("go #xyz", &query, &item).is_none());
    }

    #[test]
    fn colon_queries_lead_with_keywords_then_names() {
        let answer = [kw("😂", "happy"), kw("😊", "happy"), kw("😃", "happiness")];
        let found = glyphs("this is an example :happy", &answer, &[]);
        assert_eq!(found[..2], ["😂", "😊"]);
        // `:happ` also takes longer keywords, in keyword order.
        assert_eq!(glyphs("x :happ", &answer, &[])[..3], ["😃", "😂", "😊"]);
        // Catalog names follow for ASCII queries (`:fire` with no answer).
        assert_eq!(
            glyphs("hi :fire", &[], &[]).first().map(String::as_str),
            Some("🔥")
        );
        // Recently used emoji come first.
        assert_eq!(
            glyphs("x :happy", &answer, &["😊".into()])[..2],
            ["😊", "😂"]
        );
    }

    #[test]
    fn a_lone_word_takes_only_whole_keywords() {
        let answer = [kw("😂", "funny"), kw("🤡", "funnyman")];
        assert_eq!(glyphs("funny", &answer, &[]), ["😂"]);
        assert!(glyphs("fun", &answer, &[]).is_empty());
        // tdesktop skips one- and most two-letter English words.
        assert!(glyphs("be", &[kw("🐝", "be")], &[]).is_empty());
        assert_eq!(glyphs("ok", &[kw("👌", "ok")], &[]), ["👌"]);
    }

    #[test]
    fn picking_a_word_suggestion_replaces_the_word() {
        let query = detect("funny", 5, true).unwrap();
        let items = suggest_items(
            &query,
            &RecentHashtags::default(),
            Some(&[kw("😂", "funny")]),
            &[],
        );
        assert_eq!(
            apply_suggestion("funny", &query, &items[0]),
            Some(("😂".into(), 4))
        );
    }

    #[test]
    fn strip_selection_follows_tdesktop_keys() {
        // Nothing highlighted: back is not consumed, forward takes the first.
        assert_eq!(step_selection(None, -1, 3), None);
        assert_eq!(step_selection(None, 1, 3), Some(Some(0)));
        // Back off the first clears; forward off the last wraps.
        assert_eq!(step_selection(Some(0), -1, 3), Some(None));
        assert_eq!(step_selection(Some(2), 1, 3), Some(Some(0)));
        assert_eq!(step_selection(Some(1), -1, 3), Some(Some(0)));
        assert_eq!(step_selection(Some(0), 1, 0), None);
    }
}
