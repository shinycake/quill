//! The stories strip at the top of the chat list and its collapsed stack
//! (tdesktop `Dialogs::Stories::List`). Metrics and the scroll mapping are
//! in `quill::stories_strip`.
//!
//! The strip is the first item of the chat list's virtual list, so it
//! scrolls away with the rows; the collapse is a pure function of the
//! list's scroll offset (no timer). As it leaves, a compact stack of up
//! to three small avatars fades in beside the search field; tapping it
//! scrolls back to the top, which expands the strip again.

use super::app::QuillApp;
use super::chat_row::chat_avatar;
use super::pressable::PressableDiv;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::local_path::sandboxed_display_path;
use quill::stories_strip as metrics;
use quill::stories_strip::{Axis, AxisLock};
use quill::story_ring::StoryRing;

/// One entry of the strip: a poster with active stories.
#[derive(Clone)]
pub(super) struct StoryTile {
    pub chat_id: i64,
    pub title: String,
    pub photo: Option<std::path::PathBuf>,
    pub ring: Option<StoryRing>,
    pub latest_story: i32,
}

/// Cache key for the tile list: rebuilt only when the session changed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct StoryTilesKey {
    revision: u64,
    demo: bool,
}

/// Sideways scroll state of the expanded strip.
#[derive(Default)]
pub(super) struct StoryStripState {
    pub tiles: Vec<StoryTile>,
    pub tiles_key: Option<StoryTilesKey>,
    /// Sideways offset of the expanded cells, px.
    pub scroll: f32,
    pub axis: AxisLock,
}

impl QuillApp {
    /// Rebuild the tile list when the session changed (every render would
    /// clone every title).
    pub(super) fn refresh_story_tiles(&mut self) {
        let key = self.session().map(|s| StoryTilesKey {
            revision: s.revision,
            demo: self.demo_session.is_some(),
        });
        if key.is_some() && key == self.story_strip.tiles_key {
            return;
        }
        self.story_strip.tiles_key = key;
        let roots = self.media_display_roots();
        let tiles: Vec<StoryTile> = self
            .session()
            .map(|s| {
                s.ordered_story_tray()
                    .into_iter()
                    .map(|entry| StoryTile {
                        chat_id: entry.chat_id,
                        title: s
                            .chats
                            .get(&entry.chat_id)
                            .map(|chat| chat.title.clone())
                            .unwrap_or_else(|| format!("Chat {}", entry.chat_id)),
                        photo: s
                            .chat_photo_path(ChatId(entry.chat_id))
                            .and_then(|path| sandboxed_display_path(path, &roots)),
                        ring: StoryRing::from_active(entry),
                        latest_story: entry
                            .stories
                            .iter()
                            .map(|info| info.story_id)
                            .max()
                            .unwrap_or(0),
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.story_strip.tiles = tiles;
    }

    /// Pixel width of the strip (the chat list inside the sidebar's
    /// horizontal padding and border).
    fn story_strip_width(&self) -> f32 {
        (f32::from(self.sidebar_width) - 25.).max(120.)
    }

    /// How far the chat list is scrolled down, px.
    fn chat_list_scroll_top(&self) -> f32 {
        -f32::from(self.chat_list_scroll.offset().y)
    }

    /// The first list item: tdesktop's expanded strip, a row of 42 px
    /// avatars with names (`dialogsStoriesFull`). The "+" tile that opens
    /// the story composer comes first.
    pub(super) fn story_strip_element(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let width = self.story_strip_width();
        let count = self.story_strip.tiles.len() + 1;
        let max = metrics::max_scroll(width, count);
        self.story_strip.scroll = self.story_strip.scroll.clamp(0., max);
        let scroll = self.story_strip.scroll;
        let (first, pitch) = metrics::expanded_layout(width, count);
        let primary = cx.theme().primary;
        let muted = cx.theme().muted_foreground;
        let name = |label: String| {
            div()
                .mt(px(metrics::FULL_NAME_TOP
                    - metrics::FULL_PHOTO_TOP
                    - metrics::FULL_PHOTO))
                .w(px(metrics::CELL_WIDTH))
                .px_1()
                .text_center()
                .text_size(px(metrics::FULL_NAME_FONT))
                .truncate()
                .child(label)
        };
        let cell = |index: usize| {
            div()
                .absolute()
                .top_0()
                .left(px(first + pitch * index as f32 - scroll))
                .w(px(metrics::CELL_WIDTH))
                .h(px(metrics::FULL_HEIGHT))
                .pt(px(metrics::FULL_PHOTO_TOP))
                .flex()
                .flex_col()
                .items_center()
                .cursor_pointer()
        };
        let visible = |index: usize| {
            let left = first + pitch * index as f32 - scroll;
            left + metrics::CELL_WIDTH > 0. && left < width
        };
        let mut strip = div()
            .id("story-tray")
            .relative()
            .w_full()
            .h(px(metrics::FULL_HEIGHT))
            .overflow_hidden()
            .on_scroll_wheel(cx.listener(move |this, event: &ScrollWheelEvent, _, cx| {
                this.story_strip_wheel(event, max, cx);
            }));
        if visible(0) {
            strip = strip.child(
                cell(0)
                    .id("story-tray-add")
                    .role(gpui_kit::Role::Button)
                    .aria_label("Create story")
                    .tab_index(0)
                    .pressable(cx.theme())
                    .child(
                        div()
                            .size(px(metrics::FULL_PHOTO))
                            .rounded_full()
                            .border_2()
                            .border_color(primary)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xl()
                            .text_color(primary)
                            .child("+"),
                    )
                    .child(name("New story".to_string()))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_story_composer(window, cx);
                    })),
            );
        }
        for (i, tile) in self.story_strip.tiles.iter().enumerate() {
            let index = i + 1;
            if !visible(index) {
                continue;
            }
            let chat_id = tile.chat_id;
            let latest_story = tile.latest_story;
            let title = tile.title.clone();
            let avatar = match tile.ring {
                Some(ring) => super::story_ring::with_story_ring(
                    |size| chat_avatar(&title, tile.photo.as_deref(), size).into_any_element(),
                    ring,
                    metrics::FULL_PHOTO,
                    muted,
                ),
                None => chat_avatar(&title, tile.photo.as_deref(), metrics::FULL_PHOTO)
                    .into_any_element(),
            };
            strip = strip.child(
                cell(index)
                    .id(("story-tray-item", chat_id as u64))
                    .role(gpui_kit::Role::Button)
                    .aria_label(format!("Open stories for {title}"))
                    .tab_index(0)
                    .pressable(cx.theme())
                    .child(avatar)
                    .child(name(title))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            this.open_story_menu(chat_id, event.position, cx);
                        }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_story_viewer(ChatId(chat_id), latest_story, cx);
                    })),
            );
        }
        strip.into_any_element()
    }

    /// Wheel over the strip: sideways motion scrolls the strip, vertical
    /// motion falls through to the chat list (`verticalScrollEvents`).
    /// The axis locks for a whole gesture.
    fn story_strip_wheel(&mut self, event: &ScrollWheelEvent, max: f32, cx: &mut Context<Self>) {
        let (dx, dy) = match event.delta {
            ScrollDelta::Pixels(p) => (f32::from(p.x), f32::from(p.y)),
            ScrollDelta::Lines(l) => (l.x * 20., l.y * 20.),
        };
        let restart = event.touch_phase == TouchPhase::Started;
        let axis = self
            .story_strip
            .axis
            .feed(restart, event.delta.precise(), dx, dy);
        if matches!(event.touch_phase, TouchPhase::Ended | TouchPhase::Cancelled) {
            self.story_strip.axis.end();
        }
        if axis != Axis::Horizontal {
            return;
        }
        cx.stop_propagation();
        let next = (self.story_strip.scroll - dx).clamp(0., max);
        if next != self.story_strip.scroll {
            self.story_strip.scroll = next;
            self.notify_sidebar(cx);
        }
    }

    /// The collapsed strip: up to three overlapping avatars beside the
    /// search field, fading in as the expanded strip scrolls away.
    /// `None` while expanded or with no stories. Width follows the
    /// opacity, so the search field gives way smoothly.
    pub(super) fn story_compact_stack(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let tiles = &self.story_strip.tiles;
        if tiles.is_empty() {
            return None;
        }
        let opacity =
            metrics::compact_opacity(metrics::collapse_progress(self.chat_list_scroll_top()));
        if opacity <= 0.0 {
            return None;
        }
        let shown = metrics::compact_count(tiles.len());
        let width = metrics::compact_width(shown);
        let muted = cx.theme().muted_foreground;
        let mut stack = div().flex().flex_none().items_center().h(px(40.));
        for (i, tile) in tiles.iter().take(shown).enumerate() {
            let avatar = match tile.ring {
                Some(ring) => super::story_ring::with_story_ring(
                    |size| chat_avatar(&tile.title, tile.photo.as_deref(), size).into_any_element(),
                    ring,
                    metrics::SMALL_PHOTO,
                    muted,
                ),
                None => chat_avatar(&tile.title, tile.photo.as_deref(), metrics::SMALL_PHOTO)
                    .into_any_element(),
            };
            stack = stack.child(
                div()
                    .flex_none()
                    .size(px(metrics::SMALL_PHOTO))
                    .when(i > 0, |this| {
                        this.ml(px(metrics::SMALL_SHIFT - metrics::SMALL_PHOTO))
                    })
                    .child(avatar),
            );
        }
        Some(
            div()
                .id("story-compact")
                .role(gpui_kit::Role::Button)
                .aria_label("Show stories")
                .tab_index(0)
                .cursor_pointer()
                .flex_none()
                .overflow_hidden()
                .w(px((width + 4.) * opacity))
                .opacity(opacity)
                .child(stack)
                .on_click(cx.listener(|this, _, _, cx| {
                    // Expand: back to the top of the list.
                    this.chat_list_scroll.set_offset(point(px(0.), px(0.)));
                    this.notify_sidebar(cx);
                }))
                .into_any_element(),
        )
    }
}
