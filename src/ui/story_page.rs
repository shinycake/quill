//! Phase 9.7 story-page UI state: the `StoryPage` overlay struct and the
//! `ReadyStoryAlbums` screenshot-demo seeder. Split out of `ui/mod.rs` per
//! the file-size directive — `ui/mod.rs` keeps the `QuillApp` field, the
//! `QuillApp` methods, and the render wiring.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use gpui_kit::AppContext;
use gpui_kit::component::input::TextareaState;
use gpui_kit::gpui::{Context, Entity, ScrollHandle, Window};
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::story_album_picker::AlbumPicker;
/// Phase 9.7: the chat story page overlay — story albums (list, open,
/// create, rename, delete, add/remove/reorder stories, reorder albums),
/// chat-page stories (pin/unpin), and the paginated archive list for one
/// chat. All mutations round-trip through TDLib; the honest
/// Checking/Sending/Succeeded/Failed states live in
/// `Session::story_page_op` and render as the page's status line.
pub(crate) struct StoryPage {
    pub(crate) chat_id: ChatId,
    /// Opened album id, or `None` for the album list.
    pub(crate) open_album: Option<i32>,
    /// "New album" form inputs.
    pub(crate) new_album_name: Entity<TextareaState>,
    /// Rename input on the opened album.
    pub(crate) rename_input: Entity<TextareaState>,
    /// The story picker (add to an album, or choose a new album's
    /// stories), while it is open.
    pub(crate) picker: Option<AlbumPicker>,
    /// Scroll of the picker's grid: the next page loads near its end.
    pub(crate) picker_scroll: ScrollHandle,
    /// Scroll of the posted-stories and archive sections: the next page
    /// loads near their end (`story_albums/paging.rs`).
    pub(crate) posted_scroll: ScrollHandle,
    pub(crate) archive_scroll: ScrollHandle,
    pub(crate) posted_asked: crate::ui::scroll_load::Asked,
    pub(crate) archive_asked: crate::ui::scroll_load::Asked,
    /// Album id awaiting the second delete click (two-click confirm).
    pub(crate) delete_confirm: Option<i32>,
}

impl StoryPage {
    pub(crate) fn new(chat_id: ChatId, window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let mut input = |placeholder: &str| {
            cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder(placeholder)
                    .auto_grow(1, 1)
                    .submit_on_enter(false)
            })
        };
        Self {
            chat_id,
            open_album: None,
            new_album_name: input(tr(keys::Q_MEDIA_ALBUM_NAME_1_12_CHARACTERS)),
            rename_input: input(tr(keys::Q_MEDIA_ALBUM_NAME_1_12_CHARACTERS)),
            picker: None,
            picker_scroll: ScrollHandle::new(),
            posted_scroll: ScrollHandle::new(),
            archive_scroll: ScrollHandle::new(),
            posted_asked: Default::default(),
            archive_asked: Default::default(),
            delete_confirm: None,
        }
    }
}
