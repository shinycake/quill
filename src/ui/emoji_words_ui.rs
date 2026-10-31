//! Composer words that have an emoji (`quill::emoji_words`): a soft
//! gradient highlight painted over the Textarea (it has no decoration
//! hooks, like the spelling underlines), a one-time glimmer when a word
//! first lights up, and on hover the same bar `:word` would offer. The
//! word's right-click menu (`emoji_words_menu`) offers its emoji, stops
//! offering them, or turns the highlights off.
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
use super::composer_suggest::{SuggestItem, suggest_items};
use gpui_base::input::InputContent;
use gpui_kit::component::input::{Rope, TextareaState};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::composer_doc::ComposerTag;
use quill::emoji_words::{Highlight, candidates, pick};
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
    /// Words the user said not to offer emoji for (Settings).
    blocked: Vec<String>,
}

#[derive(Default)]
pub(super) struct EmojiWordsUi {
    basis: Basis,
    pub highlights: Vec<Highlight>,
    /// When each word of this draft (by normalized text) first lit up:
    /// its glimmer plays once.
    lit_at: HashMap<String, Instant>,
    /// The highlight under the pointer, with its bar.
    pub hovered: Option<(Highlight, Vec<SuggestItem>)>,
    /// `hovered` came from "Suggest Emoji" in the menu: it stays while
    /// its word does, highlighted or not.
    pub pinned: bool,
    /// "Suggest Emoji" on a word whose keyword answer is on its way.
    pub asked: Option<(Range<usize>, String)>,
    /// Words closed with Esc: not highlighted again in this session.
    ignored: HashSet<String>,
    /// Where the word's bar was painted, so moving the pointer from the
    /// word to it does not close it.
    strip_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The pointer went somewhere else: the bar follows it when the
    /// [`BAR_GRACE`] timer ends, unless the pointer comes back first.
    pending: Option<(BarAim, Task<()>)>,
}

impl EmojiWordsUi {
    pub(super) fn strip_bounds(&self) -> Rc<Cell<Option<Bounds<Pixels>>>> {
        self.strip_bounds.clone()
    }

    /// Close the word's bar.
    pub(super) fn unhover(&mut self) {
        self.hovered = None;
        self.pinned = false;
        self.strip_bounds.set(None);
        self.pending = None;
    }
}

/// How far around the word and its bar the pointer may stray.
const SLACK: f32 = 6.;
/// How long the pointer may be off the way to an open bar, or resting on
/// another word, before the bar follows it. Every matching word lights up,
/// so the way up to the bar often crosses other words, and a hand rarely
/// keeps to a straight line.
pub(super) const BAR_GRACE: Duration = Duration::from_millis(400);

/// Where the pointer is taking an open bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BarAim {
    /// Keep the bar: the pointer is on its word, on it, or on the way.
    Hold,
    /// To this other word's bar.
    Word(Highlight),
    Close,
}

/// What the pointer means for the bar of `shown`.
fn bar_aim(under: Option<&Highlight>, shown: &Highlight, pinned: bool, on_the_way: bool) -> BarAim {
    match under {
        Some(word) if word == shown => BarAim::Hold,
        Some(word) => BarAim::Word(word.clone()),
        None if pinned || on_the_way => BarAim::Hold,
        None => BarAim::Close,
    }
}

/// Whether `p` is on the way between a word and its bar: inside either
/// (with some slack) or in the band between them, which narrows from
/// the bar's width to the word's (their convex hull, for a bar above).
pub(super) fn on_the_way(p: Point<Pixels>, word: &[Bounds<Pixels>], bar: Bounds<Pixels>) -> bool {
    let slack = px(SLACK);
    if bar.dilate(slack).contains(&p) || word.iter().any(|r| r.dilate(slack).contains(&p)) {
        return true;
    }
    let Some(top) = word.first() else {
        return false;
    };
    let (from, to) = (bar.bottom(), top.top());
    if to <= from || p.y < from || p.y > to {
        return false;
    }
    let t = (p.y - from) / (to - from);
    let left = bar.left() + (top.left() - bar.left()) * t - slack;
    let right = bar.right() + (top.right() - bar.right()) * t + slack;
    p.x >= left && p.x <= right
}

impl QuillApp {
    /// Tests: hover the `index`th highlight (the headless test text
    /// system has no glyph rects to point at).
    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
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
            && ui.basis.blocked == self.chat_prefs.emoji_word_blocklist
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
        let blocked = &self.chat_prefs.emoji_word_blocklist;
        found.retain(|c| !ui.ignored.contains(&c.normalized) && !blocked.contains(&c.normalized));
        if let Some(live) = self.live.as_mut() {
            let _ = live
                .driver
                .lookup_emoji_keywords(found.iter().map(|c| c.normalized.as_str()));
        }
        let highlights = match self.session() {
            Some(session) => {
                let lookups = &session.stickers.emoji.keyword_lookups;
                pick(&found, words, |q| lookups.answer(q))
            }
            None => Vec::new(),
        };
        let now = Instant::now();
        let ui = &mut self.composer_ui.emoji_words;
        // A word glimmers once while it is in the draft: one the caret
        // passes through does not replay it.
        if !ui.lit_at.is_empty() {
            let lower = text.to_lowercase();
            ui.lit_at.retain(|word, _| lower.contains(word.as_str()));
        }
        for h in &highlights {
            ui.lit_at.entry(h.normalized.clone()).or_insert(now);
        }
        let same_word = |h: &Highlight| {
            text.get(h.range.clone()).map(str::to_lowercase).as_deref() == Some(&h.normalized)
        };
        if ui.hovered.as_ref().is_some_and(|(hovered, _)| {
            if ui.pinned {
                !same_word(hovered)
            } else {
                !highlights.contains(hovered)
            }
        }) {
            ui.unhover();
        }
        if ui.asked.as_ref().is_some_and(|(range, word)| {
            text.get(range.clone()).map(str::to_lowercase).as_deref() != Some(word)
        }) {
            ui.asked = None;
        }
        ui.basis = Basis {
            text: rope,
            selection,
            skip,
            generation,
            on,
            blocked: self.chat_prefs.emoji_word_blocklist.clone(),
        };
        if ui.highlights != highlights {
            ui.highlights = highlights;
            cx.notify();
        }
        self.answer_asked_word(cx);
    }

    /// The pointer moved over the field, or anywhere in the window while a
    /// word's bar shows: open the bar of the highlight under it, or let an
    /// open bar follow the pointer.
    pub(super) fn track_word_bar(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let bar = self.composer_ui.emoji_words.strip_bounds.get();
        // A word scrolled out of the field keeps its rects: above the
        // bar's bottom edge the pointer is over the history.
        let in_field = bar.is_none_or(|bar| position.y >= bar.bottom());
        let state = self.composer.read(cx);
        let ui = &self.composer_ui.emoji_words;
        let under = ui
            .highlights
            .iter()
            .find(|h| {
                in_field
                    && state
                        .range_to_rects(&h.range)
                        .iter()
                        .any(|rect| rect.contains(&position))
            })
            .cloned();
        let aim = match ui.hovered.as_ref() {
            Some((shown, _)) => bar_aim(
                under.as_ref(),
                shown,
                ui.pinned,
                self.pointer_on_the_way(position, cx),
            ),
            None => {
                if let Some(word) = under {
                    self.show_word_bar(Some(word), cx);
                }
                return;
            }
        };
        self.aim_word_bar(aim, cx);
    }

    /// Hold the bar, or start the wait for it to follow the pointer. A wait
    /// for the same place keeps running, so roaming off the way still
    /// closes the bar, and sweeping across words only moves it once the
    /// pointer rests on one.
    pub(super) fn aim_word_bar(&mut self, aim: BarAim, cx: &mut Context<Self>) {
        let ui = &mut self.composer_ui.emoji_words;
        if aim == BarAim::Hold {
            ui.pending = None;
            return;
        }
        if ui.pending.as_ref().is_some_and(|(to, _)| *to == aim) {
            return;
        }
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(BAR_GRACE).await;
            let _ = this.update(cx, |this, cx| this.settle_word_bar(cx));
        });
        ui.pending = Some((aim, task));
    }

    /// The wait ended with the pointer still away.
    fn settle_word_bar(&mut self, cx: &mut Context<Self>) {
        let Some((aim, _)) = self.composer_ui.emoji_words.pending.take() else {
            return;
        };
        match aim {
            BarAim::Hold => {}
            BarAim::Word(word) => self.show_word_bar(Some(word), cx),
            BarAim::Close => {
                self.show_word_bar(None, cx);
                if self.composer_ui.suggest.active.is_none() {
                    self.composer_ui.suggest.bar.reset();
                }
            }
        }
    }

    fn show_word_bar(&mut self, word: Option<Highlight>, cx: &mut Context<Self>) {
        let bar = word.map(|h| {
            let items = self.word_strip_items(&h);
            (h, items)
        });
        let ui = &mut self.composer_ui.emoji_words;
        ui.unhover();
        ui.hovered = bar.filter(|(_, items)| !items.is_empty());
        cx.notify();
    }

    /// Whether the pointer is over the hovered word, its bar, or on its
    /// way from one to the other.
    fn pointer_on_the_way(&self, position: Point<Pixels>, cx: &App) -> bool {
        let ui = &self.composer_ui.emoji_words;
        let Some((h, _)) = ui.hovered.as_ref() else {
            return false;
        };
        let word = self.composer.read(cx).range_to_rects(&h.range);
        match ui.strip_bounds.get() {
            Some(bar) => on_the_way(position, &word, bar),
            None => word.iter().any(|r| r.dilate(px(SLACK)).contains(&position)),
        }
    }

    /// The pointer left the field or the bar.
    pub(super) fn leave_word_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.composer_ui.emoji_words.hovered.is_some() {
            self.track_word_bar(window.mouse_position(), cx);
        }
    }

    /// The bar for a word: what `:word` would offer, custom emoji after.
    pub(super) fn word_strip_items(&mut self, h: &Highlight) -> Vec<SuggestItem> {
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
        let items = suggest_items(&query, &RecentHashtags::default(), keywords, recent);
        self.with_custom_emoji(items)
    }

    /// Esc while a word's strip is open: close it and stop highlighting
    /// that word. True when consumed.
    pub(super) fn dismiss_emoji_word(&mut self, cx: &mut Context<Self>) -> bool {
        let ui = &mut self.composer_ui.emoji_words;
        let Some((h, _)) = ui.hovered.take() else {
            return false;
        };
        ui.unhover();
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
        self.composer_ui.emoji_words.unhover();
        self.composer_ui.suggest.bar.reset();
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
        let content = match item.custom.as_ref().and_then(|s| s.custom_emoji_id) {
            Some(id) => self.custom_emoji_content(&item.insert, id),
            None => InputContent::new(item.insert.clone()),
        };
        // The rich editor inserts a custom emoji as longer markup.
        let inserted = content.text().len();
        let selection = self.composer.read(cx).selected_range();
        let shift = |at: usize| {
            if at >= h.range.end {
                (at + inserted).saturating_sub(h.range.len())
            } else {
                at
            }
        };
        let selection = shift(selection.start)..shift(selection.end);
        self.replace_composer_content(h.range.clone(), content, window, cx);
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
                    this.track_word_bar(event.position, cx);
                }
            }))
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                if !*hovered {
                    this.leave_word_bar(window, cx);
                }
            }))
    }

    /// The highlights over the Textarea (place it after the Textarea
    /// inside the `relative()` wrapper).
    pub(super) fn emoji_word_highlights(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ui = &self.composer_ui.emoji_words;
        let pinned = ui
            .hovered
            .as_ref()
            .filter(|(h, _)| ui.pinned && !ui.highlights.contains(h))
            .map(|(h, _)| (h.range.clone(), None, true));
        if (ui.highlights.is_empty() && pinned.is_none())
            || ui.basis.text != *self.composer.read(cx).text()
        {
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
            .chain(pinned)
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

#[cfg(feature = "demo-capture")]
#[path = "emoji_words_demo.rs"]
mod demo;

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "emoji_words_ui_tests.rs"]
mod tests;

#[cfg(test)]
mod way_tests {
    use super::{BarAim, bar_aim, on_the_way};
    use gpui_kit::{Bounds, point, px, size};
    use quill::emoji_words::Highlight;

    fn word(start: usize, normalized: &str) -> Highlight {
        Highlight {
            range: start..start + normalized.len(),
            normalized: normalized.into(),
            emoji: Vec::new(),
        }
    }

    #[test]
    fn a_word_on_the_way_waits_and_plain_text_on_the_way_holds() {
        let (shown, other) = (word(0, "pizza"), word(6, "cake"));
        let aim = |under, pinned, way| bar_aim(under, &shown, pinned, way);
        assert_eq!(aim(Some(&shown), false, false), BarAim::Hold);
        assert_eq!(aim(None, false, true), BarAim::Hold);
        assert_eq!(aim(None, true, false), BarAim::Hold, "asked for: stays");
        assert_eq!(aim(None, false, false), BarAim::Close);
        assert_eq!(
            aim(Some(&other), false, true),
            BarAim::Word(other.clone()),
            "crossing a lit word is not yet a switch: it goes through the wait"
        );
    }

    #[test]
    fn the_way_from_a_word_up_to_its_bar_keeps_it_open() {
        // A wide bar above, a short word two lines further down.
        let bar = Bounds::new(point(px(0.), px(0.)), size(px(400.), px(44.)));
        let word = [Bounds::new(
            point(px(200.), px(100.)),
            size(px(40.), px(18.)),
        )];
        let on = |x: f32, y: f32| on_the_way(point(px(x), px(y)), &word, bar);
        assert!(on(220., 110.), "on the word");
        assert!(on(10., 20.), "on the bar");
        assert!(on(205., 95.), "just above the word");
        assert!(on(30., 50.), "just below the bar's left end");
        assert!(on(120., 72.), "halfway, between the two");
        assert!(!on(10., 95.), "beside the word, off the way");
        assert!(!on(220., 140.), "below the word");
        assert!(!on(500., 20.), "past the bar");
    }
}
