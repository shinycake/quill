//! The emoji and sticker avatar builder (tdesktop
//! `info_userpic_emoji_builder_widget.cpp`): a preview, the seven gradient
//! presets plus a custom gradient, and a selector with an emoji tab and a
//! stickers tab.

use super::gradient_editor::GradientEditor;
use super::{PhotoSources, QuillApp, render_rgba, replace_image};
use crate::ui::pressable::PressableDiv;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::local_path::sandboxed_display_path;
use quill::profile_photo_gradient::render_background;
use quill::profile_photo_sources::{PRESET_COUNT, PhotoSticker, gradient_preset};
use quill::telegram::envelope::{StickerFormat, StickerItem};
use std::sync::Arc;

const PREVIEW_PX: f32 = 96.0;
const SWATCH_PX: f32 = 28.0;
/// Stickers shown on the stickers tab (recent first, then favorites).
const STICKER_LIMIT: usize = 60;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Tab {
    #[default]
    Emoji,
    Stickers,
}

/// What sits on the background.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Pick {
    Emoji(i64),
    Sticker { set_id: i64, id: i64 },
}

impl Pick {
    fn as_photo_sticker(self) -> PhotoSticker {
        match self {
            Pick::Emoji(custom_emoji_id) => PhotoSticker::CustomEmoji { custom_emoji_id },
            Pick::Sticker { set_id, id } => PhotoSticker::RegularOrMask {
                sticker_set_id: set_id,
                sticker_id: id,
            },
        }
    }
}

/// The builder's choices and the pictures drawn for them.
pub(crate) struct EmojiBuilder {
    pub(super) tab: Tab,
    /// 0 to `PRESET_COUNT - 1`: a preset; `PRESET_COUNT`: the custom one.
    pub(super) preset: usize,
    pub(super) custom: Vec<u32>,
    pub(super) pick: Option<Pick>,
    swatches: Vec<Option<Arc<RenderImage>>>,
    custom_swatch: Option<Arc<RenderImage>>,
    background: Option<Arc<RenderImage>>,
    pub(super) editor: Option<GradientEditor>,
}

fn draw(colors: &[u32], side: u32) -> Option<Arc<RenderImage>> {
    let square = render_background(colors, side);
    render_rgba(square.side, square.side, square.rgba)
}

impl Default for EmojiBuilder {
    fn default() -> Self {
        let mut builder = Self {
            tab: Tab::Emoji,
            preset: 0,
            custom: Vec::new(),
            pick: None,
            swatches: (0..PRESET_COUNT)
                .map(|i| draw(&gradient_preset(i), (SWATCH_PX * 2.0) as u32))
                .collect(),
            custom_swatch: None,
            background: None,
            editor: None,
        };
        builder.refresh();
        builder
    }
}

impl Drop for EmojiBuilder {
    fn drop(&mut self) {
        let held = self
            .swatches
            .drain(..)
            .flatten()
            .chain(self.custom_swatch.take())
            .chain(self.background.take());
        super::super::image_budget::retire_all(held);
    }
}

impl EmojiBuilder {
    /// The colors of the chosen background.
    pub(super) fn colors(&self) -> Vec<u32> {
        if self.preset < PRESET_COUNT {
            gradient_preset(self.preset).to_vec()
        } else {
            self.custom.clone()
        }
    }

    /// Draw the preview (and the custom swatch) for the current colors.
    pub(super) fn refresh(&mut self) {
        let colors = self.colors();
        if !colors.is_empty() {
            replace_image(
                &mut self.background,
                draw(&colors, (PREVIEW_PX * 2.0) as u32),
            );
        }
        if !self.custom.is_empty() {
            replace_image(
                &mut self.custom_swatch,
                draw(&self.custom, (SWATCH_PX * 2.0) as u32),
            );
        }
    }

    pub(super) fn close_editor(&mut self) {
        self.editor = None;
    }
}

impl QuillApp {
    /// Fetch the thumbnails of the open tab that are not on disk yet.
    pub(super) fn emoji_prepare_render(&mut self) {
        let missing: Vec<quill::ids::FileId> = {
            let Some(session) = self.session() else {
                return;
            };
            let Some(sources) = self.sources() else {
                return;
            };
            let items: Vec<StickerItem> = match sources.emoji.tab {
                Tab::Emoji => session.users_state.profile_photo_emoji_icons.clone(),
                Tab::Stickers => Self::avatar_stickers(session),
            };
            items
                .iter()
                .filter_map(|item| item.display_file_id())
                .filter(|id| session.should_download(*id))
                .collect()
        };
        if let Some(live) = self.live.as_mut() {
            for id in missing {
                let _ = live.driver.download_file(id, 8);
            }
        }
    }

    /// Stickers the avatar can use: animated or video ones, which tdesktop
    /// keeps (a static sticker has no markup).
    fn avatar_stickers(session: &quill::state::Session) -> Vec<StickerItem> {
        let mut seen = std::collections::HashSet::new();
        session
            .stickers
            .stickers
            .recent
            .iter()
            .chain(&session.stickers.stickers.favorites)
            .filter(|item| {
                item.format != StickerFormat::Webp && item.set_id > 0 && seen.insert(item.id)
            })
            .take(STICKER_LIMIT)
            .cloned()
            .collect()
    }

    fn sticker_visual(&self, item: &StickerItem, size: f32) -> AnyElement {
        let session = self.session();
        let roots = self.media_display_roots();
        let path = item
            .display_file_id()
            .and_then(|id| session.and_then(|s| s.media.files.get(&id.0)))
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, &roots));
        match path {
            Some(path) => img(path)
                .w(px(size))
                .h(px(size))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => div()
                .text_size(px(size * 0.7))
                .child(item.emoji.clone())
                .into_any_element(),
        }
    }

    fn picked_item(&self, pick: Pick) -> Option<StickerItem> {
        let session = self.session()?;
        match pick {
            Pick::Emoji(id) => session
                .users_state
                .profile_photo_emoji_icons
                .iter()
                .find(|item| item.custom_emoji_id == Some(id))
                .cloned(),
            Pick::Sticker { id, .. } => Self::avatar_stickers(session)
                .into_iter()
                .find(|item| item.id == id),
        }
    }

    fn use_emoji_avatar(&mut self, cx: &mut Context<Self>) {
        let Some((pick, colors)) = self
            .sources()
            .and_then(|s| Some((s.emoji.pick?, s.emoji.colors())))
        else {
            return;
        };
        self.finish_photo_source(
            |driver| driver.set_profile_photo_sticker(pick.as_photo_sticker(), &colors),
            cx,
        );
    }

    fn choose_background(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(emoji) = self.sources_mut().map(|s| &mut s.emoji) else {
            return;
        };
        // The custom swatch opens the editor until it has colors, and
        // again when it is already the chosen one.
        if index == PRESET_COUNT && (emoji.custom.is_empty() || emoji.preset == PRESET_COUNT) {
            self.open_gradient_editor(window, cx);
            return;
        }
        emoji.preset = index;
        emoji.refresh();
        cx.notify();
    }

    pub(super) fn emoji_panel(&self, sources: &PhotoSources, cx: &mut Context<Self>) -> AnyElement {
        let builder = &sources.emoji;
        if let Some(editor) = &builder.editor {
            return self.gradient_editor_panel(editor, cx);
        }
        let theme = cx.theme();
        let muted = theme.muted_foreground;
        let primary = theme.primary;
        let transparent = theme.transparent;
        let picked = builder.pick.and_then(|pick| self.picked_item(pick));
        let preview = div()
            .relative()
            .size(px(PREVIEW_PX))
            .flex_none()
            .rounded_full()
            .children(builder.background.clone().map(|image| {
                img(ImageSource::Render(image))
                    .size(px(PREVIEW_PX))
                    .rounded_full()
            }))
            .children(picked.map(|item| {
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(px(PREVIEW_PX))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(self.sticker_visual(&item, PREVIEW_PX * 0.67))
            }));
        let ring = |chosen: bool| if chosen { primary } else { transparent };
        let mut palette = div().flex().gap_2().justify_center();
        for index in 0..PRESET_COUNT {
            palette = palette.child(
                div()
                    .id(("profile-photo-preset", index))
                    .size(px(SWATCH_PX + 4.))
                    .rounded_full()
                    .border_2()
                    .border_color(ring(index == builder.preset))
                    .cursor_pointer()
                    .role(Role::Button)
                    .aria_label(format!("Background {}", index + 1))
                    .tab_index(0)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.choose_background(index, window, cx);
                    }))
                    .children(builder.swatches.get(index).cloned().flatten().map(|image| {
                        img(ImageSource::Render(image))
                            .size(px(SWATCH_PX))
                            .rounded_full()
                    })),
            );
        }
        palette = palette.child(
            div()
                .id("profile-photo-custom-background")
                .size(px(SWATCH_PX + 4.))
                .rounded_full()
                .border_2()
                .border_color(ring(builder.preset == PRESET_COUNT))
                .flex()
                .items_center()
                .justify_center()
                .bg(theme.muted)
                .cursor_pointer()
                .role(Role::Button)
                .aria_label("Custom background")
                .tab_index(0)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.choose_background(PRESET_COUNT, window, cx);
                }))
                .children(builder.custom_swatch.clone().map_or_else(
                    || Some(div().text_sm().child("+").into_any_element()),
                    |image| {
                        Some(
                            img(ImageSource::Render(image))
                                .size(px(SWATCH_PX))
                                .rounded_full()
                                .into_any_element(),
                        )
                    },
                )),
        );
        let tab_button = |id: &'static str, label: &'static str, tab: Tab| {
            let chosen = builder.tab == tab;
            div()
                .id(id)
                .px_3()
                .py_1()
                .rounded_md()
                .text_sm()
                .when(chosen, |d| d.bg(primary.opacity(0.25)))
                .pressable(theme)
                .role(Role::Button)
                .aria_label(label)
                .tab_index(0)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(s) = this.sources_mut() {
                        s.emoji.tab = tab;
                    }
                    cx.notify();
                }))
                .child(label)
        };
        let session = self.session();
        let (items, empty): (Vec<(Pick, StickerItem)>, Option<&str>) = match builder.tab {
            Tab::Emoji => {
                let failed =
                    session.is_some_and(|s| s.users_state.profile_photo_emoji_icons_failed);
                let items: Vec<_> = session
                    .map(|s| {
                        s.users_state
                            .profile_photo_emoji_icons
                            .iter()
                            .filter_map(|item| {
                                Some((Pick::Emoji(item.custom_emoji_id?), item.clone()))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let empty = if failed {
                    Some("Couldn't load the emoji. Go back and open this again to retry.")
                } else if items.is_empty() {
                    Some("Loading emoji…")
                } else {
                    None
                };
                (items, empty)
            }
            Tab::Stickers => {
                let items: Vec<_> = session
                    .map(Self::avatar_stickers)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|item| {
                        (
                            Pick::Sticker {
                                set_id: item.set_id,
                                id: item.id,
                            },
                            item,
                        )
                    })
                    .collect();
                let empty = items
                    .is_empty()
                    .then_some("Your recent and favorite animated stickers show up here.");
                (items, empty)
            }
        };
        let mut grid = div()
            .id("profile-photo-emoji-grid")
            .flex()
            .flex_wrap()
            .gap_1()
            .max_h(px(180.))
            .overflow_y_scroll()
            .role(Role::List)
            .aria_label(match builder.tab {
                Tab::Emoji => "Emoji",
                Tab::Stickers => "Stickers",
            });
        for (ix, (pick, item)) in items.into_iter().enumerate() {
            let tile = div()
                .id(("profile-photo-emoji", ix))
                .size(px(40.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_md()
                .when(builder.pick == Some(pick), |d| d.bg(primary.opacity(0.25)));
            grid = grid.child(
                tile.pressable(theme)
                    .role(Role::Button)
                    .aria_label(if item.emoji.is_empty() {
                        "Item".to_string()
                    } else {
                        format!("Item {}", item.emoji)
                    })
                    .tab_index(0)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(s) = this.sources_mut() {
                            s.emoji.pick = Some(pick);
                        }
                        cx.notify();
                    }))
                    .child(self.sticker_visual(&item, 28.)),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .font_semibold()
                    .child("Use an emoji or sticker as your photo"),
            )
            .child(div().flex().justify_center().child(preview))
            .child(palette)
            .child(
                div()
                    .flex()
                    .gap_1()
                    .child(tab_button("profile-photo-tab-emoji", "Emoji", Tab::Emoji))
                    .child(tab_button(
                        "profile-photo-tab-stickers",
                        "Stickers",
                        Tab::Stickers,
                    )),
            )
            .child(grid)
            .children(empty.map(|text| div().text_sm().text_color(muted).child(text)))
            .child(self.source_buttons(
                "Set photo",
                builder.pick.is_some() && !builder.colors().is_empty(),
                |this, cx| this.use_emoji_avatar(cx),
                cx,
            ))
            .into_any_element()
    }
}
