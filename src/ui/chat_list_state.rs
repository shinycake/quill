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

/// The peek preview of a chat-list row: a long press in progress (the
/// row's chat and when it went down, so the timer that opens it can tell it
/// is still the same press) or the open panel. One value: opening replaces
/// the press, a release or a close ends either.
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) enum PeekPreview {
    #[default]
    Idle,
    /// A press on a row; `over` is the panel still showing under it (a new
    /// press leaves an open panel until its own opens).
    Pressing {
        chat: ChatId,
        started: Instant,
        over: Option<ChatPreviewState>,
    },
    Open(ChatPreviewState),
}

impl PeekPreview {
    pub(super) fn open(&self) -> Option<ChatPreviewState> {
        match *self {
            Self::Open(state)
            | Self::Pressing {
                over: Some(state), ..
            } => Some(state),
            _ => None,
        }
    }

    /// A new press on `chat`; an open panel stays under it.
    pub(super) fn press(&mut self, chat: ChatId, started: Instant) {
        *self = Self::Pressing {
            chat,
            started,
            over: self.open(),
        };
    }

    /// Whether this is still the press `press` started (no release, no
    /// newer press).
    pub(super) fn is_press(&self, chat: ChatId, started: Instant) -> bool {
        matches!(*self, Self::Pressing { chat: c, started: s, .. } if c == chat && s == started)
    }

    /// The chat being long-pressed, if a press is in progress.
    #[cfg(test)]
    pub(super) fn pressing(&self) -> Option<ChatId> {
        match *self {
            Self::Pressing { chat, .. } => Some(chat),
            _ => None,
        }
    }

    /// A release or a click elsewhere: a press ends, an open panel stays.
    pub(super) fn cancel_press(&mut self) {
        if let Self::Pressing { over, .. } = *self {
            *self = over.map_or(Self::Idle, Self::Open);
        }
    }

    /// Nothing is showing and no press is waiting.
    pub(super) fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }
}

/// The pinned-row gesture: nothing, a press waiting for the drag threshold
/// (where it went down, so the dragged row starts under the pointer), or
/// the drag itself. One value, so a press cannot outlive or contradict a drag.
#[derive(Default)]
pub(crate) enum PinStage {
    #[default]
    Idle,
    Pressed {
        chat: i64,
        y: f32,
    },
    Dragging(PinDrag),
}

impl PinStage {
    /// A mouse-down on a pinned row. A drag (or its release slide) is not
    /// cut by another press: the press waits in it for the drag it may become.
    pub(super) fn press(&mut self, chat: i64, y: f32) {
        match self {
            Self::Dragging(drag) => drag.next_press = Some((chat, y)),
            _ => *self = Self::Pressed { chat, y },
        }
    }

    /// The release slide settled: the drag ends, but a press made during it
    /// stays pressed.
    pub(super) fn settle(&mut self) {
        *self = match std::mem::take(self) {
            Self::Dragging(PinDrag {
                next_press: Some((chat, y)),
                ..
            }) => Self::Pressed { chat, y },
            _ => Self::Idle,
        };
    }

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

    /// Ends a drag (a press is kept); true when one was running.
    pub(super) fn end_drag(&mut self) -> bool {
        if matches!(self, Self::Dragging(_)) {
            *self = Self::Idle;
            true
        } else {
            false
        }
    }

    /// The press point for `chat`, consumed by the drag that starts from it.
    pub(super) fn take_press(&mut self, chat: i64) -> Option<f32> {
        match *self {
            Self::Pressed { chat: id, y } if id == chat => {
                *self = Self::Idle;
                Some(y)
            }
            Self::Dragging(ref mut drag) => match drag.next_press {
                Some((id, y)) if id == chat => {
                    drag.next_press = None;
                    Some(y)
                }
                _ => None,
            },
            _ => None,
        }
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
    /// The pinned-row gesture: press, drag (or its release slide).
    pub(super) pin: PinStage,
    /// Slice CL: the peek preview — idle, a long press on a row in
    /// progress, or the open panel. Transient; never an open chat.
    pub(super) peek: PeekPreview,
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
            pin: PinStage::Idle,
            peek: PeekPreview::Idle,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn a_release_ends_a_press_but_leaves_an_open_peek() {
        let started = Instant::now();
        let mut peek = PeekPreview::Idle;
        peek.press(ChatId(5), started);
        assert_eq!(peek.pressing(), Some(ChatId(5)));
        peek.cancel_press();
        assert!(peek.is_idle());
        peek = PeekPreview::Open(ChatPreviewState {
            chat_id: ChatId(5),
            anchor: Point::default(),
        });
        peek.cancel_press();
        assert!(peek.open().is_some(), "an open panel stays");
        assert!(!peek.is_idle());
        // A new press leaves the open panel under it; its release restores it.
        peek.press(ChatId(6), started);
        assert!(peek.open().is_some() && peek.pressing() == Some(ChatId(6)));
        peek.cancel_press();
        assert!(matches!(peek, PeekPreview::Open(_)));
    }

    #[core::prelude::v1::test]
    fn a_press_over_a_drag_does_not_cut_it() {
        let mut pin = PinStage::Idle;
        pin.press(4, 30.);
        assert!(matches!(pin, PinStage::Pressed { chat: 4, .. }));
        pin.press(5, 40.);
        assert!(matches!(pin, PinStage::Pressed { chat: 5, .. }));
        let ids = vec![4, 5];
        let heights = ids.iter().map(|id| (*id, 10.)).collect();
        let reorder = quill::pin_reorder::PinReorder::begin(ids, heights, 4, 1., Instant::now());
        assert!(reorder.is_some());
        if let Some(reorder) = reorder {
            let mut dragging = PinStage::Dragging(PinDrag {
                archived: false,
                reorder,
                next_press: None,
            });
            dragging.press(6, 1.);
            assert!(dragging.drag().is_some(), "the drag or its slide stays");
        }
    }

    #[core::prelude::v1::test]
    fn a_press_is_taken_by_the_drag_of_its_own_row() {
        let mut pin = PinStage::Pressed { chat: 4, y: 30. };
        assert_eq!(
            pin.take_press(9),
            None,
            "another row's drag starts at the pointer"
        );
        assert!(matches!(pin, PinStage::Pressed { .. }));
        assert_eq!(pin.take_press(4), Some(30.));
        assert!(matches!(pin, PinStage::Idle));
        assert!(!pin.end_drag());
    }
}
