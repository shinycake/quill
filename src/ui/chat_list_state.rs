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

/// A pinned-chat drag (or its release slide) and the pinned list it
/// reorders: the main list's, or the archive's. One value, so the list
/// can never be kept apart from the drag by hand.
pub(crate) struct PinDrag {
    pub(super) archived: bool,
    pub(super) reorder: quill::pin_reorder::PinReorder,
    /// A press on another pinned row that went down during the release
    /// slide: the slide goes on, and that row's drag starts under the pointer.
    pub(super) next_press: Option<(i64, f32)>,
}

/// The pinned-row gesture: a press that may become a drag, then the drag
/// (and its release slide). One value, so a press and a drag are never
/// kept in step by hand.
#[derive(Default)]
pub(crate) enum PinGesture {
    #[default]
    Idle,
    /// Where the press on a pinned row went down, so the dragged row
    /// starts under the pointer instead of where the drag threshold fired.
    Pressed {
        chat: i64,
        y: f32,
    },
    Dragging(PinDrag),
}

impl PinGesture {
    pub(super) fn drag(&self) -> Option<&PinDrag> {
        match self {
            Self::Dragging(drag) => Some(drag),
            _ => None,
        }
    }

    pub(super) fn drag_mut(&mut self) -> Option<&mut PinDrag> {
        match self {
            Self::Dragging(drag) => Some(drag),
            _ => None,
        }
    }

    /// A press went down on `chat`. A release slide in progress goes on;
    /// the press waits in it for the drag it may become.
    pub(super) fn press(&mut self, chat: i64, y: f32) {
        match self {
            Self::Dragging(drag) => drag.next_press = Some((chat, y)),
            _ => *self = Self::Pressed { chat, y },
        }
    }

    /// Where the press on `chat` went down, taking it.
    pub(super) fn take_press(&mut self, chat: i64) -> Option<f32> {
        match self {
            Self::Pressed { chat: pressed, y } if *pressed == chat => {
                let y = *y;
                *self = Self::Idle;
                Some(y)
            }
            Self::Dragging(drag) => match drag.next_press {
                Some((pressed, y)) if pressed == chat => {
                    drag.next_press = None;
                    Some(y)
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// The release slide settled: the gesture ends, but a press made
    /// during it stays pressed.
    pub(super) fn settle(&mut self) {
        *self = match std::mem::take(self) {
            Self::Dragging(PinDrag {
                next_press: Some((chat, y)),
                ..
            }) => Self::Pressed { chat, y },
            _ => Self::Idle,
        };
    }

    /// End the gesture, unsaved. Whether a drag was running.
    pub(super) fn end(&mut self) -> bool {
        matches!(std::mem::take(self), Self::Dragging(_))
    }
}

/// The peek preview of a chat-list row: a long press in progress, then the
/// open preview. One value: the press ends when the preview opens.
#[derive(Clone, Copy, Default)]
pub(crate) enum ChatPeek {
    #[default]
    Idle,
    /// A press on this row since `started`; held long enough, it opens.
    Pressing {
        chat_id: ChatId,
        started: Instant,
    },
    Open(ChatPreviewState),
}

impl ChatPeek {
    pub(super) fn open(&self) -> Option<ChatPreviewState> {
        match *self {
            Self::Open(state) => Some(state),
            _ => None,
        }
    }

    pub(super) fn pressing(&self) -> Option<(ChatId, Instant)> {
        match *self {
            Self::Pressing { chat_id, started } => Some((chat_id, started)),
            _ => None,
        }
    }

    /// The press ended (release, drag, jump); an open preview stays.
    pub(super) fn end_press(&mut self) {
        if matches!(self, Self::Pressing { .. }) {
            *self = Self::Idle;
        }
    }

    pub(super) fn is_active(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    pub(super) fn close(&mut self) {
        *self = Self::Idle;
    }
}

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
    /// Online-dot and unread-badge animation state of the rows drawn. The
    /// account's: a switch starts it empty, so a row of the next account
    /// snaps on first sight instead of animating from another account's
    /// state (a chat id alone does not name the same chat across accounts).
    pub(super) row_fx: std::cell::RefCell<quill::row_fx::RowFxMap>,
    /// Slice CL1: right-click chat-row context menu target + window
    /// position.
    pub(super) menu: Option<ChatMenuState>,
    /// Right-click menu of the "Archived chats" row (window position).
    pub(super) archive_menu: Option<Point<Pixels>>,
    /// Contacts tab, stories menu, suggestions and search tabs.
    pub(super) global: super::chatlist_global::ChatlistGlobal,
    /// Pinned-chat press, drag in progress (or its release slide), with
    /// the pinned list it reorders.
    pub(super) pin: PinGesture,
    /// Slice CL: the peek preview — a long press on a row, then the open
    /// preview. Transient; never an open chat.
    pub(super) peek: ChatPeek,
    /// Slice CL3: multi-select mode — checked chat ids. Non-empty while
    /// selecting; rows toggle the check instead of opening the chat and
    /// the select bar offers the bulk actions.
    pub(super) selected: HashSet<i64>,
    /// Slice CL2: chat-list category filter (TGX `ChatFilter` unread /
    /// archive categories, `MainController` pager categories). `All` is
    /// the unfiltered list; `Unread` filters to unread chats;
    /// `Archived` shows only the archive.
    pub(super) filter: ChatListFilter,
    /// The Downloads panel beside the conversation is open. Here, not in
    /// the `Session`, so it starts closed when an account comes back on
    /// screen.
    pub(super) downloads_panel_open: bool,
    /// The Archive menu's "How does it work?" box is open.
    pub(super) archive_hint_open: bool,
    /// The main menu's My Groups (`false`) or My Channels (`true`) list is open.
    pub(super) my_chats_open: Option<bool>,
    /// The sidebar tab: the chat list, the contacts list, or the
    /// recent-calls list with its call settings. One value, so a view
    /// change sets it in one write.
    pub(super) tab: SidebarTab,
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

/// Which list the sidebar shows (the tab strip under its header).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SidebarTab {
    #[default]
    Chats,
    Contacts,
    Calls,
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
            downloads_panel_open: false,
            row_fx: Default::default(),
            menu: None,
            archive_menu: None,
            global,
            pin: PinGesture::Idle,
            peek: ChatPeek::Idle,
            selected: HashSet::new(),
            filter: ChatListFilter::All,
            archive_hint_open: false,
            my_chats_open: None,
            tab: SidebarTab::Chats,
            pending_close_secret_chat: None,
            pending_story_open: None,
            drop_on_row: Default::default(),
            drop_topic: None,
            keys: Default::default(),
        }
    }
}

/// Nothing to say: its state is the account's only app-side.
impl super::on_screen::Farewell for ChatListUi {}
