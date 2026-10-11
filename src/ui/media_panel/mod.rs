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
use quill::ids::{ChatId, FileId, MessageId};
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
    reaction: bool,
    query: String,
    session_revision: u64,
    recent_emoji: usize,
    premium: bool,
}

/// The message the panel is choosing a reaction for (Telegram Desktop's
/// expanded reaction selector), and where its menu was.
#[derive(Clone, Copy, PartialEq)]
pub(super) struct ReactionTarget {
    pub chat_id: ChatId,
    pub message_id: MessageId,
    pub position: Point<Pixels>,
}

pub(super) struct MediaPanel {
    pub open: bool,
    /// Set while the panel picks a reaction instead of composing.
    pub reaction: Option<ReactionTarget>,
    pub tab: PanelTab,
    pub list: ListState,
    pub rows: Vec<PanelRow>,
    pub sections: Vec<PanelSection>,
    pub key: Option<PanelKey>,
    pub active_section: usize,
    pub hovered: Option<FileId>,
    /// The sticker search query last sent to TDLib.
    pub searched: String,
    /// The emoji query `getKeywordEmojis` last went out for.
    pub keyword_searched: String,
}

impl Default for MediaPanel {
    fn default() -> Self {
        Self {
            open: false,
            reaction: None,
            tab: PanelTab::default(),
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            rows: Vec::new(),
            sections: Vec::new(),
            key: None,
            active_section: 0,
            hovered: None,
            searched: String::new(),
            keyword_searched: String::new(),
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

/// One section per installed custom emoji pack. With `matches` (a search),
/// only emoji whose associated emoji matched, and only loaded packs.
fn push_custom_packs(
    rows: &mut Vec<PanelRow>,
    sections: &mut Vec<PanelSection>,
    session: &quill::state::Session,
    matches: Option<&std::collections::HashSet<String>>,
) {
    for set in &session.stickers.emoji.installed_sets {
        let title: SharedString = if set.title.is_empty() {
            set.name.clone().into()
        } else {
            set.title.clone().into()
        };
        let items = session.media.media_library.set_stickers.get(&set.id);
        let cells: Vec<PanelCell> = match (items, matches) {
            (Some(items), matches) => items
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    matches.is_none_or(|set| set.contains(&item.emoji.replace('\u{fe0f}', "")))
                })
                .map(|(ix, _)| PanelCell::Custom {
                    source: StickerSource::Set(set.id),
                    ix,
                })
                .collect(),
            (None, Some(_)) => Vec::new(),
            (None, None) => (0..set.size.max(1) as usize)
                .map(|_| PanelCell::Placeholder { set_id: set.id })
                .collect(),
        };
        if cells.is_empty() {
            continue;
        }
        sections.push(PanelSection {
            label: title.clone(),
            icon: SectionIcon::Set(set.id),
            first_row: rows.len(),
        });
        rows.push(PanelRow::Header {
            label: title,
            set_id: Some(set.id),
        });
        push_grid(rows, cells, EMOJI_COLS, false);
    }
}

impl QuillApp {
    pub(super) fn media_panel_open(&self) -> bool {
        self.pickers.media_panel.open
    }

    pub(super) fn toggle_media_panel(&mut self, tab: PanelTab, cx: &mut Context<Self>) {
        if self.pickers.media_panel.open && self.pickers.media_panel.tab == tab {
            self.close_media_panel(cx);
        } else {
            self.open_media_panel(tab, cx);
        }
    }

    pub(super) fn open_media_panel(&mut self, tab: PanelTab, cx: &mut Context<Self>) {
        if self.recording_active() {
            self.cancel_recording(cx);
        }
        self.pickers.media_panel.open = true;
        self.set_media_panel_tab(tab, cx);
        if let Some(live) = self.live.as_mut() {
            let _ = live.driver.open_media_panel();
            if tab == PanelTab::Gifs {
                let _ = live.driver.open_gif_panel();
            }
        } else if tab == PanelTab::Gifs
            && let Some(session) = self.demo_session.as_mut()
        {
            session.stickers.gifs.open = true;
        }
        cx.notify();
    }

    /// Telegram Desktop's expanded reaction selector: the emoji panel in
    /// reaction mode, where the message menu was.
    pub(super) fn open_reaction_selector(
        &mut self,
        chat_id: ChatId,
        message_id: MessageId,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.message_ui.menu = None;
        self.pickers.reaction_search_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
            input.focus(window, cx);
        });
        self.pickers.media_panel.reaction = Some(ReactionTarget {
            chat_id,
            message_id,
            position,
        });
        self.pickers.media_panel.key = None;
        self.pickers.media_panel.active_section = 0;
        self.pickers.media_panel.open = true;
        self.pickers.media_panel.tab = PanelTab::Emoji;
        if let Some(live) = self.live.as_mut() {
            // Loads the installed custom emoji packs.
            let _ = live.driver.open_media_panel();
        }
        cx.notify();
    }

    /// In reaction mode a cell reacts (and closes) instead of inserting.
    fn react_from_panel(
        &mut self,
        choice: quill::state::ReactionChoice,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(target) = self.pickers.media_panel.reaction else {
            return false;
        };
        self.close_media_panel(cx);
        self.toggle_reaction(target.chat_id, target.message_id, choice, cx);
        true
    }

    pub(super) fn close_media_panel(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.pickers.media_panel.open {
            return false;
        }
        self.pickers.media_panel.open = false;
        self.pickers.media_panel.hovered = None;
        if self.pickers.media_panel.reaction.take().is_some() {
            self.pickers.media_panel.key = None;
        }
        if let Some(live) = self.live.as_mut() {
            live.driver.close_gif_panel();
        } else if let Some(session) = self.demo_session.as_mut() {
            session.stickers.gifs.close();
        }
        cx.notify();
        true
    }

    fn set_media_panel_tab(&mut self, tab: PanelTab, cx: &mut Context<Self>) {
        if self.pickers.media_panel.tab != tab {
            self.pickers.media_panel.tab = tab;
            self.pickers.media_panel.key = None;
            self.pickers.media_panel.active_section = 0;
            if tab == PanelTab::Gifs {
                if let Some(live) = self.live.as_mut() {
                    let _ = live.driver.open_gif_panel();
                } else if let Some(session) = self.demo_session.as_mut() {
                    session.stickers.gifs.open = true;
                }
            }
        }
        cx.notify();
    }

    /// The item a sticker / custom-emoji cell refers to.
    pub(super) fn panel_item(&self, source: StickerSource, ix: usize) -> Option<&StickerItem> {
        let session = self.session()?;
        match source {
            StickerSource::Set(id) => session.media.media_library.set_stickers.get(&id)?.get(ix),
            StickerSource::Recent => session.stickers.stickers.recent.get(ix),
            StickerSource::Favorites => session.stickers.stickers.favorites.get(ix),
            StickerSource::Found => session.stickers.stickers.found_stickers.get(ix),
            StickerSource::CustomEmoji => session.stickers.emoji.custom_emoji_stickers.get(ix),
        }
    }

    fn panel_query(&self, cx: &App) -> String {
        match self.pickers.media_panel.tab {
            PanelTab::Emoji if self.pickers.media_panel.reaction.is_some() => self
                .pickers
                .reaction_search_input
                .read(cx)
                .value()
                .trim()
                .to_string(),
            PanelTab::Emoji => self
                .pickers
                .emoji_search_input
                .read(cx)
                .value()
                .trim()
                .to_string(),
            PanelTab::Stickers => self
                .pickers
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
            tab: self.pickers.media_panel.tab,
            reaction: self.pickers.media_panel.reaction.is_some(),
            query: query.clone(),
            session_revision: self.session().map_or(0, |s| s.revision),
            recent_emoji: self
                .session()
                .map_or(0, |s| s.settings.media_prefs.recent_emoji.len()),
            premium: self.session().is_some_and(|s| s.my_is_premium()),
        };
        if self.pickers.media_panel.key.as_ref() == Some(&key) {
            return;
        }
        let same_view = self.pickers.media_panel.key.as_ref().is_some_and(|old| {
            old.tab == key.tab && old.query == key.query && old.reaction == key.reaction
        });
        let (rows, sections) = match key.tab {
            PanelTab::Emoji if key.reaction => self.build_reaction_rows(&query),
            PanelTab::Emoji => self.build_emoji_rows(&query),
            PanelTab::Stickers => self.build_sticker_rows(&query),
            PanelTab::Gifs => (Vec::new(), Vec::new()),
        };
        let top = self.pickers.media_panel.list.logical_scroll_top();
        self.pickers.media_panel.list.reset(rows.len());
        if same_view && top.item_ix < rows.len() {
            self.pickers.media_panel.list.scroll_to(top);
        }
        self.pickers.media_panel.rows = rows;
        self.pickers.media_panel.sections = sections;
        self.pickers.media_panel.key = Some(key);
        // Emoji search adds TDLib's keyword matches (all typed languages).
        if self.pickers.media_panel.tab == PanelTab::Emoji
            && query != self.pickers.media_panel.keyword_searched
        {
            self.pickers.media_panel.keyword_searched = query.clone();
            if let Some(live) = self.live.as_mut() {
                let _ = live.driver.search_keyword_emojis(&query);
            }
        }
        // Sticker search goes to TDLib (by emoji or keyword).
        if self.pickers.media_panel.tab == PanelTab::Stickers
            && query != self.pickers.media_panel.searched
        {
            self.pickers.media_panel.searched = query.clone();
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
            // Catalog matches first, then `getKeywordEmojis` matches the
            // catalog's names missed (other languages, synonyms).
            let mut cells: Vec<PanelCell> = quill::emoji_catalog::search(None, query)
                .filter(|entry| !has_skin_tone(entry.emoji))
                .map(|entry| PanelCell::Emoji(SharedString::new_static(entry.emoji)))
                .collect();
            if let Some(session) = self.session() {
                let mut seen: std::collections::HashSet<String> =
                    quill::emoji_catalog::search(None, query)
                        .map(|entry| entry.emoji.replace('\u{fe0f}', ""))
                        .collect();
                for emoji in &session.stickers.emoji.keyword_emojis {
                    if !has_skin_tone(emoji) && seen.insert(emoji.replace('\u{fe0f}', "")) {
                        cells.push(PanelCell::Emoji(SharedString::from(emoji.clone())));
                    }
                }
            }
            push_grid(&mut rows, cells, EMOJI_COLS, false);
            return (rows, sections);
        }
        let session = self.session();
        // Recent: plain emoji, then recently used custom emoji.
        let recent: Vec<PanelCell> = session
            .map(|s| {
                let custom = s
                    .settings
                    .media_prefs
                    .recent_custom_emoji_ids
                    .iter()
                    .filter_map(|id| {
                        s.stickers
                            .emoji
                            .custom_emoji_stickers
                            .iter()
                            .position(|item| item.custom_emoji_id == Some(*id))
                            .map(|ix| PanelCell::Custom {
                                source: StickerSource::CustomEmoji,
                                ix,
                            })
                    });
                s.settings
                    .media_prefs
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
            push_custom_packs(&mut rows, &mut sections, session, None);
        }
        (rows, sections)
    }

    /// Telegram Desktop's full reaction selector: every reaction the
    /// message allows, then (when the chat allows custom emoji) the
    /// installed custom emoji packs. The search filters both.
    fn build_reaction_rows(&self, query: &str) -> (Vec<PanelRow>, Vec<PanelSection>) {
        use quill::state::ReactionChoice;
        let mut rows = Vec::new();
        let mut sections = Vec::new();
        let (Some(session), Some(target)) = (self.session(), self.pickers.media_panel.reaction)
        else {
            return (rows, sections);
        };
        let Some(options) = session
            .stickers
            .message_reaction_options
            .as_ref()
            .filter(|o| o.chat_id == target.chat_id && o.message_id == target.message_id)
        else {
            return (rows, sections);
        };
        // Emoji matching the search (variation selectors ignored).
        let strip = |e: &str| e.replace('\u{fe0f}', "");
        let matches: Option<std::collections::HashSet<String>> = (!query.is_empty()).then(|| {
            quill::emoji_catalog::search(None, query)
                .map(|entry| strip(entry.emoji))
                .chain(
                    session
                        .stickers
                        .emoji
                        .keyword_emojis
                        .iter()
                        .map(|e| strip(e)),
                )
                .collect()
        });
        let wanted = |emoji: &str| {
            matches
                .as_ref()
                .is_none_or(|set| set.contains(&strip(emoji)))
        };
        let cells: Vec<PanelCell> = options
            .all()
            .into_iter()
            .filter_map(|choice| match choice {
                ReactionChoice::Emoji(emoji) => wanted(&emoji)
                    .then(|| PanelCell::Emoji(super::reactions::emoji_presentation(&emoji).into())),
                ReactionChoice::CustomEmoji(id) => {
                    let ix = session
                        .stickers
                        .emoji
                        .custom_emoji_stickers
                        .iter()
                        .position(|item| item.custom_emoji_id == Some(id))?;
                    wanted(&session.stickers.emoji.custom_emoji_stickers[ix].emoji).then_some(
                        PanelCell::Custom {
                            source: StickerSource::CustomEmoji,
                            ix,
                        },
                    )
                }
            })
            .collect();
        if !cells.is_empty() {
            sections.push(PanelSection {
                label: "Reactions".into(),
                icon: SectionIcon::Glyph(gpui_kit::assets::IconName::Heart),
                first_row: rows.len(),
            });
            push_grid(&mut rows, cells, EMOJI_COLS, false);
        }
        if options.allow_custom_emoji {
            push_custom_packs(&mut rows, &mut sections, session, matches.as_ref());
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
                (0..session.stickers.stickers.found_stickers.len()).map(|ix| PanelCell::Sticker {
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
                session.stickers.stickers.favorites.len(),
            ),
            (
                StickerSource::Recent,
                "Recently used",
                gpui_kit::assets::IconName::Clock,
                session.stickers.stickers.recent.len(),
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
        for set in &session.stickers.stickers.sets {
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
            match session.media.media_library.set_stickers.get(&set.id) {
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
            .filter_map(|id| session.media.files.get(&id.0)?.usable_path())
            .find_map(|path| sandboxed_display_path(path, &roots))
    }

    /// One virtualized row; also asks for what it needs to show (set
    /// contents, cell files).
    pub(super) fn render_panel_row(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.pickers.media_panel.rows.get(ix).cloned() else {
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
                                let on_screen = row_on_screen(
                                    self.pickers.media_panel.list.item_is_above_viewport(ix),
                                    self.pickers.media_panel.list.item_is_below_viewport(ix),
                                );
                                self.custom_emoji_cell(id, item, on_screen, cx)
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
                                self.panel_sticker_cell(
                                    id,
                                    item,
                                    matches!(source, StickerSource::Recent),
                                    cx,
                                )
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
        // A recently used emoji offers "Reset recent emoji" (the list is
        // local; tdesktop clears it from the same section).
        let in_recent = self.pickers.media_panel.reaction.is_none()
            && self.session().is_some_and(|s| {
                s.settings
                    .media_prefs
                    .recent_emoji
                    .iter()
                    .any(|e| *e == *emoji)
            });
        let owner = cx.entity().downgrade();
        let this_emoji = emoji.to_string();
        let cell = div()
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
            .child(emoji);
        if !in_recent {
            return cell.into_any_element();
        }
        cell.context_menu(move |menu, _, _| {
            let reset_owner = owner.clone();
            let remove_owner = owner.clone();
            let emoji = this_emoji.clone();
            menu.item(
                PopupMenuItem::new("Remove from recent").on_click(move |_, _, cx| {
                    let emoji = emoji.clone();
                    let _ = remove_owner.update(cx, |this, cx| {
                        this.set_media_pref(
                            move |prefs| prefs.recent_emoji.retain(|e| *e != emoji),
                            cx,
                        );
                        this.pickers.media_panel.key = None;
                        cx.notify();
                    });
                }),
            )
            .item(
                PopupMenuItem::new("Reset recent emoji").on_click(move |_, _, cx| {
                    let _ = reset_owner.update(cx, |this, cx| this.reset_recent_emoji(cx));
                }),
            )
        })
        .into_any_element()
    }
}

/// Whether a list row touches the viewport; unknown (not laid out yet)
/// counts as visible so the first frame still animates.
fn row_on_screen(above: Option<bool>, below: Option<bool>) -> bool {
    above != Some(true) && below != Some(true)
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

mod custom_emoji_cell;

#[cfg(test)]
mod tests {
    use super::row_on_screen;

    #[test]
    fn overdraw_rows_do_not_count_as_on_screen() {
        assert!(!row_on_screen(Some(true), Some(false)));
        assert!(!row_on_screen(Some(false), Some(true)));
        assert!(row_on_screen(Some(false), Some(false)));
        // Not laid out yet: animate rather than flash a still.
        assert!(row_on_screen(None, None));
    }
}
