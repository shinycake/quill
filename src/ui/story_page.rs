//! Phase 9.7 story-page UI state: the `StoryPage` overlay struct and the
//! `ReadyStoryAlbums` screenshot-demo seeder. Split out of `ui/mod.rs` per
//! the file-size directive — `ui/mod.rs` keeps the `QuillApp` field, the
//! `QuillApp` methods, and the render wiring.

use super::QuillApp;
use gpui_kit::AppContext;
use gpui_kit::component::input::TextareaState;
use gpui_kit::gpui::{Context, Entity, Window};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::state::{RequestPurpose, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

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
            new_album_name: input("Album name (1–12 characters)"),
            new_album_story_ids: input("Story ids, comma-separated (optional)"),
            rename_input: input("Album name (1–12 characters)"),
            add_story_ids: input("Story ids to add, comma-separated"),
            delete_confirm: None,
        }
    }
}

/// Phase 9.7: `ReadyStoryAlbums` fixture — the `ReadyStories` seed plus
/// story albums, chat-page stories (one pinned) and archive stories for
/// chat 11, injected through the same reducers the live paths use
/// (`@extra`-correlated like `apply_ready_sponsored`).
pub(crate) fn apply_ready_story_albums(
    session: &mut Session,
    sink: &Arc<MemorySink>,
    seq: &AtomicU64,
) {
    let dyn_sink: Arc<dyn DiagnosticSink> = sink.clone();
    super::apply_ready_stories(session, sink, seq);
    let story = |id: i32, caption: &str| {
        format!(
            r#"{{"@type":"story","id":{id},"poster_chat_id":11,"date":1700000000,"caption":{{"@type":"formattedText","text":"{caption}","entities":[]}}}}"#
        )
    };
    // `getChatStoryAlbums` answer — two albums.
    let extra_albums = session.request(RequestPurpose::GetChatStoryAlbums, Some(ChatId(11)));
    let albums = format!(
        r#"{{"@type":"storyAlbums","@extra":"{}","albums":[{{"@type":"storyAlbum","id":1,"name":"Travel"}},{{"@type":"storyAlbum","id":2,"name":"Food"}}]}}"#,
        extra_albums.0
    );
    // `getChatPostedToChatPageStories` first page — story 301 pinned.
    let extra_page = session.request(
        RequestPurpose::GetChatPostedToChatPageStories,
        Some(ChatId(11)),
    );
    let chat_page = format!(
        r#"{{"@type":"stories","@extra":"{}","total_count":2,"pinned_story_ids":[301],"stories":[{},{}]}}"#,
        extra_page.0,
        story(301, "Venice at dusk"),
        story(302, "Pasta night")
    );
    // `getChatArchivedStories` first page.
    let extra_archive = session.request(RequestPurpose::GetChatArchivedStories, Some(ChatId(11)));
    let archive = format!(
        r#"{{"@type":"stories","@extra":"{}","total_count":2,"stories":[{},{}]}}"#,
        extra_archive.0,
        story(201, "Old road trip"),
        story(202, "Winter market")
    );
    // `getStoryAlbumStories` for album 1 (Travel).
    let extra_album_stories = session.request_for_story_album(
        RequestPurpose::GetStoryAlbumStories,
        ChatId(11),
        None,
        Some(1),
    );
    let album_stories = format!(
        r#"{{"@type":"stories","@extra":"{}","total_count":2,"stories":[{},{}]}}"#,
        extra_album_stories.0,
        story(301, "Venice at dusk"),
        story(303, "Mountain pass")
    );
    for json in [albums, chat_page, archive, album_stories] {
        if let Some(owned) = copy_and_parse(&json, seq, &dyn_sink) {
            session.apply(owned);
        }
    }
}
