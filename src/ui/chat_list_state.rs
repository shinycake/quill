//! Chat list: rows, filter, menus, pinning, preview and side tabs.
#![deny(clippy::indexing_slicing)]

use super::app::ChatListFilter;
use super::chat_row::ChatListItem;
use super::chat_row::ChatPreviewState;
use super::*;
use gpui_kit::component::*;
use gpui_kit::*;
use quill::ids::ChatId;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Instant;

pub(crate) struct ChatListUi {
    /// kit Phase 3: chat-list virtualization — scroll handle owned by the
    /// app so scroll position survives re-renders (scroll restoration).
    pub(super) scroll: VirtualListScrollHandle,
    /// kit Phase 3: chat-list virtualization — the flat item list the
    /// `VirtualList` renders (main rows + archive section).
    pub(super) items: Vec<ChatListItem>,
    /// Folder tag chip names the rows of `items` draw, built with them.
    pub(super) folder_tags: Rc<super::chat_row::FolderTagContext>,
    /// Chat-row swipe gesture state (`quill::chat_swipe`).
    pub(super) swipe: super::chat_swipe_ui::ChatSwipeState,
    /// Slice CL1: right-click chat-row context menu target + window
    /// position.
    pub(super) menu: Option<ChatMenuState>,
    /// Right-click menu of the "Archived chats" row (window position).
    pub(super) archive_menu: Option<Point<Pixels>>,
    /// Contacts tab, stories menu, suggestions and search tabs.
    pub(super) global: super::chatlist_global::ChatlistGlobal,
    /// Pinned-chat drag in progress (or its release slide), see
    /// `quill::pin_reorder`; `pin_reorder_archived` says which pinned list.
    pub(super) pin_reorder: Option<quill::pin_reorder::PinReorder>,
    pub(super) pin_reorder_archived: bool,
    /// Where the last press on a pinned row went down, so the dragged row
    /// starts under the pointer instead of where the drag threshold fired.
    pub(super) pin_press: Option<(i64, f32)>,
    /// Slice CL: the open peek preview — hovered/press-and-hold chat,
    /// or `None`. Transient; never an open chat.
    pub(super) preview: Option<ChatPreviewState>,
    /// Slice CL: an in-progress long press on a chat-list row — the
    /// row's chat id + press start, for the peek preview.
    pub(super) preview_press: Option<(ChatId, Instant)>,
    /// The message the next chat open lands on ("Go To Message" from the
    /// peek preview); taken by `select_chat_with`.
    pub(super) open_at: Option<quill::ids::MessageId>,
    /// Slice CL3: multi-select mode — checked chat ids. Non-empty while
    /// selecting; rows toggle the check instead of opening the chat and
    /// the select bar offers the bulk actions.
    pub(super) selected: HashSet<i64>,
    /// Slice CL2: chat-list category filter (TGX `ChatFilter` unread /
    /// archive categories, `MainController` pager categories). `All` is
    /// the unfiltered list; `Unread` filters to unread chats;
    /// `Archived` shows only the archive.
    pub(super) filter: ChatListFilter,
    /// The Archive menu's "How does it work?" box is open.
    pub(super) archive_hint_open: bool,
    /// The main menu's My Groups (`false`) or My Channels (`true`) list is open.
    pub(super) my_chats_open: Option<bool>,
    /// Phase 6: sidebar tab — `true` shows the contacts list instead of
    /// the chat list.
    pub(super) contacts_tab_open: bool,
    /// Phase C2i: sidebar tab — `true` shows the recent-calls list +
    /// call settings instead of the chat list. Mutually exclusive with
    /// `contacts_tab_open`.
    pub(super) calls_tab_open: bool,
    /// On a narrow window the forum's topic column replaces the chat list;
    /// this brings the list back until another forum opens.
    pub(super) forum_chats_peek: bool,
    /// The forum topic column is on screen this frame, so the conversation
    /// shows a hint instead of repeating the topic list.
    pub(super) forum_column_shown: bool,
    /// Phase B1: pending "Close secret chat" confirm for the open chat
    /// (`closeSecretChat`, schema 1.8.67 line 15242).
    pub(super) pending_close_secret_chat: Option<ChatId>,
    /// Phase 9.1: the story the user opened while its full content was
    /// still being fetched; resolved on the next render once the `story`
    /// response lands in the cache.
    pub(super) pending_story_open: Option<super::story_viewer::PendingStoryOpen>,
    /// A file drag over the rows.
    pub(super) drop_on_row: super::chat_list::DropOnRow,
    /// A drop on a forum row waiting for its topic (`DialogKind::DropTopic`).
    pub(super) drop_topic: Option<super::chat_list::DropTopic>,
    /// The keyboard selection in the list and in its search results.
    pub(super) keys: super::chat_list_keys::ChatListKeys,
}

impl ChatListUi {
    pub(super) fn new(window: &mut Window, cx: &mut Context<QuillApp>) -> Self {
        let global = super::chatlist_global::ChatlistGlobal::new(window, cx);
        Self {
            // kit Phase 3: chat list + message history virtualization.
            scroll: VirtualListScrollHandle::new(),
            items: Vec::new(),
            folder_tags: Default::default(),
            swipe: Default::default(),
            menu: None,
            archive_menu: None,
            global,
            pin_reorder: None,
            pin_reorder_archived: false,
            pin_press: None,
            preview: None,
            preview_press: None,
            open_at: None,
            selected: HashSet::new(),
            filter: ChatListFilter::All,
            archive_hint_open: false,
            my_chats_open: None,
            contacts_tab_open: false,
            calls_tab_open: false,
            forum_chats_peek: false,
            forum_column_shown: false,
            pending_close_secret_chat: None,
            pending_story_open: None,
            drop_on_row: Default::default(),
            drop_topic: None,
            keys: Default::default(),
        }
    }
}
