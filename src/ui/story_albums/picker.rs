//! The story album picker (tdesktop `EditAlbumBox` over the story grid): a
//! grid of the user's stories with a check on each pick, and a button that
//! counts them.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::scroll_load::scrolled_near_end;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use quill::local_path::sandboxed_display_path;
use quill::story_album_picker::{AlbumPicker, candidate_ids};
use quill::story_cover::time_label;
use quill::telegram::envelope::StoryContentView;
use std::path::PathBuf;

const TILE_WIDTH: f32 = 118.0;
const TILE_HEIGHT: f32 = 168.0;
const CHECK_SIZE: f32 = 22.0;

/// What a tile shows.
struct Tile {
    story_id: i32,
    thumb: Option<PathBuf>,
    duration_label: Option<String>,
    selected: bool,
}

impl QuillApp {
    /// Open the picker on an album (its stories start ticked) or for a new
    /// album, and ask for the thumbnails not yet local.
    pub(super) fn open_album_picker(&mut self, target: AlbumPickerTarget, cx: &mut Context<Self>) {
        let Some(chat_id) = self.stories.page.as_ref().map(|page| page.chat_id) else {
            return;
        };
        let initial: Vec<i32> = match target {
            AlbumPickerTarget::Album(album_id) => self
                .session()
                .and_then(|s| s.stories.album_stories.get(&(chat_id.0, album_id)))
                .cloned()
                .unwrap_or_default(),
            AlbumPickerTarget::NewAlbum => Vec::new(),
        };
        if let Some(page) = self.stories.page.as_mut() {
            page.picker = Some(AlbumPicker::new(target, initial));
        }
        self.request_picker_thumbs(cx);
        self.picker_load_near_end(cx);
        cx.notify();
    }

    pub(super) fn close_album_picker(&mut self, cx: &mut Context<Self>) {
        if let Some(page) = self.stories.page.as_mut() {
            page.picker = None;
        }
        cx.notify();
    }

    pub(super) fn toggle_picker_story(&mut self, story_id: i32, cx: &mut Context<Self>) {
        if let Some(picker) = self
            .stories
            .page
            .as_mut()
            .and_then(|page| page.picker.as_mut())
        {
            picker.toggle(story_id);
        }
        cx.notify();
    }

    /// The stories the picker offers, newest first.
    fn picker_candidates(&self) -> Vec<i32> {
        let (Some(page), Some(session)) = (self.stories.page.as_ref(), self.session()) else {
            return Vec::new();
        };
        let chat = page.chat_id.0;
        let archived = session
            .stories
            .archived_stories
            .get(&chat)
            .map(|state| state.story_ids.as_slice())
            .unwrap_or_default();
        let chat_page = session
            .stories
            .chat_page_stories
            .get(&chat)
            .map(|state| state.story_ids.as_slice())
            .unwrap_or_default();
        let album: &[i32] = match page.picker.as_ref().map(|picker| picker.target) {
            Some(AlbumPickerTarget::Album(album_id)) => session
                .stories
                .album_stories
                .get(&(chat, album_id))
                .map(Vec::as_slice)
                .unwrap_or_default(),
            _ => &[],
        };
        candidate_ids(archived, chat_page, album, |id| {
            session
                .stories
                .stories
                .get(&(chat, id))
                .is_none_or(|story| story.can_be_added_to_album)
        })
    }

    /// The file a story's tile shows: a video's thumbnail, or the photo
    /// size closest to the tile.
    fn picker_thumb_id(&self, story_id: i32) -> Option<quill::ids::FileId> {
        let chat = self.stories.page.as_ref()?.chat_id.0;
        let story = self.session()?.stories.stories.get(&(chat, story_id))?;
        match &story.content {
            StoryContentView::Photo { sizes } => sizes
                .iter()
                .filter(|size| size.width >= TILE_WIDTH as i32 * 2)
                .min_by_key(|size| i64::from(size.width) * i64::from(size.height))
                .or_else(|| {
                    sizes
                        .iter()
                        .max_by_key(|size| i64::from(size.width) * i64::from(size.height))
                })
                .map(|size| size.file_id),
            StoryContentView::Video {
                thumb_file_id,
                file_id,
                ..
            } => thumb_file_id.or(Some(*file_id)),
            _ => None,
        }
    }

    /// More stories exist than are loaded.
    pub(super) fn picker_has_more(&self) -> bool {
        let Some(chat) = self.stories.page.as_ref().map(|page| page.chat_id.0) else {
            return false;
        };
        self.session()
            .and_then(|s| s.stories.archived_stories.get(&chat))
            .is_some_and(|archived| {
                usize::try_from(archived.total_count).is_ok_and(|n| n > archived.story_ids.len())
            })
    }

    /// Load the next page when the grid is scrolled near its end (or does
    /// not fill its box yet), as tdesktop's story grid does.
    pub(super) fn picker_load_near_end(&mut self, cx: &mut Context<Self>) {
        let Some(scroll) = self.stories.page.as_ref().map(|p| p.picker_scroll.clone()) else {
            return;
        };
        if self.archived_loading() || !self.picker_has_more() {
            return;
        }
        if !scrolled_near_end(&scroll) {
            return;
        }
        self.load_more_archived_stories(cx);
        self.request_picker_thumbs(cx);
    }

    /// Download the thumbnails of the candidates that are not local yet.
    pub(super) fn request_picker_thumbs(&mut self, cx: &mut Context<Self>) {
        let roots = self.media_display_roots();
        let wanted: Vec<quill::ids::FileId> = self
            .picker_candidates()
            .into_iter()
            .filter_map(|id| self.picker_thumb_id(id))
            .filter(|file_id| {
                !self.session().is_some_and(|session| {
                    session
                        .media
                        .files
                        .get(&file_id.0)
                        .and_then(|file| file.usable_path())
                        .and_then(|path| sandboxed_display_path(path, &roots))
                        .is_some()
                })
            })
            .collect();
        for file_id in wanted {
            self.request_media_download(file_id, None, cx);
        }
    }

    fn picker_tiles(&self) -> Vec<Tile> {
        let roots = self.media_display_roots();
        let chat = self.stories.page.as_ref().map_or(0, |page| page.chat_id.0);
        self.picker_candidates()
            .into_iter()
            .map(|story_id| {
                let thumb = self.picker_thumb_id(story_id).and_then(|file_id| {
                    self.session()?
                        .media
                        .files
                        .get(&file_id.0)
                        .and_then(|file| file.usable_path())
                        .and_then(|path| sandboxed_display_path(path, &roots))
                });
                let duration_label = self
                    .session()
                    .and_then(|s| s.stories.stories.get(&(chat, story_id)))
                    .and_then(|story| match &story.content {
                        StoryContentView::Video { duration_secs, .. } => {
                            Some(time_label(i64::from(*duration_secs) * 1000))
                        }
                        _ => None,
                    });
                let selected = self
                    .stories
                    .page
                    .as_ref()
                    .and_then(|page| page.picker.as_ref())
                    .is_some_and(|picker| picker.is_selected(story_id));
                Tile {
                    story_id,
                    thumb,
                    duration_label,
                    selected,
                }
            })
            .collect()
    }

    /// The picker, in place of the album list while it is open.
    pub(super) fn album_picker_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(picker) = self.stories.page.as_ref().and_then(|p| p.picker.as_ref()) else {
            return div().into_any_element();
        };
        let count = picker.count();
        let target = picker.target;
        let unchanged = picker.unchanged();
        let only_adds = picker.only_adds();
        let name_empty = self
            .stories
            .page
            .as_ref()
            .is_some_and(|page| page.new_album_name.read(cx).value().trim().is_empty());
        let label = match target {
            AlbumPickerTarget::NewAlbum if count > 0 => trn_with(
                keys::Q_MEDIA_ALBUM_CREATE_STORIES_COUNT,
                count,
                &[("count", &count.to_string())],
            ),
            AlbumPickerTarget::NewAlbum => tr(keys::Q_MEDIA_CREATE_ALBUM).to_string(),
            AlbumPickerTarget::Album(_) if only_adds => trn_with(
                keys::Q_MEDIA_ALBUM_ADD_STORIES_COUNT,
                picker.added().len(),
                &[("count", &picker.added().len().to_string())],
            ),
            AlbumPickerTarget::Album(_) if count > 0 => trn_with(
                keys::Q_MEDIA_ALBUM_SAVE_STORIES_COUNT,
                count,
                &[("count", &count.to_string())],
            ),
            AlbumPickerTarget::Album(_) => tr(keys::SETTINGS_SAVE).to_string(),
        };
        let disabled = match target {
            AlbumPickerTarget::Album(_) => unchanged,
            AlbumPickerTarget::NewAlbum => name_empty,
        };
        let tiles = self.picker_tiles();
        let scroll = self
            .stories
            .page
            .as_ref()
            .map(|page| page.picker_scroll.clone())
            .unwrap_or_default();
        let loading = self.archived_loading();
        let mut grid = div()
            .id("story-album-picker-grid")
            .flex()
            .flex_wrap()
            .gap_2()
            .max_h(px(380.))
            .overflow_y_scroll()
            .track_scroll(&scroll)
            .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                this.spawn_scoped(cx, async move |this, cx| {
                    let _ = this.update(cx, |this, cx| this.picker_load_near_end(cx));
                })
                .detach();
            }));
        if tiles.is_empty() {
            grid = grid.child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(tr(keys::Q_MEDIA_ALBUM_NO_STORIES_TO_ADD)),
            );
        }
        for tile in tiles {
            grid = grid.child(self.picker_tile(tile, cx));
        }
        if loading {
            grid = grid.child(
                div()
                    .id("story-album-picker-loading")
                    .w_full()
                    .flex()
                    .justify_center()
                    .py_2()
                    .child(Spinner::new().small()),
            );
        }
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("story-album-picker-back")
                            .icon(gpui_kit::assets::IconName::ArrowLeft)
                            .tooltip(tr(keys::Q_MEDIA_ALBUMS))
                            .accessibility_label(tr(keys::Q_MEDIA_ALBUMS))
                            .ghost()
                            .text_color(cx.theme().foreground)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_album_picker(cx);
                            })),
                    )
                    .child(
                        div()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child(tr(keys::Q_MEDIA_ALBUM_ADD_STORIES_TITLE)),
                    ),
            )
            .child(grid)
            .child(
                div().flex().justify_end().child(
                    Button::new("story-album-picker-confirm")
                        .primary()
                        .label(label)
                        .disabled(disabled)
                        .on_click(cx.listener(move |this, _, _, cx| match target {
                            AlbumPickerTarget::Album(_) => this.save_album_picker(cx),
                            AlbumPickerTarget::NewAlbum => this.create_story_album(cx),
                        })),
                ),
            )
            .into_any_element()
    }

    fn picker_tile(&self, tile: Tile, cx: &mut Context<Self>) -> AnyElement {
        let story_id = tile.story_id;
        let selected = tile.selected;
        let name = tr_with(
            keys::Q_MEDIA_ALBUM_STORY_TILE,
            &[("story_id", &story_id.to_string())],
        );
        let badge = div()
            .absolute()
            .top(px(6.))
            .right(px(6.))
            .size(px(CHECK_SIZE))
            .rounded_full()
            .flex()
            .items_center()
            .justify_center()
            .border_2()
            .border_color(gpui_kit::white())
            .when(selected, |this| this.bg(cx.theme().primary))
            .when(!selected, |this| this.bg(gpui_kit::black().opacity(0.25)))
            .when(selected, |this| {
                this.child(
                    Icon::new(gpui_kit::assets::IconName::Check)
                        .size_3()
                        .text_color(cx.theme().primary_foreground),
                )
            });
        div()
            .id(("story-album-picker-tile", story_id as u64))
            .relative()
            .w(px(TILE_WIDTH))
            .h(px(TILE_HEIGHT))
            .rounded_md()
            .overflow_hidden()
            .bg(cx.theme().secondary)
            .cursor_pointer()
            .border_2()
            .border_color(if selected {
                cx.theme().primary
            } else {
                gpui_kit::transparent_black()
            })
            .aria_label(name.clone())
            .tooltip({
                let tip = name.clone();
                move |window, cx| Tooltip::new(tip.clone()).build(window, cx)
            })
            .when_some(tile.thumb, |this, path| {
                this.child(img(path).size_full().object_fit(ObjectFit::Cover))
            })
            .child(badge)
            .when_some(tile.duration_label, |this, label| {
                this.child(
                    div()
                        .absolute()
                        .left(px(6.))
                        .bottom(px(6.))
                        .px_1()
                        .rounded_sm()
                        .bg(gpui_kit::black().opacity(0.55))
                        .text_xs()
                        .text_color(gpui_kit::white())
                        .child(label),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_picker_story(story_id, cx);
            }))
            .into_any_element()
    }
}
