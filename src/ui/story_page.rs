//! Phase 9.7 story-page UI state: the `StoryPage` overlay struct and the
//! `ReadyStoryAlbums` screenshot-demo seeder. Split out of `ui/mod.rs` per
//! the file-size directive — `ui/mod.rs` keeps the `QuillApp` field, the
//! `QuillApp` methods, and the render wiring.
#![deny(clippy::indexing_slicing)]

use super::QuillApp;
use gpui_kit::AppContext;
use gpui_kit::component::input::TextareaState;
use gpui_kit::gpui::{Context, Entity, Window};
use quill::ids::ChatId;
use quill::lang::{keys, tr};
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
    pub(crate) new_album_story_ids: Entity<TextareaState>,
    /// Rename + add-stories inputs on the opened album.
    pub(crate) rename_input: Entity<TextareaState>,
    pub(crate) add_story_ids: Entity<TextareaState>,
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
            new_album_story_ids: input(tr(keys::Q_MEDIA_STORY_IDS_COMMA_SEPARATED_OPTIONAL)),
            rename_input: input(tr(keys::Q_MEDIA_ALBUM_NAME_1_12_CHARACTERS)),
            add_story_ids: input(tr(keys::Q_MEDIA_STORY_IDS_TO_ADD_COMMA)),
            delete_confirm: None,
        }
    }
}
