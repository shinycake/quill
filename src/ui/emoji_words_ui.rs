//! Composer words that have an emoji (`quill::emoji_words`): a soft
//! gradient highlight painted over the Textarea (it has no decoration
//! hooks, like the spelling underlines), a one-time glimmer when a word
//! first lights up, and on hover the same strip `:word` would offer.
//!
//! Cost: the words are picked again only when the text, the selection,
//! the formatting, the keyword answers or the setting change (a compare
//! of the borrowed text per frame otherwise, no copy); the overlay paints
//! with the composer and asks for frames only while a glimmer runs.
#![deny(clippy::indexing_slicing)]

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::app::QuillApp;
use super::composer_suggest::{GAP, STRIP_VISIBLE, SuggestItem, TILE, emoji_tiles, suggest_items};
use gpui_kit::component::input::{Rope, TextareaState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer_doc::ComposerTag;
use quill::emoji_words::{Highlight, candidates, pick};
use quill::lang::{keys, tr};
use quill::suggest::{RecentHashtags, SuggestKind, SuggestQuery};

/// How long a word's glimmer runs when it first lights up.
const GLIMMER: Duration = Duration::from_millis(900);

/// What the highlights were picked from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Basis {
    text: Rope,
    selection: Range<usize>,
    /// Code, pre, link and mention spans: no highlights inside them.
    skip: Vec<Range<usize>>,
    generation: u64,
    on: bool,
}

#[derive(Default)]
pub(super) struct EmojiWordsUi {
    basis: Basis,
    pub highlights: Vec<Highlight>,
    /// When each word of this draft (by normalized text) first lit up:
    /// its glimmer, and its place kept while the draft changes.
    lit_at: HashMap<String, Instant>,
    /// The highlight under the pointer, with its strip.
    pub hovered: Option<(Highlight, Vec<SuggestItem>)>,
    /// Words closed with Esc: not highlighted again in this session.
    ignored: HashSet<String>,
    /// Where the hover strip was painted, so leaving the field for it
    /// does not close it.
    strip_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl QuillApp {
    /// Tests: hover the `index`th highlight (the headless test text
    /// system has no glyph rects to point at).
    #[cfg(test)]
    pub(super) fn hover_highlight(&mut self, index: usize) {
        if let Some(h) = self.composer_ui.emoji_words.highlights.get(index).cloned() {
            let items = self.word_strip_items(&h);
            self.composer_ui.emoji_words.hovered = Some((h, items));
        }
    }
    fn emoji_words_on(&self) -> bool {
        self.chat_prefs.suggest_emoji && self.chat_prefs.highlight_emoji_words
    }

    /// Pick the highlights again if anything they depend on changed.
    /// Called every frame and after composer edits.
    pub(super) fn sync_emoji_words(&mut self, cx: &mut Context<Self>) {
        let composer = self.composer.read(cx);
        let selection = composer.selected_range();
        let generation = self
            .session()
            .map_or(0, |s| s.stickers.emoji.keyword_lookups.generation);
        let on = self.emoji_words_on() && self.composer_ui.pending_edit.is_none();
        let skip = composer.spans().iter().filter(|span| {
            matches!(
                ComposerTag::from_tag(span.tag()),
                Some(
                    ComposerTag::Code
                        | ComposerTag::Pre(_)
                        | ComposerTag::Link(_)
                        | ComposerTag::Mention(_)
                )
            )
        });
        let ui = &self.composer_ui.emoji_words;
        if ui.basis.selection == selection
            && ui.basis.generation == generation
            && ui.basis.on == on
            && ui.basis.text == *composer.text()
            && ui
                .basis
                .skip
                .iter()
                .cloned()
                .eq(skip.clone().map(|span| span.range()))
        {
            return;
        }
        let skip: Vec<Range<usize>> = skip.map(|span| span.range()).collect();
        let rope = composer.text().clone();
        let text = rope.to_string();
        let (mut found, words) = if on {
            candidates(&text, selection.clone(), &skip)
        } else {
            (Vec::new(), 0)
        };
        found.retain(|c| !ui.ignored.contains(&c.normalized));
        if let Some(live) = self.live.as_mut() {
            let _ = live
                .driver
                .lookup_emoji_keywords(found.iter().map(|c| c.normalized.as_str()));
        }
        let ui = &self.composer_ui.emoji_words;
        let highlights = match self.session() {
            Some(session) => {
                let lookups = &session.stickers.emoji.keyword_lookups;
                pick(
                    &found,
                    words,
                    |w| ui.lit_at.contains_key(w),
                    |q| lookups.answer(q),
                )
            }
            None => Vec::new(),
        };
        let now = Instant::now();
        let ui = &mut self.composer_ui.emoji_words;
        // A word keeps its place while the caret passes through it; one
        // deleted from the draft is forgotten.
        if !ui.lit_at.is_empty() {
            let lower = text.to_lowercase();
            ui.lit_at.retain(|word, _| lower.contains(word.as_str()));
        }
        for h in &highlights {
            ui.lit_at.entry(h.normalized.clone()).or_insert(now);
        }
        if ui
            .hovered
            .as_ref()
            .is_some_and(|(hovered, _)| !highlights.contains(hovered))
        {
            ui.hovered = None;
        }
        ui.basis = Basis {
            text: rope,
            selection,
            skip,
            generation,
            on,
        };
        if ui.highlights != highlights {
            ui.highlights = highlights;
            cx.notify();
        }
    }

    /// The pointer moved over the field: hover the highlight under it,
    /// or close the strip when it is over plain text.
    fn hover_emoji_word(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let state = self.composer.read(cx);
        let under = self
            .composer_ui
            .emoji_words
            .highlights
            .iter()
            .find(|h| {
                state
                    .range_to_rects(&h.range)
                    .iter()
                    .any(|rect| rect.contains(&position))
            })
            .cloned();
        let ui = &self.composer_ui.emoji_words;
        if ui.hovered.as_ref().map(|(h, _)| h) == under.as_ref() {
            return;
        }
        if under.is_none() && ui.strip_bounds.get().is_some_and(|b| b.contains(&position)) {
            return;
        }
        let hovered = under.map(|h| {
            let items = self.word_strip_items(&h);
            (h, items)
        });
        self.composer_ui.emoji_words.hovered = hovered.filter(|(_, items)| !items.is_empty());
        cx.notify();
    }

    /// The strip for a highlighted word: what `:word` would offer.
    fn word_strip_items(&self, h: &Highlight) -> Vec<SuggestItem> {
        let query = SuggestQuery {
            kind: SuggestKind::Emoji,
            range: h.range.clone(),
            query: h.normalized.clone(),
        };
        let session = self.session();
        let keywords = session.and_then(|s| s.stickers.emoji.keyword_lookups.answer(&h.normalized));
        let recent = session
            .map(|s| s.settings.media_prefs.recent_emoji.as_slice())
            .unwrap_or_default();
        suggest_items(&query, &RecentHashtags::default(), keywords, recent)
    }

    /// The pointer left the field and is not over the strip.
    fn leave_emoji_words(&mut self, window: &Window, cx: &mut Context<Self>) {
        let ui = &mut self.composer_ui.emoji_words;
        let position = window.mouse_position();
        if ui.hovered.is_some() && !ui.strip_bounds.get().is_some_and(|b| b.contains(&position)) {
            ui.hovered = None;
            cx.notify();
        }
    }

    /// Esc while a word's strip is open: close it and stop highlighting
    /// that word. True when consumed.
    pub(super) fn dismiss_emoji_word(&mut self, cx: &mut Context<Self>) -> bool {
        let ui = &mut self.composer_ui.emoji_words;
        let Some((h, _)) = ui.hovered.take() else {
            return false;
        };
        ui.ignored.insert(h.normalized);
        ui.basis = Basis::default();
        self.sync_emoji_words(cx);
        cx.notify();
        true
    }

    /// A tile of a word's strip was pressed: the word becomes the emoji.
    pub(super) fn pick_word_emoji(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((h, items)) = self.composer_ui.emoji_words.hovered.take() else {
            return;
        };
        let Some(item) = items.get(index) else {
            return;
        };
        let text = self.composer.read(cx).value().to_string();
        if text.get(h.range.clone()).map(str::to_lowercase).as_deref() != Some(&h.normalized) {
            cx.notify();
            return;
        }
        // The word is not at the caret: put the caret back where it was,
        // shifted by the change in length if it was after the word.
        let selection = self.composer.read(cx).selected_range();
        let shift = |at: usize| {
            if at >= h.range.end {
                (at + item.insert.len()).saturating_sub(h.range.len())
            } else {
                at
            }
        };
        let selection = shift(selection.start)..shift(selection.end);
        self.replace_composer_text(h.range.clone(), &item.insert, window, cx);
        self.composer.update(cx, |input, cx| {
            input.focus(window, cx);
            input.set_selected_range(selection, cx);
        });
        let new_text = self.composer.read(cx).value().to_string();
        self.sync_composer_typing(&new_text);
        cx.notify();
    }

    /// Mouse handlers for the field's wrapper (they never stop the event,
    /// so the Textarea still gets every click and drag).
    pub(super) fn emoji_words_hover<E: InteractiveElement + StatefulInteractiveElement>(
        &self,
        element: E,
        cx: &mut Context<Self>,
    ) -> E {
        element
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if !this.composer_ui.emoji_words.highlights.is_empty()
                    || this.composer_ui.emoji_words.hovered.is_some()
                {
                    this.hover_emoji_word(event.position, cx);
                }
            }))
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                if !*hovered {
                    this.leave_emoji_words(window, cx);
                }
            }))
    }

    /// The highlights over the Textarea (place it after the Textarea
    /// inside the `relative()` wrapper).
    pub(super) fn emoji_word_highlights(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ui = &self.composer_ui.emoji_words;
        if ui.highlights.is_empty() || ui.basis.text != *self.composer.read(cx).text() {
            return None;
        }
        let now = Instant::now();
        let still = cx.reduce_motion();
        let words: Vec<(Range<usize>, Option<f32>, bool)> = ui
            .highlights
            .iter()
            .map(|h| {
                let glimmer = ui
                    .lit_at
                    .get(&h.normalized)
                    .map(|at| now.saturating_duration_since(*at))
                    .filter(|age| !still && *age < GLIMMER)
                    .map(|age| age.as_secs_f32() / GLIMMER.as_secs_f32());
                let hovered = ui.hovered.as_ref().is_some_and(|(o, _)| o == h);
                (h.range.clone(), glimmer, hovered)
            })
            .collect();
        if words.iter().any(|(_, glimmer, _)| glimmer.is_some()) {
            self.request_animation_tick(60, cx);
        }
        let composer = self.composer.clone();
        let theme = cx.theme();
        // The accent into a violet, so it reads as a different thing from
        // links (accent) and spelling marks (red).
        let colors = (theme.primary, hsla(0.76, 0.65, 0.62, 1.));
        Some(
            canvas(
                |_, _, _| (),
                move |bounds, (), window, cx| {
                    paint_highlights(&composer, &words, colors, bounds, window, cx);
                },
            )
            .absolute()
            .inset_0()
            .into_any_element(),
        )
    }

    /// The strip over a hovered word, anchored just above it.
    pub(super) fn emoji_word_strip(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (h, items) = self.composer_ui.emoji_words.hovered.as_ref()?;
        let rect = self
            .composer
            .read(cx)
            .range_to_rects(&h.range)
            .first()
            .copied()?;
        let bounds_out = self.composer_ui.emoji_words.strip_bounds.clone();
        let tiles = emoji_tiles(items, None, "word-emoji", cx, QuillApp::pick_word_emoji);
        let strip = div()
            .id("word-emoji-strip")
            .role(gpui_kit::Role::ListBox)
            .aria_label(tr(keys::Q_CHAT_EMOJI_SUGGESTIONS))
            .relative()
            .flex()
            .items_center()
            .gap_1()
            .p_1()
            .mb_1()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .shadow_md()
            .max_w(px(STRIP_VISIBLE as f32 * (TILE + GAP) + 8.))
            .overflow_x_scroll()
            .on_mouse_move(|_, _, cx| cx.stop_propagation())
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                if !*hovered {
                    this.composer_ui.emoji_words.strip_bounds.set(None);
                    let position = window.mouse_position();
                    let over_word =
                        this.composer_ui
                            .emoji_words
                            .hovered
                            .as_ref()
                            .is_some_and(|(h, _)| {
                                this.composer
                                    .read(cx)
                                    .range_to_rects(&h.range)
                                    .iter()
                                    .any(|r| r.contains(&position))
                            });
                    if !over_word {
                        this.composer_ui.emoji_words.hovered = None;
                        cx.notify();
                    }
                }
            }))
            .children(tiles)
            .child(
                canvas(
                    move |bounds, _, _| bounds_out.set(Some(bounds)),
                    |_, _, _, _| (),
                )
                .absolute()
                .inset_0(),
            );
        Some(
            deferred(
                anchored()
                    .position(rect.origin)
                    .anchor(Anchor::BottomLeft)
                    .snap_to_window_with_margin(px(8.))
                    .child(strip),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}

/// A soft two-color wash behind each word's letters, stronger while
/// hovered; a lighter band sweeps across while it glimmers.
fn paint_highlights(
    composer: &Entity<TextareaState>,
    words: &[(Range<usize>, Option<f32>, bool)],
    (from, to): (Hsla, Hsla),
    clip: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    let state = composer.read(cx);
    let mut quads = Vec::new();
    for (range, glimmer, hovered) in words {
        for rect in state.range_to_rects(range) {
            let alpha = if *hovered { 0.28 } else { 0.12 };
            let wash = rect.dilate(px(1.));
            quads.push(
                fill(
                    wash,
                    linear_gradient(
                        90.,
                        linear_color_stop(from.opacity(alpha), 0.),
                        linear_color_stop(to.opacity(alpha), 1.),
                    ),
                )
                .corner_radii(px(4.)),
            );
            // A thin gradient line under the letters says "this does
            // something" without reading as a spelling mark.
            let line = Bounds::new(
                point(wash.left() + px(2.), wash.bottom() - px(2.)),
                size((wash.size.width - px(4.)).max(px(0.)), px(1.5)),
            );
            quads.push(
                fill(
                    line,
                    linear_gradient(
                        90.,
                        linear_color_stop(from.opacity(0.85), 0.),
                        linear_color_stop(to.opacity(0.85), 1.),
                    ),
                )
                .corner_radii(px(1.)),
            );
            if let Some(t) = glimmer {
                // The band runs from just before the word to just past it,
                // fading out as it goes.
                let band = rect.size.width * 0.35;
                let travel = rect.size.width + band * 2.;
                let x = rect.left() - band + travel * *t;
                let strength = 0.45 * (1. - t);
                let shine = hsla(0., 0., 1., 1.);
                for (start, a, b) in [(x, 0., strength), (x + band / 2., strength, 0.)] {
                    let glow =
                        Bounds::new(point(start, rect.top()), size(band / 2., rect.size.height))
                            .intersect(&wash);
                    if glow.size.width > px(0.) {
                        quads.push(fill(
                            glow,
                            linear_gradient(
                                90.,
                                linear_color_stop(shine.opacity(a), 0.),
                                linear_color_stop(shine.opacity(b), 1.),
                            ),
                        ));
                    }
                }
            }
        }
    }
    window.with_content_mask(Some(ContentMask { bounds: clip }), |window| {
        for quad in quads {
            window.paint_quad(quad);
        }
    });
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "emoji_words_ui_tests.rs"]
mod tests;
