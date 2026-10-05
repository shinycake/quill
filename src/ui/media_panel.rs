//! The composer's emoji / sticker / GIF panel, after Telegram Desktop's
//! tabbed selector: one popover above the composer with three tabs, each a
//! virtualized list of sections (set header + grid rows), a footer strip of
//! section icons to jump between them, and a search field. Set contents load
//! lazily as their sections scroll into view; cells show static images and
//! only the hovered sticker animates.
use super::app::QuillApp;
use gpui_kit::component::button::*;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::*;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use quill::emoji_catalog::{CATEGORIES, catalog};
use quill::ids::FileId;
use quill::local_path::sandboxed_display_path;
use quill::telegram::envelope::StickerItem;

pub(super) const PANEL_WIDTH: f32 = 404.;
const PANEL_HEIGHT: f32 = 452.;
const EMOJI_CELL: f32 = 40.;
const EMOJI_COLS: usize = 9;
const STICKER_CELL: f32 = 72.;
const STICKER_COLS: usize = 5;
const CUSTOM_EMOJI_SIZE: f32 = 30.;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum PanelTab {
    #[default]
    Emoji,
    Stickers,
    Gifs,
}

/// Where a sticker cell's item lives (rows keep references, not clones).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StickerSource {
    /// `Session::media_library` set contents.
    Set(i64),
    Recent,
    Favorites,
    Found,
    /// `Session::emoji.custom_emoji_stickers` (resolved custom emoji).
    CustomEmoji,
}

#[derive(Clone, Debug)]
pub(super) enum PanelCell {
    Emoji(SharedString),
    /// A custom emoji from a pack (or recent custom emoji).
    Custom {
        source: StickerSource,
        ix: usize,
    },
    Sticker {
        source: StickerSource,
        ix: usize,
    },
    /// A set whose contents haven't loaded yet.
    Placeholder {
        set_id: i64,
    },
}

#[derive(Clone, Debug)]
pub(super) enum PanelRow {
    Header {
        label: SharedString,
        set_id: Option<i64>,
    },
    Cells {
        cells: Vec<PanelCell>,
        sticker: bool,
    },
}

#[derive(Clone, Debug)]
pub(super) enum SectionIcon {
    Emoji(&'static str),
    Glyph(gpui_kit::assets::IconName),
    /// A set's first item (once loaded).
    Set(i64),
}

#[derive(Clone, Debug)]
pub(super) struct PanelSection {
    pub label: SharedString,
    pub icon: SectionIcon,
    pub first_row: usize,
}

/// What the rows were built from; a change rebuilds them.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PanelKey {
    tab: PanelTab,
    query: String,
    session_revision: u64,
    recent_emoji: usize,
    premium: bool,
}

pub(super) struct MediaPanel {
    pub open: bool,
    pub tab: PanelTab,
    pub list: ListState,
    pub rows: Vec<PanelRow>,
    pub sections: Vec<PanelSection>,
    pub key: Option<PanelKey>,
    pub active_section: usize,
    pub hovered: Option<FileId>,
    /// The sticker search query last sent to TDLib.
    pub searched: String,
}

impl Default for MediaPanel {
    fn default() -> Self {
        Self {
            open: false,
            tab: PanelTab::default(),
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            rows: Vec::new(),
            sections: Vec::new(),
            key: None,
            active_section: 0,
            hovered: None,
            searched: String::new(),
        }
    }
}

/// Skin-tone variants are offered from the base emoji, not as grid cells.
fn has_skin_tone(emoji: &str) -> bool {
    emoji
        .chars()
        .any(|c| ('\u{1F3FB}'..='\u{1F3FF}').contains(&c))
}

/// One representative glyph per emoji category for the footer strip.
fn category_glyph(category: &str) -> &'static str {
    match category {
        "Smileys & Emotion" => "😀",
        "People & Body" => "👋",
        "Animals & Nature" => "🐻",
        "Food & Drink" => "🍔",
        "Travel & Places" => "🚗",
        "Activities" => "⚽",
        "Objects" => "💡",
        "Symbols" => "🔣",
        "Flags" => "🏁",
        _ => "😀",
    }
}

fn push_grid(
    rows: &mut Vec<PanelRow>,
    cells: impl IntoIterator<Item = PanelCell>,
    cols: usize,
    sticker: bool,
) {
    let mut row = Vec::with_capacity(cols);
    for cell in cells {
        row.push(cell);
        if row.len() == cols {
            rows.push(PanelRow::Cells {
                cells: std::mem::take(&mut row),
                sticker,
            });
        }
    }
    if !row.is_empty() {
        rows.push(PanelRow::Cells {
            cells: row,
            sticker,
        });
    }
}

impl QuillApp {
    pub(super) fn media_panel_open(&self) -> bool {
        self.media_panel.open
    }

    pub(super) fn toggle_media_panel(&mut self, tab: PanelTab, cx: &mut Context<Self>) {
        if self.media_panel.open && self.media_panel.tab == tab {
            self.close_media_panel(cx);
        } else {
            self.open_media_panel(tab, cx);
        }
    }

    pub(super) fn open_media_panel(&mut self, tab: PanelTab, cx: &mut Context<Self>) {
        if self.recording_active() {
            self.cancel_recording(cx);
        }
        self.media_panel.open = true;
        self.set_media_panel_tab(tab, cx);
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.open_media_panel();
            if tab == PanelTab::Gifs {
                let _ = live.driver.open_gif_panel();
            }
        } else if tab == PanelTab::Gifs
            && let Some(session) = self.demo_session.as_mut()
        {
            session.gifs.open = true;
        }
        cx.notify();
    }

    pub(super) fn close_media_panel(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.media_panel.open {
            return false;
        }
        self.media_panel.open = false;
        self.media_panel.hovered = None;
        if let Some(live) = self.live.as_mut() {
            live.driver.close_gif_panel();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.gifs.close();
        }
        cx.notify();
        true
    }

    fn set_media_panel_tab(&mut self, tab: PanelTab, cx: &mut Context<Self>) {
        if self.media_panel.tab != tab {
            self.media_panel.tab = tab;
            self.media_panel.key = None;
            self.media_panel.active_section = 0;
            if tab == PanelTab::Gifs {
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.open_gif_panel();
                } else if let Some(session) = self.demo_session.as_mut() {
                    session.gifs.open = true;
                }
            }
        }
        cx.notify();
    }

    /// The item a sticker / custom-emoji cell refers to.
    pub(super) fn panel_item(&self, source: StickerSource, ix: usize) -> Option<&StickerItem> {
        let session = self.session()?;
        match source {
            StickerSource::Set(id) => session.media_library.set_stickers.get(&id)?.get(ix),
            StickerSource::Recent => session.stickers.recent.get(ix),
            StickerSource::Favorites => session.stickers.favorites.get(ix),
            StickerSource::Found => session.stickers.found_stickers.get(ix),
            StickerSource::CustomEmoji => session.emoji.custom_emoji_stickers.get(ix),
        }
    }

    fn panel_query(&self, cx: &App) -> String {
        match self.media_panel.tab {
            PanelTab::Emoji => self.emoji_search_input.read(cx).value().trim().to_string(),
            PanelTab::Stickers => self
                .sticker_search_input
                .read(cx)
                .value()
                .trim()
                .to_string(),
            PanelTab::Gifs => String::new(),
        }
    }

    /// Rebuild the rows when what they're made of changed; keeps the
    /// scroll position across rebuilds of the same tab and query.
    fn sync_media_panel_rows(&mut self, cx: &mut Context<Self>) {
        let query = self.panel_query(cx);
        let key = PanelKey {
            tab: self.media_panel.tab,
            query: query.clone(),
            session_revision: self.session().map_or(0, |s| s.revision),
            recent_emoji: self
                .session()
                .map_or(0, |s| s.media_prefs.recent_emoji.len()),
            premium: self.session().is_some_and(|s| s.my_is_premium()),
        };
        if self.media_panel.key.as_ref() == Some(&key) {
            return;
        }
        let same_view = self
            .media_panel
            .key
            .as_ref()
            .is_some_and(|old| old.tab == key.tab && old.query == key.query);
        let (rows, sections) = match key.tab {
            PanelTab::Emoji => self.build_emoji_rows(&query),
            PanelTab::Stickers => self.build_sticker_rows(&query),
            PanelTab::Gifs => (Vec::new(), Vec::new()),
        };
        let top = self.media_panel.list.logical_scroll_top();
        self.media_panel.list.reset(rows.len());
        if same_view && top.item_ix < rows.len() {
            self.media_panel.list.scroll_to(top);
        }
        self.media_panel.rows = rows;
        self.media_panel.sections = sections;
        self.media_panel.key = Some(key);
        // Sticker search goes to TDLib (by emoji or keyword).
        if self.media_panel.tab == PanelTab::Stickers && query != self.media_panel.searched {
            self.media_panel.searched = query.clone();
            if !query.is_empty()
                && let Some(live) = self.live.as_mut()
            {
                let _ = live.driver.search_sticker_picker(&query);
            }
        }
    }

    fn build_emoji_rows(&self, query: &str) -> (Vec<PanelRow>, Vec<PanelSection>) {
        let mut rows = Vec::new();
        let mut sections = Vec::new();
        if !query.is_empty() {
            rows.push(PanelRow::Header {
                label: "Search results".into(),
                set_id: None,
            });
            push_grid(
                &mut rows,
                quill::emoji_catalog::search(None, query)
                    .filter(|entry| !has_skin_tone(entry.emoji))
                    .map(|entry| PanelCell::Emoji(SharedString::new_static(entry.emoji))),
                EMOJI_COLS,
                false,
            );
            return (rows, sections);
        }
        let session = self.session();
        // Recent: plain emoji, then recently used custom emoji.
        let recent: Vec<PanelCell> = session
            .map(|s| {
                let custom = s
                    .media_prefs
                    .recent_custom_emoji_ids
                    .iter()
                    .filter_map(|id| {
                        s.emoji
                            .custom_emoji_stickers
                            .iter()
                            .position(|item| item.custom_emoji_id == Some(*id))
                            .map(|ix| PanelCell::Custom {
                                source: StickerSource::CustomEmoji,
                                ix,
                            })
                    });
                s.media_prefs
                    .recent_emoji
                    .iter()
                    .take(quill::emoji_catalog::RECENT_LIMIT)
                    .map(|emoji| PanelCell::Emoji(SharedString::from(emoji.clone())))
                    .chain(custom.take(EMOJI_COLS * 2))
                    .collect()
            })
            .unwrap_or_default();
        if !recent.is_empty() {
            sections.push(PanelSection {
                label: "Recently used".into(),
                icon: SectionIcon::Glyph(gpui_kit::assets::IconName::Clock),
                first_row: rows.len(),
            });
            rows.push(PanelRow::Header {
                label: "Recently used".into(),
                set_id: None,
            });
            push_grid(&mut rows, recent, EMOJI_COLS, false);
        }
        for category in CATEGORIES {
            sections.push(PanelSection {
                label: SharedString::new_static(category),
                icon: SectionIcon::Emoji(category_glyph(category)),
                first_row: rows.len(),
            });
            rows.push(PanelRow::Header {
                label: SharedString::new_static(category),
                set_id: None,
            });
            push_grid(
                &mut rows,
                catalog()
                    .filter(|entry| entry.category == *category && !has_skin_tone(entry.emoji))
                    .map(|entry| PanelCell::Emoji(SharedString::new_static(entry.emoji))),
                EMOJI_COLS,
                false,
            );
        }
        // Custom emoji packs after the standard emoji.
        if let Some(session) = session {
            for set in &session.emoji.installed_sets {
                let title: SharedString = if set.title.is_empty() {
                    set.name.clone().into()
                } else {
                    set.title.clone().into()
                };
                sections.push(PanelSection {
                    label: title.clone(),
                    icon: SectionIcon::Set(set.id),
                    first_row: rows.len(),
                });
                rows.push(PanelRow::Header {
                    label: title,
                    set_id: Some(set.id),
                });
                match session.media_library.set_stickers.get(&set.id) {
                    Some(items) => push_grid(
                        &mut rows,
                        (0..items.len()).map(|ix| PanelCell::Custom {
                            source: StickerSource::Set(set.id),
                            ix,
                        }),
                        EMOJI_COLS,
                        false,
                    ),
                    None => push_grid(
                        &mut rows,
                        (0..set.size.max(1) as usize)
                            .map(|_| PanelCell::Placeholder { set_id: set.id }),
                        EMOJI_COLS,
                        false,
                    ),
                }
            }
        }
        (rows, sections)
    }

    fn build_sticker_rows(&self, query: &str) -> (Vec<PanelRow>, Vec<PanelSection>) {
        let mut rows = Vec::new();
        let mut sections = Vec::new();
        let Some(session) = self.session() else {
            return (rows, sections);
        };
        if !query.is_empty() {
            rows.push(PanelRow::Header {
                label: "Search results".into(),
                set_id: None,
            });
            push_grid(
                &mut rows,
                (0..session.stickers.found_stickers.len()).map(|ix| PanelCell::Sticker {
                    source: StickerSource::Found,
                    ix,
                }),
                STICKER_COLS,
                true,
            );
            return (rows, sections);
        }
        for (source, label, icon, len) in [
            (
                StickerSource::Favorites,
                "Favorites",
                gpui_kit::assets::IconName::Star,
                session.stickers.favorites.len(),
            ),
            (
                StickerSource::Recent,
                "Recently used",
                gpui_kit::assets::IconName::Clock,
                session.stickers.recent.len(),
            ),
        ] {
            if len == 0 {
                continue;
            }
            sections.push(PanelSection {
                label: label.into(),
                icon: SectionIcon::Glyph(icon),
                first_row: rows.len(),
            });
            rows.push(PanelRow::Header {
                label: label.into(),
                set_id: None,
            });
            push_grid(
                &mut rows,
                (0..len.min(STICKER_COLS * 4)).map(|ix| PanelCell::Sticker { source, ix }),
                STICKER_COLS,
                true,
            );
        }
        for set in &session.stickers.sets {
            let title: SharedString = if set.title.is_empty() {
                set.name.clone().into()
            } else {
                set.title.clone().into()
            };
            sections.push(PanelSection {
                label: title.clone(),
                icon: SectionIcon::Set(set.id),
                first_row: rows.len(),
            });
            rows.push(PanelRow::Header {
                label: title,
                set_id: Some(set.id),
            });
            match session.media_library.set_stickers.get(&set.id) {
                Some(items) => push_grid(
                    &mut rows,
                    (0..items.len()).map(|ix| PanelCell::Sticker {
                        source: StickerSource::Set(set.id),
                        ix,
                    }),
                    STICKER_COLS,
                    true,
                ),
                None => push_grid(
                    &mut rows,
                    (0..set.size.max(1) as usize)
                        .map(|_| PanelCell::Placeholder { set_id: set.id }),
                    STICKER_COLS,
                    true,
                ),
            }
        }
        (rows, sections)
    }

    /// The static image for a sticker / custom emoji cell, when local.
    pub(super) fn panel_still(&self, item: &StickerItem) -> Option<std::path::PathBuf> {
        let roots = self.media_display_roots();
        let session = self.session()?;
        [item.display_file_id(), item.thumb_file_id]
            .into_iter()
            .flatten()
            .filter_map(|id| session.files.get(&id.0)?.usable_path())
            .find_map(|path| sandboxed_display_path(path, &roots))
    }

    /// One virtualized row; also asks for what it needs to show (set
    /// contents, cell files).
    pub(super) fn render_panel_row(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.media_panel.rows.get(ix).cloned() else {
            return div().into_any_element();
        };
        let mut need_sets: Vec<i64> = Vec::new();
        let mut need_files: Vec<FileId> = Vec::new();
        let element = match row {
            PanelRow::Header { label, set_id } => {
                need_sets.extend(set_id);
                div()
                    .px_3()
                    .pt_3()
                    .pb_1()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .truncate()
                    .child(label)
                    .into_any_element()
            }
            PanelRow::Cells { cells, sticker } => {
                let mut line = div().flex().px_2();
                for (col, cell) in cells.into_iter().enumerate() {
                    let id = (ix * 16 + col) as u64;
                    line = line.child(match cell {
                        PanelCell::Emoji(emoji) => self.emoji_cell(id, emoji, cx),
                        PanelCell::Custom {
                            source,
                            ix: item_ix,
                        } => match self.panel_item(source, item_ix).cloned() {
                            Some(item) => {
                                if let Some(file) = item.display_file_id() {
                                    need_files.push(file);
                                }
                                self.custom_emoji_cell(id, item, cx)
                            }
                            None => placeholder_cell(EMOJI_CELL, cx),
                        },
                        PanelCell::Sticker {
                            source,
                            ix: item_ix,
                        } => match self.panel_item(source, item_ix).cloned() {
                            Some(item) => {
                                if let Some(file) = item.display_file_id() {
                                    need_files.push(file);
                                }
                                self.panel_sticker_cell(id, item, cx)
                            }
                            None => placeholder_cell(STICKER_CELL, cx),
                        },
                        PanelCell::Placeholder { set_id } => {
                            need_sets.push(set_id);
                            placeholder_cell(if sticker { STICKER_CELL } else { EMOJI_CELL }, cx)
                        }
                    });
                }
                line.into_any_element()
            }
        };
        if !need_sets.is_empty() || !need_files.is_empty() {
            let weak = cx.weak_entity();
            cx.defer(move |cx| {
                let _ = weak.update(cx, |this, _| {
                    if let Some(live) = this.live.as_mut() {
                        let _ = live.driver.ensure_library_sets(&need_sets);
                        let _ = live.driver.ensure_media_files(&need_files);
                    }
                });
            });
        }
        element
    }

    fn emoji_cell(&self, id: u64, emoji: SharedString, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id(("panel-emoji", id))
            .size(px(EMOJI_CELL))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .text_size(px(26.))
            .hover(|style| style.bg(cx.theme().accent))
            .role(gpui_kit::Role::Button)
            .aria_label(emoji.clone())
            .on_click(cx.listener({
                let emoji = emoji.clone();
                move |this, _, window, cx| this.insert_panel_emoji(emoji.clone(), window, cx)
            }))
            .child(emoji)
            .into_any_element()
    }

    fn custom_emoji_cell(&self, id: u64, item: StickerItem, cx: &mut Context<Self>) -> AnyElement {
        let still = self.panel_still(&item);
        let premium = self.session().is_some_and(|s| s.my_is_premium());
        let fallback: SharedString = item.emoji.clone().into();
        div()
            .id(("panel-custom-emoji", id))
            .size(px(EMOJI_CELL))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().accent))
            .role(gpui_kit::Role::Button)
            .aria_label(format!("Custom emoji {}", item.emoji))
            .when(!premium, |this| this.opacity(0.55))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.insert_panel_custom_emoji(&item, window, cx)
            }))
            .child(match still {
                Some(path) => img(path)
                    .size(px(CUSTOM_EMOJI_SIZE))
                    .aspect_square()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
                None => div().text_size(px(22.)).child(fallback).into_any_element(),
            })
            .into_any_element()
    }

    fn panel_sticker_cell(
        &mut self,
        id: u64,
        item: StickerItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hovered = self.media_panel.hovered == Some(item.file_id);
        // Only the hovered sticker animates; the rest stay still.
        let animated = hovered
            .then(|| self.sticker_image(item.file_id, item.format, cx))
            .flatten();
        let still = self.panel_still(&item);
        let source = animated
            .map(ImageSource::Render)
            .or_else(|| still.map(ImageSource::from));
        let file_id = item.file_id;
        let favorite = self
            .session()
            .is_some_and(|s| s.stickers.favorites.iter().any(|f| f.file_id == file_id));
        let owner = cx.entity().downgrade();
        let (emoji, width, height) = (item.emoji.clone(), item.width, item.height);
        let thumb = item
            .thumb_file_id
            .filter(|id| id.0 != 0)
            .map(|id| (id, item.thumb_width, item.thumb_height));
        div()
            .id(("panel-sticker", id))
            .size(px(STICKER_CELL))
            .p_1()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .cursor_pointer()
            .hover(|style| style.bg(cx.theme().accent))
            .role(gpui_kit::Role::Button)
            .aria_label(format!("Send {} sticker", item.emoji))
            .on_hover(cx.listener(move |this, hovering: &bool, _, cx| {
                let next = hovering.then_some(file_id);
                if this.media_panel.hovered != next
                    && (*hovering || this.media_panel.hovered == Some(file_id))
                {
                    this.media_panel.hovered = next;
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.send_sticker_pick(file_id, emoji.clone(), width, height, thumb, cx);
                this.close_media_panel(cx);
            }))
            .context_menu(move |menu, _, _| {
                let owner = owner.clone();
                menu.item(
                    PopupMenuItem::new(if favorite {
                        "Remove from favorites"
                    } else {
                        "Add to favorites"
                    })
                    .on_click(move |_, _, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            if let Some(live) = this.live.as_mut() {
                                let _ = live.driver.set_favorite_sticker(file_id, !favorite);
                            }
                            cx.notify();
                        });
                    }),
                )
            })
            .child(match source {
                Some(source) => img(source)
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
                None => div()
                    .size_full()
                    .rounded_md()
                    .bg(cx.theme().muted.opacity(0.5))
                    .into_any_element(),
            })
            .into_any_element()
    }

    fn insert_panel_emoji(
        &mut self,
        emoji: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |input, cx| {
            input.replace(emoji.as_ref(), window, cx);
            input.focus(window, cx);
        });
        let emoji = emoji.to_string();
        self.set_media_pref(
            move |prefs| {
                prefs.recent_emoji.retain(|e| e != &emoji);
                prefs.recent_emoji.insert(0, emoji);
                prefs
                    .recent_emoji
                    .truncate(quill::emoji_catalog::RECENT_LIMIT);
            },
            cx,
        );
    }

    fn insert_panel_custom_emoji(
        &mut self,
        item: &StickerItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.session().is_some_and(|s| s.my_is_premium()) {
            self.status_note = "Custom emoji need Telegram Premium".into();
            cx.notify();
            return;
        }
        let Some(id) = item.custom_emoji_id else {
            return;
        };
        let markup = quill::composer::custom_emoji_markup(&item.emoji, id);
        self.composer.update(cx, |input, cx| {
            input.replace(&markup, window, cx);
            input.focus(window, cx);
        });
        self.set_media_pref(
            move |prefs| {
                prefs.recent_custom_emoji_ids.retain(|e| *e != id);
                prefs.recent_custom_emoji_ids.insert(0, id);
                prefs.recent_custom_emoji_ids.truncate(128);
            },
            cx,
        );
    }

    fn jump_to_panel_section(&mut self, section: usize, cx: &mut Context<Self>) {
        if let Some(first) = self.media_panel.sections.get(section).map(|s| s.first_row) {
            self.media_panel.list.scroll_to(ListOffset {
                item_ix: first,
                offset_in_item: px(0.),
            });
            self.media_panel.active_section = section;
            cx.notify();
        }
    }

    /// The popover itself (absolutely positioned by the caller above the
    /// composer).
    pub(super) fn media_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.sync_media_panel_rows(cx);
        let tab = self.media_panel.tab;
        let tabs = div().flex().items_center().gap_1().children(
            [
                (PanelTab::Emoji, "Emoji"),
                (PanelTab::Stickers, "Stickers"),
                (PanelTab::Gifs, "GIFs"),
            ]
            .into_iter()
            .map(|(value, label)| {
                Button::new(SharedString::from(format!("panel-tab-{label}")))
                    .label(label)
                    .ghost()
                    .small()
                    .selected(tab == value)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_media_panel_tab(value, cx);
                    }))
            }),
        );
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .pt_2()
            .child(tabs)
            .child(
                Button::new("media-panel-close")
                    .icon(gpui_kit::assets::IconName::X)
                    .ghost()
                    .small()
                    .tooltip("Close")
                    .accessibility_label("Close")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_media_panel(cx);
                    })),
            );
        let body: AnyElement = if tab == PanelTab::Gifs {
            div()
                .flex_1()
                .min_h_0()
                .id("media-panel-gifs")
                .overflow_y_scroll()
                .child(self.gif_picker_panel(cx))
                .into_any_element()
        } else {
            let weak = cx.weak_entity();
            let sections = self.media_panel.sections.clone();
            let active = self.media_panel.active_section;
            let search = div().px_2().pt_2().child(
                Textarea::new(if tab == PanelTab::Emoji {
                    &self.emoji_search_input
                } else {
                    &self.sticker_search_input
                })
                .aria_label(if tab == PanelTab::Emoji {
                    "Search emoji"
                } else {
                    "Search stickers"
                })
                .h(px(34.)),
            );
            let empty = self.media_panel.rows.is_empty();
            let list = list(self.media_panel.list.clone(), move |ix, _window, cx| {
                weak.update(cx, |this, cx| this.render_panel_row(ix, cx))
                    .unwrap_or_else(|_| div().into_any_element())
            })
            .flex_1()
            .min_h_0();
            let footer = div()
                .id("media-panel-sections")
                .flex()
                .items_center()
                .gap_1()
                .px_2()
                .py_1()
                .border_t_1()
                .border_color(cx.theme().border)
                .overflow_x_scroll()
                .children(sections.into_iter().enumerate().map(|(index, section)| {
                    let icon: AnyElement = match section.icon {
                        SectionIcon::Emoji(glyph) => {
                            div().text_size(px(18.)).child(glyph).into_any_element()
                        }
                        SectionIcon::Glyph(icon) => {
                            Icon::new(icon).size(px(18.)).into_any_element()
                        }
                        SectionIcon::Set(set_id) => self
                            .panel_item(StickerSource::Set(set_id), 0)
                            .and_then(|item| self.panel_still(item))
                            .map(|path| {
                                img(path)
                                    .size(px(24.))
                                    .aspect_square()
                                    .object_fit(ObjectFit::Contain)
                                    .into_any_element()
                            })
                            .unwrap_or_else(|| {
                                div()
                                    .size(px(22.))
                                    .rounded_md()
                                    .bg(cx.theme().muted)
                                    .into_any_element()
                            }),
                    };
                    div()
                        .id(("panel-section", index as u64))
                        .size(px(32.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_md()
                        .cursor_pointer()
                        .when(index == active, |this| this.bg(cx.theme().accent))
                        .hover(|style| style.bg(cx.theme().accent))
                        .role(gpui_kit::Role::Button)
                        .aria_label(section.label.clone())
                        .tooltip({
                            let label = section.label.clone();
                            move |window, cx| {
                                gpui_kit::component::tooltip::Tooltip::new(label.clone())
                                    .build(window, cx)
                            }
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.jump_to_panel_section(index, cx);
                        }))
                        .child(icon)
                }));
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(search)
                .child(if empty {
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(if tab == PanelTab::Stickers {
                            "No stickers here yet."
                        } else {
                            "No emoji found."
                        })
                        .into_any_element()
                } else {
                    list.into_any_element()
                })
                .child(footer)
                .into_any_element()
        };
        div()
            .id("media-panel")
            .occlude()
            .w(px(PANEL_WIDTH))
            .h(px(PANEL_HEIGHT))
            .flex()
            .flex_col()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().popover)
            .shadow_lg()
            .overflow_hidden()
            .child(header)
            .child(body)
            .into_any_element()
    }
}

fn placeholder_cell(size: f32, cx: &App) -> AnyElement {
    div()
        .size(px(size))
        .p_1()
        .child(
            div()
                .size_full()
                .rounded_md()
                .bg(cx.theme().muted.opacity(0.4)),
        )
        .into_any_element()
}
