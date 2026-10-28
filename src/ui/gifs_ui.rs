//! The media panel's GIFs tab (tdesktop `GifsListWidget`): a search field
//! that searches as you type, the saved or found GIFs in justified rows
//! that play muted on loop, and a footer of sections (saved, trending and
//! the search provider's emoji). The packing is `quill::gif_grid`.
#![deny(clippy::indexing_slicing)]
use super::app::QuillApp;
use super::dialogs::GroupConfirmAction;
use super::inline_players::Panel;
use super::inline_video::InlineTile;
use super::pressable::PressableDiv;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::skeleton::Skeleton;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::gif_grid::{GIF_GRID_PADDING, layout_gifs};
use quill::ids::{ChatId, FileId};
use quill::inline_grid::{MEDIA_HEIGHT, Placed, SKIP, should_load_more};
use quill::lang::{keys, tr, tr_with};
use quill::state::{GifPanel, RequestPurpose};
use quill::telegram::envelope::AnimationItem;
use std::path::PathBuf;
use std::time::Duration;

/// tdesktop `kSearchRequestDelay`: typing waits this long before searching.
const SEARCH_DELAY: Duration = Duration::from_millis(400);

/// The grid's width: the panel's, inside its border.
const GRID_WIDTH: i32 = super::media_panel::PANEL_WIDTH as i32 - 2;

/// The corners of a tile (the inline results' cells have the same).
const TILE_RADIUS: Pixels = px(4.);

/// A tile whose picture could not be decoded: a plain block, never text,
/// so nothing can spill out of it.
fn empty_tile(color: Hsla) -> AnyElement {
    div()
        .size_full()
        .rounded(TILE_RADIUS)
        .bg(color)
        .into_any_element()
}

/// What the footer's sections choose.
#[derive(Clone, PartialEq, Eq)]
enum GifSection {
    Saved,
    Trending,
    Emoji(String),
}

impl GifSection {
    /// The section the panel shows: saved GIFs, or a search for nothing
    /// (trending) or for one of the provider's emoji. Typed text is none.
    fn shown(panel: &GifPanel) -> Option<Self> {
        if !panel.search_mode {
            Some(Self::Saved)
        } else if panel.search_query.is_empty() {
            Some(Self::Trending)
        } else {
            panel
                .provider_emojis
                .contains(&panel.search_query)
                .then(|| Self::Emoji(panel.search_query.clone()))
        }
    }
}

impl QuillApp {
    /// The search field changed: an empty field shows the saved GIFs at
    /// once, text searches once typing pauses.
    pub(super) fn gif_search_typed(&mut self, cx: &mut Context<Self>) {
        self.pickers.gif_keystroke = self.pickers.gif_keystroke.wrapping_add(1);
        let keystroke = self.pickers.gif_keystroke;
        let query = self
            .pickers
            .gif_search_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        if query.is_empty() {
            self.choose_gif_section(GifSection::Saved, cx);
            return;
        }
        self.spawn_scoped(cx, async move |this, cx| {
            cx.background_executor().timer(SEARCH_DELAY).await;
            this.update(cx, |this, cx| {
                if this.pickers.gif_keystroke == keystroke {
                    this.search_gifs_for(&query, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn search_gifs_for(&mut self, query: &str, cx: &mut Context<Self>) {
        self.pickers.gif_scroll.set_offset(point(px(0.), px(0.)));
        if let Some(live) = self.live.as_mut() {
            if live.driver.search_gifs(query).is_err() {
                self.connection
                    .set_note(StatusNote::error(tr(keys::Q_MISC_COULD_NOT_SEARCH_GIFS)));
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            // No search bot offline: the demo finds nothing.
            let gifs = &mut session.stickers.gifs;
            gifs.search_mode = true;
            gifs.search_query = query.trim().to_string();
            gifs.search_results.clear();
        }
        cx.notify();
    }

    /// A footer section was picked: the field empties (tdesktop cancels its
    /// search) and the section's GIFs show from the top.
    fn choose_gif_section(&mut self, section: GifSection, cx: &mut Context<Self>) {
        // Typing that was waiting no longer searches.
        self.pickers.gif_keystroke = self.pickers.gif_keystroke.wrapping_add(1);
        match section {
            GifSection::Saved => {
                self.pickers.gif_scroll.set_offset(point(px(0.), px(0.)));
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.show_saved_gifs();
                } else if let Some(session) = self.demo_session.as_mut() {
                    session.stickers.gifs.search_mode = false;
                }
                cx.notify();
            }
            GifSection::Trending => self.search_gifs_for("", cx),
            GifSection::Emoji(emoji) => self.search_gifs_for(&emoji, cx),
        }
    }

    fn clear_gif_search_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pickers
            .gif_search_input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    /// tdesktop pages the search once the scroll position gets within one
    /// viewport of the end (`GifsListWidget::checkLoadMore`).
    fn load_more_gifs_if_needed(&mut self) {
        let Some(panel) = self.session().map(|session| &session.stickers.gifs) else {
            return;
        };
        let content = layout_gifs(panel.visible_animations(), GRID_WIDTH).height;
        let handle = &self.pickers.gif_scroll;
        let viewport = f32::from(handle.bounds().size.height);
        let scroll_top = -f32::from(handle.offset().y);
        if !should_load_more(
            scroll_top,
            viewport,
            content as f32,
            panel.search_mode && !panel.search_next_offset.is_empty(),
            panel.search_loading,
        ) {
            return;
        }
        if let Some(live) = self.live.as_mut()
            && live.driver.more_gif_search_results().is_err()
        {
            self.connection
                .set_note(StatusNote::error(tr(keys::Q_MISC_COULD_NOT_LOAD_MORE_GIFS)));
        }
    }

    fn save_gif_pick(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.connection
                .set_note(match live.driver.set_gif_saved(file_id, true) {
                    Ok(_) => StatusNote::silent(tr(keys::Q_MISC_SAVING_GIF)),
                    Err(_) => StatusNote::error(tr(keys::Q_MISC_COULD_NOT_SAVE_GIF)),
                });
        }
        cx.notify();
    }

    /// The GIFs tab of the media panel.
    pub(super) fn gif_picker_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        // Clips whose tiles do not render in this pass stop.
        self.playback.inline_videos.begin_panel_render(Panel::Gifs);
        let no_panel = GifPanel::default();
        let panel = self
            .session()
            .map_or(&no_panel, |session| &session.stickers.gifs);
        let loading = panel.loading || panel.search_loading;
        let muted = cx.theme().muted_foreground;
        let search = div().px_2().py_2().child(
            Input::new(&self.pickers.gif_search_input)
                .small()
                .cleanable(true)
                .prefix(
                    Icon::new(gpui_kit::assets::IconName::Search)
                        .size(px(14.))
                        .text_color(muted),
                )
                .when(loading, |input| input.suffix(Spinner::new().small()))
                .aria_label(tr(keys::GIFS_SEARCH)),
        );
        let animations = panel.visible_animations();
        let body = if animations.is_empty() {
            self.gif_panel_status(panel, cx)
        } else {
            self.gif_grid(panel, animations, cx)
        };
        div()
            .id("gif-picker")
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(search)
            .child(body)
            .child(self.gif_sections(panel, cx))
            .into_any_element()
    }

    /// Why the grid is empty. Nothing while the first answer is on its way
    /// (the field's spinner says so).
    fn gif_panel_status(&self, panel: &GifPanel, cx: &mut Context<Self>) -> AnyElement {
        let text = if panel.loading || panel.search_loading {
            None
        } else if panel.search_mode && panel.search_failed && panel.search_bot_username.is_empty() {
            Some(tr(keys::Q_MISC_GIF_SEARCH_IS_NOT_AVAILABLE))
        } else if panel.failed || panel.search_mode && panel.search_failed {
            Some(tr(keys::Q_MISC_COULD_NOT_UPDATE_GIFS_TRY))
        } else if panel.search_mode {
            Some(tr(keys::Q_MISC_NO_GIFS_FOUND))
        } else {
            Some(tr(keys::Q_MISC_NO_SAVED_GIFS_YET_RIGHT))
        };
        div()
            .id("gif-panel-status")
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .px_6()
            .text_sm()
            .text_center()
            .text_color(cx.theme().muted_foreground)
            .children(text)
            .into_any_element()
    }

    fn gif_grid(
        &self,
        panel: &GifPanel,
        animations: &[AnimationItem],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mosaic = layout_gifs(animations, GRID_WIDTH);
        let roots = self.media_display_roots();
        let pending = self.session().is_some_and(|session| {
            session
                .requests
                .has_purpose(RequestPurpose::AddSavedAnimation)
                || session
                    .requests
                    .has_purpose(RequestPurpose::RemoveSavedAnimation)
        });
        let mut grid = div().flex().flex_col().pb(px(GIF_GRID_PADDING as f32));
        for row in &mosaic.rows {
            let mut line = div()
                .flex()
                .items_start()
                .gap(px(SKIP as f32))
                .px(px(GIF_GRID_PADDING as f32))
                .h(px(row.height as f32))
                .flex_none();
            for cell in &row.cells {
                let Some(animation) = animations.get(cell.item.result) else {
                    continue;
                };
                let saved = panel
                    .animations
                    .iter()
                    .any(|item| item.file_id == animation.file_id);
                line = line.child(self.gif_tile(animation, cell, saved, pending, &roots, cx));
            }
            grid = grid.child(line);
        }
        div()
            .id("gif-grid")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.pickers.gif_scroll)
            .on_scroll(cx.listener(|this, _, _, _| this.load_more_gifs_if_needed()))
            .child(grid)
            .into_any_element()
    }

    /// One GIF: its still picture (or the blurred minithumbnail, or a
    /// placeholder while neither has arrived) with the clip playing over
    /// it once downloaded. A click sends it; the context menu saves or
    /// removes it.
    fn gif_tile(
        &self,
        animation: &AnimationItem,
        cell: &Placed,
        saved: bool,
        pending: bool,
        roots: &[PathBuf],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let index = cell.item.result;
        let (w, h) = (px(cell.width as f32), px(MEDIA_HEIGHT as f32));
        let muted = cx.theme().muted;
        let still = animation
            .still_thumb()
            .and_then(|id| self.inline_image_path(id, roots))
            .map(|path| {
                img(path)
                    .size_full()
                    .rounded(TILE_RADIUS)
                    .object_fit(ObjectFit::Cover)
                    .with_fallback(move || empty_tile(muted))
                    .into_any_element()
            })
            .or_else(|| {
                animation
                    .minithumbnail
                    .as_ref()
                    .filter(|mini| !mini.data.is_empty())
                    .map(|mini| {
                        img(super::mini_image::mini_image(mini))
                            .size_full()
                            .rounded(TILE_RADIUS)
                            .object_fit(ObjectFit::Cover)
                            .with_fallback(move || empty_tile(muted))
                            .into_any_element()
                    })
            })
            .unwrap_or_else(|| {
                Skeleton::new()
                    .size_full()
                    .rounded(TILE_RADIUS)
                    .into_any_element()
            });
        let clip = self.gif_clip_player(animation, cell, roots, cx);
        let popover = cx.theme().popover;
        let (file_id, duration, width, height) = (
            animation.file_id,
            animation.duration,
            animation.width,
            animation.height,
        );
        let owner = cx.entity().downgrade();
        div()
            .id(("gif-tile", index as u64))
            .relative()
            .w(w)
            .h(h)
            .flex_none()
            .overflow_hidden()
            .rounded(TILE_RADIUS)
            .role(gpui_kit::Role::Button)
            .aria_label(tr_with(
                keys::Q_MISC_SEND_GIF,
                &[("index_1", &(index + 1).to_string())],
            ))
            .tab_index(0)
            .cursor_pointer()
            .pressable(cx.theme())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.send_gif_pick(file_id, duration, width, height, cx);
            }))
            .child(div().absolute().inset_0().child(still))
            .children(clip.map(|player| {
                div().absolute().inset_0().child(
                    super::video_player::video_frame(&player)
                        .object_fit(ObjectFit::Cover)
                        .corner_radii(Corners::all(TILE_RADIUS), popover),
                )
            }))
            .context_menu(move |menu, _, _| {
                let owner = owner.clone();
                menu.item(
                    PopupMenuItem::new(if saved {
                        tr(keys::Q_MISC_REMOVE_FROM_SAVED_GIFS)
                    } else {
                        tr(keys::Q_MISC_SAVE_GIF)
                    })
                    .disabled(pending)
                    .on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            if saved {
                                this.open_group_confirm(
                                    ChatId(0),
                                    GroupConfirmAction::RemoveSavedGif { file_id },
                                    cx,
                                );
                            } else {
                                this.save_gif_pick(file_id, cx);
                            }
                        });
                    }),
                )
            })
            .into_any_element()
    }

    /// The tile's clip: the GIF itself once downloaded, else its MP4
    /// preview loop (tdesktop `VideoPreviewState`).
    fn gif_clip_player(
        &self,
        animation: &AnimationItem,
        cell: &Placed,
        roots: &[PathBuf],
        cx: &mut Context<Self>,
    ) -> Option<super::video_player::VideoPlayer> {
        let cell_size = (cell.width as f32, MEDIA_HEIGHT as f32);
        let full = (animation.file_id, (animation.width, animation.height));
        let preview = animation
            .preview_clip()
            .map(|id| (id, (animation.thumb_width, animation.thumb_height)));
        [Some(full), preview]
            .into_iter()
            .flatten()
            .find(|(id, _)| self.inline_image_path(*id, roots).is_some())
            .and_then(|(id, (width, height))| {
                self.panel_clip_player(
                    Panel::Gifs,
                    id,
                    InlineTile::cell(width, height, cell_size),
                    roots,
                    cx,
                )
            })
    }

    /// The footer: saved GIFs, trending and the search provider's emoji,
    /// like the sticker and emoji tabs' section strip.
    fn gif_sections(&self, panel: &GifPanel, cx: &mut Context<Self>) -> AnyElement {
        let shown = GifSection::shown(panel);
        let glyph = |icon| Icon::new(icon).size(px(18.)).into_any_element();
        let mut sections = vec![
            (
                GifSection::Saved,
                glyph(gpui_kit::assets::IconName::Clock),
                SharedString::from(tr(keys::Q_MISC_SAVED)),
            ),
            (
                GifSection::Trending,
                glyph(gpui_kit::assets::IconName::TrendingUp),
                SharedString::from(tr(keys::Q_MISC_TRENDING)),
            ),
        ];
        sections.extend(panel.provider_emojis.iter().map(|emoji| {
            (
                GifSection::Emoji(emoji.clone()),
                div()
                    .text_size(px(18.))
                    .child(emoji.clone())
                    .into_any_element(),
                SharedString::from(emoji.clone()),
            )
        }));
        div()
            .id("gif-sections")
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .border_t_1()
            .border_color(cx.theme().border)
            .overflow_x_scroll()
            .children(
                sections
                    .into_iter()
                    .enumerate()
                    .map(|(index, (section, icon, label))| {
                        let selected = shown.as_ref() == Some(&section);
                        let tooltip = !matches!(section, GifSection::Emoji(_));
                        super::media_panel::section_chip(
                            ("gif-section", index as u64),
                            icon,
                            label,
                            tooltip,
                            selected,
                            cx.listener(move |this, _, window, cx| {
                                this.clear_gif_search_field(window, cx);
                                this.choose_gif_section(section.clone(), cx);
                            }),
                            cx,
                        )
                    }),
            )
            .into_any_element()
    }
}
