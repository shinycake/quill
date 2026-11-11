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
/// `Session::story_page_ops` and render as the page's status line.
pub(crate) struct StoryPage {
    pub(crate) chat_id: ChatId,
    /// The album list, or one opened album and where its delete stands.
    pub(crate) view: AlbumView,
    /// The mutation the page waits on and what its answer does.
    pub(crate) wait: PageWait,
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
}

/// What the story page shows below its header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlbumView {
    List,
    Album { id: i32, delete: AlbumDelete },
}

/// The two-click album delete: the first click arms it, the second sends
/// it. Part of the album view, so leaving the view (back, the picker, an
/// answer) disarms it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlbumDelete {
    Idle,
    Armed,
}

impl AlbumView {
    pub(crate) fn album(id: i32) -> Self {
        Self::Album {
            id,
            delete: AlbumDelete::Idle,
        }
    }

    /// The opened album, if any.
    pub(crate) fn open_album(self) -> Option<i32> {
        match self {
            Self::List => None,
            Self::Album { id, .. } => Some(id),
        }
    }

    /// Whether the delete of album `album_id` waits for its second click.
    pub(crate) fn delete_armed(self, album_id: i32) -> bool {
        matches!(
            self,
            Self::Album { id, delete: AlbumDelete::Armed } if id == album_id
        )
    }

    /// Something else was pressed: a delete armed before does not carry on.
    pub(crate) fn disarm(&mut self) {
        if let Self::Album { delete, .. } = self {
            *delete = AlbumDelete::Idle;
        }
    }
}

/// The one mutation of the page still on its way. It ends with its answer:
/// `Succeeded` runs the follow-up, a failure or a lost call leaves the page
/// as it was, with the failure on the status line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PageWait {
    Idle,
    /// `createStoryAlbum`: the name and the picker go once it is made.
    Creating(quill::ids::RequestId),
    /// The picker's adds and removes on an opened album.
    SavingPicker(Vec<quill::ids::RequestId>),
    /// `deleteStoryAlbum`: the album view goes once it is deleted.
    Deleting(quill::ids::RequestId),
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
            view: AlbumView::List,
            wait: PageWait::Idle,
            new_album_name: input(tr(keys::Q_MEDIA_ALBUM_NAME_1_12_CHARACTERS)),
            rename_input: input(tr(keys::Q_MEDIA_ALBUM_NAME_1_12_CHARACTERS)),
            picker: None,
            picker_scroll: ScrollHandle::new(),
            posted_scroll: ScrollHandle::new(),
            archive_scroll: ScrollHandle::new(),
            posted_asked: Default::default(),
            archive_asked: Default::default(),
        }
    }
}
