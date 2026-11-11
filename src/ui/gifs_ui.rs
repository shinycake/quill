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

/// An answer for the session's GIF list is on its way.
fn gifs_on_their_way(panel: &GifPanel) -> bool {
    panel.loading || panel.search_loading
}

/// What the footer's sections choose.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum GifSection {
    Saved,
    Trending,
    Emoji(String),
}

impl GifSection {
    /// The section a window's GIF tab asks for: saved GIFs, or a search
    /// for nothing (trending) or for one of the provider's emoji. Typed
    /// text is none.
    fn of(wanted: &GifWanted, panel: &GifPanel) -> Option<Self> {
        match wanted {
            GifWanted::Saved => Some(Self::Saved),
            GifWanted::Search(query) if query.is_empty() => Some(Self::Trending),
            GifWanted::Search(query) => panel
                .provider_emojis
                .contains(query)
                .then(|| Self::Emoji(query.clone())),
        }
    }
}

/// What one window's GIF tab asks the session's one GIF list for: the
/// saved GIFs, or a search (empty: trending; a footer emoji; typed text).
/// Each window has its own (`PickerUi::gif_wanted`), as each tdesktop
/// window has its own `GifsListWidget`: a footer emoji empties the field
/// but stays the window's section, and another window's search is not
/// this window's.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) enum GifWanted {
    #[default]
    Saved,
    Search(String),
}

/// What a window last asked the session's GIF list for: a window not in
/// use asks for its own section once, so two of them never take the list
/// from each other every frame.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::ui) enum GifAsked {
    #[default]
    Nothing,
    For(GifWanted),
}

/// What one window's GIF tab shows from the session.
#[derive(Clone, Copy)]
pub(super) enum GifShown<'a> {
    /// The session's list holds this window's GIFs: they page, and the
    /// spinner and errors are this window's.
    Live,
    /// Another window's search is in the list: this window's saved GIFs.
    Saved(&'a [AnimationItem]),
    /// Or its search, answered earlier.
    Found(&'a [AnimationItem]),
    /// Or nothing yet: the window asks (`gif_tab_upkeep`).
    Missing,
}

impl GifWanted {
    fn of_section(section: &GifSection) -> Self {
        match section {
            GifSection::Saved => Self::Saved,
            GifSection::Trending => Self::Search(String::new()),
            GifSection::Emoji(emoji) => Self::Search(emoji.clone()),
        }
    }

    /// What the session's list holds now.
    fn shown(panel: &GifPanel) -> Self {
        if panel.search_mode {
            Self::Search(panel.search_query.clone())
        } else {
            Self::Saved
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
        self.request_gifs(GifWanted::Search(query.trim().to_string()), cx);
    }

    /// Put `wanted` in the session's GIF list as this window's. A request
    /// that cannot be sent leaves the window on what the list holds, so
    /// nothing asks again every frame.
    fn request_gifs(&mut self, wanted: GifWanted, cx: &mut Context<Self>) {
        self.pickers.gif_wanted = wanted.clone();
        self.pickers.gif_asked = GifAsked::For(wanted.clone());
        let chat = self.viewed_chat();
        let sent = if let Some(live) = self.live.as_mut() {
            match &wanted {
                GifWanted::Saved => live.driver.show_saved_gifs().is_ok(),
                GifWanted::Search(query) => live.driver.search_gifs(query, chat).is_ok(),
            }
        } else if let Some(session) = self.demo_session.as_mut() {
            match &wanted {
                GifWanted::Saved => session.stickers.gifs.search_mode = false,
                GifWanted::Search(query) => {
                    // No search bot offline: the demo finds nothing.
                    session.stickers.gifs.search_mode = true;
                    session.stickers.gifs.search_query = query.clone();
                    session.accept_gif_search_results(Vec::new(), String::new(), true);
                }
            }
            true
        } else {
            false
        };
        if !sent {
            if let Some(panel) = self.session().map(|session| &session.stickers.gifs) {
                self.pickers.gif_wanted = GifWanted::shown(panel);
            }
            if matches!(wanted, GifWanted::Search(_)) && self.live.is_some() {
                self.set_note(StatusNote::error(tr(keys::Q_MISC_COULD_NOT_SEARCH_GIFS)));
            }
        }
        cx.notify();
    }

    /// A footer section was picked: the field empties (tdesktop cancels its
    /// search) and the section's GIFs show from the top.
    pub(super) fn choose_gif_section(&mut self, section: GifSection, cx: &mut Context<Self>) {
        // Typing that was waiting no longer searches.
        self.pickers.gif_keystroke = self.pickers.gif_keystroke.wrapping_add(1);
        self.pickers.gif_scroll.set_offset(point(px(0.), px(0.)));
        self.request_gifs(GifWanted::of_section(&section), cx);
    }

    /// The GIF tab shows in the window in place: the session keeps one
    /// list for every window, and each window has its own field and
    /// section (`PickerUi::gif_wanted`), so the list follows this
    /// window's. Typed text that has not searched yet searches now.
    pub(super) fn follow_gif_field(&mut self, cx: &mut Context<Self>) {
        let field = self
            .pickers
            .gif_search_input
            .read(cx)
            .value()
            .trim()
            .to_string();
        if !field.is_empty() && self.pickers.gif_wanted != GifWanted::Search(field.clone()) {
            self.search_gifs_for(&field, cx);
            return;
        }
        self.follow_gif_wanted(cx);
    }

    /// The window in use: the session's list holds another window's
    /// GIFs, so ask for this window's again (they page from there). A
    /// section kept from before shows meanwhile, and waits for an answer
    /// on its way to another window instead of cancelling it.
    fn follow_gif_wanted(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.session().map(|session| &session.stickers.gifs) else {
            return;
        };
        let ask = match self.gif_shown(panel) {
            GifShown::Live | GifShown::Saved(_) => false,
            GifShown::Found(_) => !gifs_on_their_way(panel),
            GifShown::Missing => true,
        };
        if ask {
            let wanted = self.pickers.gif_wanted.clone();
            self.request_gifs(wanted, cx);
        }
    }

    /// Frame upkeep: every window with the GIF tab open shows its own
    /// section from the session's one list. The window the user last
    /// pressed in has its GIFs in the list; another shows its saved GIFs
    /// or its search kept from before, and when it has neither it asks
    /// once, after any answer on its way, without a press.
    pub(in crate::ui) fn gif_tab_upkeep(&mut self, cx: &mut Context<Self>) {
        if !self.pickers.media_panel.shows_gifs() {
            return;
        }
        if self.windows.pressed_here() {
            self.follow_gif_wanted(cx);
            return;
        }
        let Some(panel) = self.session().map(|session| &session.stickers.gifs) else {
            return;
        };
        let wanted = self.pickers.gif_wanted.clone();
        if matches!(self.gif_shown(panel), GifShown::Missing)
            && !gifs_on_their_way(panel)
            && self.pickers.gif_asked != GifAsked::For(wanted.clone())
        {
            self.request_gifs(wanted, cx);
        }
    }

    /// What this window's tab shows from the session's list.
    pub(super) fn gif_shown<'a>(&self, panel: &'a GifPanel) -> GifShown<'a> {
        if GifWanted::shown(panel) == self.pickers.gif_wanted {
            return GifShown::Live;
        }
        match &self.pickers.gif_wanted {
            GifWanted::Saved if panel.loaded || !panel.animations.is_empty() => {
                GifShown::Saved(&panel.animations)
            }
            GifWanted::Saved => GifShown::Missing,
            GifWanted::Search(query) => panel
                .searches_kept
                .get(query)
                .map_or(GifShown::Missing, GifShown::Found),
        }
    }

    /// Shown in the field: only for the list this window asked for.
    fn gifs_loading(&self, panel: &GifPanel) -> bool {
        matches!(self.gif_shown(panel), GifShown::Live) && gifs_on_their_way(panel)
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
        if !matches!(self.gif_shown(panel), GifShown::Live) {
            return;
        }
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
            self.set_note(StatusNote::error(tr(keys::Q_MISC_COULD_NOT_LOAD_MORE_GIFS)));
        }
    }

    fn save_gif_pick(&mut self, file_id: FileId, cx: &mut Context<Self>) {
        if let Some(live) = self.live.as_mut() {
            self.screen.connection.set_note_for(
                &self.screen.windows,
                match live.driver.set_gif_saved(file_id, true) {
                    Ok(_) => StatusNote::silent(tr(keys::Q_MISC_SAVING_GIF)),
                    Err(_) => StatusNote::error(tr(keys::Q_MISC_COULD_NOT_SAVE_GIF)),
                },
            );
        }
        cx.notify();
    }

    /// The GIFs tab of the media panel.
    pub(super) fn gif_picker_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        // Clips whose tiles do not render in this pass stop.
        self.players
            .inline_videos
            .begin_panel_render(self.windows.scope, Panel::Gifs);
        let no_panel = GifPanel::default();
        let panel = self
            .session()
            .map_or(&no_panel, |session| &session.stickers.gifs);
        let loading = self.gifs_loading(panel);
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
        let shown = self.gif_shown(panel);
        let animations: &[AnimationItem] = match shown {
            GifShown::Live => panel.visible_animations(),
            GifShown::Saved(animations) | GifShown::Found(animations) => animations,
            GifShown::Missing => &[],
        };
        let body = if animations.is_empty() {
            self.gif_panel_status(panel, shown, cx)
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
    fn gif_panel_status(
        &self,
        panel: &GifPanel,
        shown: GifShown<'_>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = if let GifShown::Saved(_) = shown {
            Some(tr(keys::Q_MISC_NO_SAVED_GIFS_YET_RIGHT))
        } else if let GifShown::Found(_) = shown {
            Some(tr(keys::Q_MISC_NO_GIFS_FOUND))
        } else if matches!(shown, GifShown::Missing) || gifs_on_their_way(panel) {
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
        let shown = GifSection::of(&self.pickers.gif_wanted, panel);
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
