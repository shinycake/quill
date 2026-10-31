//! The emoji suggestion bar over the composer, shared by the `:` query and
//! a highlighted word (`emoji_words_ui`). As in Telegram for macOS it spans
//! the field and scrolls sideways, fading at an edge with more to see
//! (tdesktop's suggestions widget fades the same way); the chevron opens it
//! into a grid that scrolls down, and closes it again.
//!
//! After the standard emoji come the custom emoji of the installed emoji
//! sets that stand for them (tdesktop `SuggestionsWidget::appendCustom`),
//! for Premium users, who are the only ones who can send them.
#![deny(clippy::indexing_slicing)]

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::app::QuillApp;
use super::composer_suggest::SuggestItem;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::FileId;
use quill::lang::{keys, tr};
use quill::telegram::envelope::StickerItem;

/// A tile's side and the gap between tiles.
pub(super) const TILE: f32 = 36.;
pub(super) const GAP: f32 = 4.;
/// Custom emoji drawn in a tile.
const CUSTOM_SIDE: f32 = 26.;
/// Custom emoji after the standard ones.
pub(super) const CUSTOM_MAX: usize = 60;
/// Rows the open grid shows before it scrolls.
const GRID_ROWS: f32 = 5.;
/// How wide an edge fades.
const FADE: f32 = 48.;
/// Tiles treated as on screen before the bar has been laid out.
const FIRST_FRAME_TILES: usize = 16;

/// The bar's scroll positions and whether it is open as a grid.
#[derive(Default)]
pub(super) struct EmojiBarUi {
    pub expanded: bool,
    pub strip: ScrollHandle,
    pub grid: ScrollHandle,
    /// The installed emoji set list was asked for (once per session).
    pub sets_asked: bool,
}

impl EmojiBarUi {
    /// Back to a closed strip at its start, for a new list.
    pub fn reset(&mut self) {
        self.expanded = false;
        self.strip.set_offset(point(px(0.), px(0.)));
        self.grid.set_offset(point(px(0.), px(0.)));
    }

    /// The handle of what is showing.
    pub fn handle(&self) -> &ScrollHandle {
        if self.expanded {
            &self.grid
        } else {
            &self.strip
        }
    }

    /// Tiles in one row of the open grid (from its last layout).
    pub fn grid_columns(&self) -> usize {
        let width = self.grid.bounds().size.width.as_f32();
        ((width + GAP) / (TILE + GAP)).floor().max(1.) as usize
    }
}

/// What one bar shows and what a press on a tile does.
pub(super) struct BarSpec<'a> {
    pub id: &'static str,
    pub items: &'a [SuggestItem],
    pub selected: Option<usize>,
    pub pick: fn(&mut QuillApp, usize, &mut Window, &mut Context<QuillApp>),
    /// Where the bar was painted (the hovered word's strip keeps open
    /// while the pointer crosses to it).
    pub bounds_out: Option<Rc<Cell<Option<Bounds<Pixels>>>>>,
    /// The pointer left the bar.
    pub on_leave: Option<fn(&mut QuillApp, &mut Window, &mut Context<QuillApp>)>,
}

/// Standard emoji without the variation selector, for matching a custom
/// emoji's emoji.
fn bare(emoji: &str) -> String {
    emoji.replace('\u{fe0f}', "")
}

/// The custom emoji of `sets` (installed emoji sets, in order) that stand
/// for the standard emoji of `items`, in the order of `items`, each once,
/// at most [`CUSTOM_MAX`].
pub(super) fn custom_items(items: &[SuggestItem], sets: &[&[StickerItem]]) -> Vec<SuggestItem> {
    let wanted: HashSet<String> = items
        .iter()
        .filter(|item| item.custom.is_none() && !item.glyph.is_empty())
        .map(|item| bare(&item.glyph))
        .collect();
    let mut by_emoji: HashMap<String, Vec<&StickerItem>> = HashMap::new();
    for sticker in sets.iter().flat_map(|set| set.iter()) {
        let key = bare(&sticker.emoji);
        if sticker.custom_emoji_id.is_some() && wanted.contains(&key) {
            by_emoji.entry(key).or_default().push(sticker);
        }
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items.iter().filter(|item| item.custom.is_none()) {
        for sticker in by_emoji.get(&bare(&item.glyph)).into_iter().flatten() {
            if out.len() == CUSTOM_MAX {
                return out;
            }
            if seen.insert(sticker.custom_emoji_id) {
                out.push(SuggestItem {
                    insert: sticker.emoji.clone(),
                    glyph: sticker.emoji.clone(),
                    name: item.name.clone(),
                    custom: Some((*sticker).clone()),
                });
            }
        }
    }
    out
}

/// Whether item `ix` of a bar scrolled by `handle` is in view.
fn on_screen(handle: &ScrollHandle, ix: usize, vertical: bool) -> bool {
    let view = handle.bounds();
    let Some(item) = handle.bounds_for_item(ix) else {
        return ix < FIRST_FRAME_TILES;
    };
    if view.size.width <= px(0.) {
        return ix < FIRST_FRAME_TILES;
    }
    let offset = handle.offset();
    if vertical {
        item.bottom() > view.top() - offset.y && item.top() < view.bottom() - offset.y
    } else {
        item.right() > view.left() - offset.x && item.left() < view.right() - offset.x
    }
}

/// A soft fade over one edge of the scrolling tiles: there is more past it.
fn fade_edge(side: Side, color: Hsla) -> Div {
    let (angle, edge) = match side {
        Side::Left => (90., div().top_0().bottom_0().left_0().w(px(FADE))),
        Side::Right => (270., div().top_0().bottom_0().right_0().w(px(FADE))),
        Side::Top => (180., div().left_0().right_0().top_0().h(px(FADE / 2.))),
        Side::Bottom => (0., div().left_0().right_0().bottom_0().h(px(FADE / 2.))),
    };
    edge.absolute().bg(linear_gradient(
        angle,
        linear_color_stop(color, 0.),
        linear_color_stop(color.opacity(0.), 1.),
    ))
}

#[derive(Clone, Copy)]
enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl QuillApp {
    /// The custom emoji for `items`, after them, when the user can send
    /// custom emoji; asks for the installed sets' contents it lacks.
    pub(super) fn with_custom_emoji(&mut self, mut items: Vec<SuggestItem>) -> Vec<SuggestItem> {
        let Some(session) = self.session() else {
            return items;
        };
        if items.is_empty() || !session.my_is_premium() {
            return items;
        }
        let library = &session.media.media_library.set_stickers;
        let sets: Vec<&[StickerItem]> = session
            .stickers
            .emoji
            .installed_sets
            .iter()
            .filter_map(|set| library.get(&set.id).map(Vec::as_slice))
            .collect();
        let custom = custom_items(&items, &sets);
        let missing = sets.len() < session.stickers.emoji.installed_sets.len()
            || session.stickers.emoji.installed_sets.is_empty();
        items.extend(custom);
        if missing {
            let list = !std::mem::replace(&mut self.composer_ui.suggest.bar.sets_asked, true);
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.ensure_suggestion_emoji_sets(list);
            }
        }
        items
    }

    /// How many installed emoji sets have their contents: the bar is
    /// rebuilt when this moves.
    pub(super) fn custom_sets_loaded(&self) -> usize {
        self.session().map_or(0, |session| {
            let library = &session.media.media_library.set_stickers;
            session
                .stickers
                .emoji
                .installed_sets
                .iter()
                .filter(|set| library.contains_key(&set.id))
                .count()
        })
    }

    /// The chevron: open the bar as a grid, or close it back to a strip.
    fn toggle_emoji_bar(&mut self, cx: &mut Context<Self>) {
        let bar = &mut self.composer_ui.suggest.bar;
        bar.expanded = !bar.expanded;
        bar.grid.set_offset(point(px(0.), px(0.)));
        // Keep the highlighted tile in view in the new layout.
        if let Some(at) = self.composer_ui.suggest.selected {
            self.composer_ui.suggest.bar.handle().scroll_to_item(at);
        }
        cx.notify();
    }

    fn emoji_bar_tile(
        &self,
        index: usize,
        item: &SuggestItem,
        spec: &BarSpec,
        visible: bool,
        need_files: &mut Vec<FileId>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pick = spec.pick;
        let content = match &item.custom {
            Some(sticker) => {
                let animated = (visible
                    && !quill::power_saving::on(quill::power_saving::Flag::EmojiPanel))
                .then(|| self.custom_emoji_image(sticker.file_id, sticker.format, CUSTOM_SIDE, cx))
                .flatten();
                let still = self.panel_still(sticker);
                if animated.is_none() && still.is_none() {
                    need_files.extend(sticker.display_file_id());
                }
                match animated
                    .map(ImageSource::from)
                    .or_else(|| still.map(ImageSource::from))
                {
                    Some(source) => img(source)
                        .size(px(CUSTOM_SIDE))
                        .object_fit(ObjectFit::Contain)
                        .into_any_element(),
                    None => div().child(item.glyph.clone()).into_any_element(),
                }
            }
            None => div().child(item.glyph.clone()).into_any_element(),
        };
        div()
            .id((spec.id, index as u64))
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
            .when(Some(index) == spec.selected, |this| {
                this.bg(cx.theme().selection)
            })
            .hover(|style| style.bg(cx.theme().accent))
            // On press: the composer's blur would close the bar before a
            // click could land.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    window.prevent_default();
                    pick(this, index, window, cx);
                }),
            )
            .child(content)
            .into_any_element()
    }

    fn emoji_bar_toggle(&self, expanded: bool, cx: &mut Context<Self>) -> AnyElement {
        let label = tr(if expanded {
            keys::Q_CHAT_EMOJI_SUGGESTIONS_COLLAPSE
        } else {
            keys::Q_CHAT_EMOJI_SUGGESTIONS_EXPAND
        });
        div()
            .id("emoji-bar-toggle")
            .flex_none()
            .size(px(TILE))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .role(gpui_kit::Role::Button)
            .aria_label(label)
            .hover(|style| style.bg(cx.theme().accent))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    window.prevent_default();
                    this.toggle_emoji_bar(cx);
                }),
            )
            .child(
                Icon::new(if expanded {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronUp
                })
                .small()
                .text_color(cx.theme().muted_foreground),
            )
            .into_any_element()
    }

    /// The bar for `spec`: a strip, or the open grid.
    pub(super) fn emoji_bar(&self, spec: BarSpec, cx: &mut Context<Self>) -> AnyElement {
        let bar = &self.composer_ui.suggest.bar;
        let expanded = bar.expanded;
        let handle = bar.handle().clone();
        let mut need_files = Vec::new();
        let tiles: Vec<AnyElement> = spec
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let visible = on_screen(&handle, index, expanded);
                self.emoji_bar_tile(index, item, &spec, visible, &mut need_files, cx)
            })
            .collect();
        if !need_files.is_empty() {
            let weak = cx.weak_entity();
            cx.defer(move |cx| {
                let _ = weak.update(cx, |this, _| {
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.ensure_media_files(&need_files);
                    }
                });
            });
        }
        // What the last layout says about the room left on either side.
        let offset = handle.offset();
        let max = handle.max_offset();
        let view = handle.bounds().size;
        let fits = ((view.width.as_f32() + GAP) / (TILE + GAP)).floor() as usize;
        let overflows = if view.width > px(0.) {
            spec.items.len() > fits
        } else {
            spec.items.len() > FIRST_FRAME_TILES / 2
        };
        let theme = cx.theme();
        let fade = theme.popover;
        let body = if expanded {
            let grid = div()
                .id("emoji-bar-grid")
                .flex()
                .flex_wrap()
                .gap(px(GAP))
                .max_h(px(GRID_ROWS * (TILE + GAP)))
                .overflow_y_scroll()
                .track_scroll(&handle)
                .children(tiles);
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .child(grid)
                .when(offset.y < px(-0.5), |this| {
                    this.child(fade_edge(Side::Top, fade))
                })
                .when(offset.y > -max.y + px(0.5), |this| {
                    this.child(fade_edge(Side::Bottom, fade))
                })
        } else {
            let strip = div()
                .id("emoji-bar-strip")
                .flex()
                .gap(px(GAP))
                .overflow_x_scroll()
                .track_scroll(&handle)
                .children(tiles);
            let more_after = if max.x > px(0.) {
                offset.x > -max.x + px(0.5)
            } else {
                overflows
            };
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .child(strip)
                .when(offset.x < px(-0.5), |this| {
                    this.child(fade_edge(Side::Left, fade))
                })
                .when(more_after, |this| this.child(fade_edge(Side::Right, fade)))
        };
        let on_leave = spec.on_leave;
        div()
            .id("emoji-bar")
            .role(gpui_kit::Role::ListBox)
            .aria_label(tr(keys::Q_CHAT_EMOJI_SUGGESTIONS))
            .relative()
            .flex()
            .when(expanded, |this| this.items_start())
            .when(!expanded, |this| this.items_center())
            .gap_1()
            .p_1()
            .w_full()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .shadow_md()
            // Floats over the history: a click between the tiles must not
            // reach the media under the bar.
            .block_mouse_except_scroll()
            .on_mouse_move(|_, _, cx| cx.stop_propagation())
            .when_some(on_leave, |this, leave| {
                this.on_hover(cx.listener(move |this, hovered: &bool, window, cx| {
                    if !*hovered {
                        leave(this, window, cx);
                    }
                }))
            })
            .child(body)
            .when(overflows || expanded, |this| {
                this.child(self.emoji_bar_toggle(expanded, cx))
            })
            .when_some(spec.bounds_out, |this, out| {
                this.child(
                    canvas(move |bounds, _, _| out.set(Some(bounds)), |_, _, _, _| ())
                        .absolute()
                        .inset_0(),
                )
            })
            .into_any_element()
    }

    /// The bar above the composer field: the `:` query's, or else the
    /// hovered word's. Place it inside the input row (`relative()`).
    pub(super) fn emoji_bar_overlay(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let suggest = &self.composer_ui.suggest;
        let spec = match suggest.active.as_ref() {
            Some(active) if active.query.kind != quill::suggest::SuggestKind::Hashtag => BarSpec {
                id: "suggest-item",
                items: &active.items,
                selected: suggest.selected,
                pick: QuillApp::pick_suggest_index,
                bounds_out: None,
                on_leave: None,
            },
            Some(_) => return None,
            None => {
                let (_, items) = self.composer_ui.emoji_words.hovered.as_ref()?;
                BarSpec {
                    id: "word-emoji",
                    items,
                    selected: None,
                    pick: QuillApp::pick_word_emoji,
                    bounds_out: Some(self.composer_ui.emoji_words.strip_bounds()),
                    on_leave: Some(QuillApp::leave_word_bar),
                }
            }
        };
        let word_bar = spec.on_leave.is_some();
        let bar = self.emoji_bar(spec, cx);
        let weak = cx.weak_entity();
        Some(
            deferred(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(relative(1.))
                    .pb_2()
                    .child(bar)
                    // A word's bar follows the pointer wherever it goes.
                    .when(word_bar, |this| {
                        this.child(
                            canvas(
                                |_, _, _| (),
                                move |_, (), window, _| {
                                    window.on_mouse_event(
                                        move |_: &MouseMoveEvent, phase, window, cx| {
                                            if phase != DispatchPhase::Bubble {
                                                return;
                                            }
                                            let _ = weak.update(cx, |this, cx| {
                                                this.leave_word_bar(window, cx);
                                            });
                                        },
                                    );
                                },
                            )
                            .absolute()
                            .size_0(),
                        )
                    }),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{CUSTOM_MAX, FileId, StickerItem, SuggestItem, custom_items};
    use quill::telegram::envelope::StickerFormat;

    fn standard(glyph: &str) -> SuggestItem {
        SuggestItem {
            insert: glyph.into(),
            glyph: glyph.into(),
            name: String::new(),
            custom: None,
        }
    }

    fn sticker(id: i64, emoji: &str) -> StickerItem {
        StickerItem {
            custom_emoji_id: Some(id),
            id,
            set_id: 1,
            emoji: emoji.into(),
            width: 100,
            height: 100,
            format: StickerFormat::Tgs,
            file_id: FileId(id as i32),
            thumb_file_id: None,
            thumb_width: 0,
            thumb_height: 0,
            requires_premium: true,
            premium_animation: None,
        }
    }

    #[test]
    fn custom_emoji_follow_their_standard_emoji_in_order() {
        let items = [standard("😂"), standard("❤️")];
        let set_a = [sticker(1, "❤"), sticker(2, "🍕"), sticker(3, "😂")];
        let set_b = [sticker(4, "😂"), sticker(3, "😂")];
        let found: Vec<Option<i64>> = custom_items(&items, &[&set_a, &set_b])
            .into_iter()
            .map(|item| item.custom.and_then(|s| s.custom_emoji_id))
            .collect();
        // 😂's first (sets in order, each once), then ❤️'s, whose variation
        // selector the set's "❤" lacks; 🍕 was not suggested.
        assert_eq!(found, [Some(3), Some(4), Some(1)]);
    }

    #[test]
    fn custom_emoji_are_capped() {
        let items = [standard("😂")];
        let set: Vec<StickerItem> = (1..=(CUSTOM_MAX as i64 + 10))
            .map(|id| sticker(id, "😂"))
            .collect();
        assert_eq!(custom_items(&items, &[&set]).len(), CUSTOM_MAX);
    }
}
