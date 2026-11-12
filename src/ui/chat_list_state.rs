//! Chat list: rows, filter, menus, pinning, preview and side tabs.
#![deny(clippy::indexing_slicing)]

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

/// What floats over the window's lists: a context menu (a chat row's, the
/// "Archived chats" row's, a folder tab's or a message's), or the peek
/// preview of a row (a long press in progress, with the row's chat and the
/// timer that opens the panel, or the open panel). One value, held in the
/// window's `MessageUi::menu`: opening one replaces whatever floated before
/// it, so no opener closes the others by hand, and a close ends only its
/// own. The press owns its timer, so a release, a newer press, a menu or a
/// close drops it.
#[derive(Default)]
pub(crate) enum ChatOverlay {
    #[default]
    None,
    /// The right-click menu of a chat row (or a top person).
    RowMenu(ChatMenuState),
    /// The archive row's menu, at the pointer.
    ArchiveMenu(Point<Pixels>),
    /// A folder tab's menu.
    FolderTab(super::folder_extras::FolderTabMenu),
    /// A message's menu, with what was true when it opened
    /// (`message_menu_state`).
    Message(super::message_menu_state::OpenMenu),
    /// A press on a row; `over` is the panel still showing under it (a new
    /// press leaves an open panel until its own opens).
    Pressing {
        /// The row the press went down on (tests read it).
        #[allow(dead_code, reason = "read by tests through `pressing`")]
        chat: ChatId,
        over: Option<ChatPreviewState>,
        _timer: Task<()>,
    },
    Peek(ChatPreviewState),
}

impl ChatOverlay {
    /// A chat row's menu, if that is what floats.
    pub(super) fn row(&self) -> Option<&ChatMenuState> {
        match self {
            Self::RowMenu(menu) => Some(menu),
            _ => None,
        }
    }

    pub(super) fn row_mut(&mut self) -> Option<&mut ChatMenuState> {
        match self {
            Self::RowMenu(menu) => Some(menu),
            _ => None,
        }
    }

    /// A chat row's menu by value, for drawing it.
    pub(super) fn row_copy(&self) -> Option<ChatMenuState> {
        self.row().copied()
    }

    /// Where the archive row's menu is, if that is what floats.
    pub(super) fn archive(&self) -> Option<Point<Pixels>> {
        match self {
            Self::ArchiveMenu(at) => Some(*at),
            _ => None,
        }
    }

    /// A folder tab's menu, if that is what floats.
    pub(super) fn folder_tab(&self) -> Option<super::folder_extras::FolderTabMenu> {
        match self {
            Self::FolderTab(menu) => Some(*menu),
            _ => None,
        }
    }

    /// A menu (a chat row's, the archive row's, a folder tab's or a
    /// message's) is open.
    pub(super) fn menu_open(&self) -> bool {
        matches!(
            self,
            Self::RowMenu(_) | Self::ArchiveMenu(_) | Self::FolderTab(_) | Self::Message(_)
        )
    }

    /// The menu goes, whichever it is; a peek is not the menu's to close.
    pub(super) fn close_menu(&mut self) {
        if self.menu_open() {
            *self = Self::None;
        }
    }

    /// A row's menu opens over whatever floated before it.
    pub(super) fn open_row(&mut self, menu: ChatMenuState) {
        *self = Self::RowMenu(menu);
    }

    /// The archive row's menu opens over whatever floated before it.
    pub(super) fn open_archive(&mut self, at: Point<Pixels>) {
        *self = Self::ArchiveMenu(at);
    }

    /// A folder tab's menu opens over whatever floated before it.
    pub(super) fn open_folder_tab(&mut self, menu: super::folder_extras::FolderTabMenu) {
        *self = Self::FolderTab(menu);
    }

    /// The open peek panel, also while a new press waits over it.
    pub(super) fn peek(&self) -> Option<ChatPreviewState> {
        match *self {
            Self::Peek(state)
            | Self::Pressing {
                over: Some(state), ..
            } => Some(state),
            _ => None,
        }
    }

    /// The panel opens over whatever floated before it.
    pub(super) fn open_peek(&mut self, state: ChatPreviewState) {
        *self = Self::Peek(state);
    }

    /// A new press on `chat`; an open panel stays under it, a menu goes.
    pub(super) fn press(&mut self, chat: ChatId, timer: Task<()>) {
        *self = Self::Pressing {
            chat,
            over: self.peek(),
            _timer: timer,
        };
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
            *self = over.map_or(Self::None, Self::Peek);
        }
    }

    /// The peek goes, a press waiting included; a menu is not its to close.
    pub(super) fn close_peek(&mut self) {
        if !self.peek_idle() {
            *self = Self::None;
        }
    }

    /// No peek panel is showing and no press is waiting.
    pub(super) fn peek_idle(&self) -> bool {
        !matches!(self, Self::Pressing { .. } | Self::Peek(_))
    }
}

/// The pinned-row gesture: nothing, a press waiting for the drag threshold
/// (where it went down, so the dragged row starts under the pointer), the
/// drag itself with the row held, or the slide of the released row into its
/// slot. One value, so a press cannot outlive or contradict a drag and held
/// versus sliding is told by the variant alone.
#[derive(Default)]
pub(crate) enum PinStage {
    #[default]
    Idle,
    Pressed {
        chat: i64,
        y: f32,
    },
    /// The pointer holds the dragged row.
    Dragging(PinDrag),
    /// Released: the row slides into its slot, then the stage settles.
    Sliding(PinDrag),
}

/// What the list draws of a pinned drag, in either of its two stages.
pub(crate) struct PinView<'a> {
    pub(super) reorder: &'a quill::pin_reorder::PinReorder,
    held: bool,
}

impl PinView<'_> {
    pub(super) fn offset(&self, id: i64, now: Instant) -> f32 {
        self.reorder.offset(id, now, self.held)
    }

    pub(super) fn lift(&self, id: i64, now: Instant) -> f32 {
        self.reorder.lift(id, now, self.held)
    }

    /// The drag is held, or a slide still runs: the list needs frames.
    pub(super) fn settled(&self, now: Instant) -> bool {
        !self.reorder.animating(now, self.held)
    }
}

impl PinStage {
    /// A mouse-down on a pinned row. A drag (or its release slide) is not
    /// cut by another press: the press waits in it for the drag it may become.
    pub(super) fn press(&mut self, chat: i64, y: f32) {
        match self {
            Self::Dragging(drag) | Self::Sliding(drag) => drag.next_press = Some((chat, y)),
            _ => *self = Self::Pressed { chat, y },
        }
    }

    /// The release slide settled: the drag ends, but a press made during it
    /// stays pressed. A held drag is not settled.
    pub(super) fn settle(&mut self) {
        *self = match std::mem::take(self) {
            Self::Sliding(PinDrag {
                next_press: Some((chat, y)),
                ..
            }) => Self::Pressed { chat, y },
            Self::Sliding(_) => Self::Idle,
            other => other,
        };
    }

    /// The drag in either stage (for what the list draws).
    pub(super) fn view(&self) -> Option<(&PinDrag, PinView<'_>)> {
        match self {
            Self::Dragging(drag) => Some((
                drag,
                PinView {
                    reorder: &drag.reorder,
                    held: true,
                },
            )),
            Self::Sliding(drag) => Some((
                drag,
                PinView {
                    reorder: &drag.reorder,
                    held: false,
                },
            )),
            _ => None,
        }
    }

    /// The drag in either stage.
    #[cfg(test)]
    pub(super) fn drag(&self) -> Option<&PinDrag> {
        self.view().map(|(drag, _)| drag)
    }

    /// The drag while the pointer holds the row.
    pub(super) fn held(&self) -> Option<&PinDrag> {
        match self {
            Self::Dragging(drag) => Some(drag),
            _ => None,
        }
    }

    pub(super) fn held_mut(&mut self) -> Option<&mut PinDrag> {
        match self {
            Self::Dragging(drag) => Some(drag),
            _ => None,
        }
    }

    /// The pointer let go: the row starts its slide. Returns the pinned
    /// list's side and the dragged order to save; `None` when no row was
    /// held, and then the stage stays as it is (one release reaches here
    /// more than once, and a slide or a press is not this release's).
    pub(super) fn release(&mut self, now: Instant) -> Option<(bool, Vec<i64>)> {
        if !matches!(self, Self::Dragging(_)) {
            return None;
        }
        let Self::Dragging(mut drag) = std::mem::take(self) else {
            return None;
        };
        let order = drag.reorder.release(now);
        let archived = drag.archived;
        *self = Self::Sliding(drag);
        Some((archived, order))
    }

    /// Ends a drag in either stage (a press is kept); true when one was
    /// running.
    pub(super) fn end_drag(&mut self) -> bool {
        if matches!(self, Self::Dragging(_) | Self::Sliding(_)) {
            *self = Self::Idle;
            true
        } else {
            false
        }
    }

    /// One frame of the drag's upkeep: it ends itself when its window lost
    /// focus (the release never comes) and settles when its slide is done.
    /// True while rows still slide and frames must keep coming.
    pub(super) fn advance(&mut self, now: Instant, window_active: bool) -> bool {
        let Some(settled) = self.view().map(|(_, view)| view.settled(now)) else {
            return false;
        };
        if !window_active {
            self.end_drag();
            false
        } else if settled {
            self.settle();
            false
        } else {
            true
        }
    }

    /// The press point for `chat`, consumed by the drag that starts from it.
    pub(super) fn take_press(&mut self, chat: i64) -> Option<f32> {
        match *self {
            Self::Pressed { chat: id, y } if id == chat => {
                *self = Self::Idle;
                Some(y)
            }
            Self::Dragging(ref mut drag) | Self::Sliding(ref mut drag) => match drag.next_press {
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

/// Which of the main menu's chat lists is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum MyChats {
    #[default]
    Closed,
    Groups,
    Channels,
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
    /// Contacts tab, stories menu, suggestions and search tabs.
    pub(super) global: super::chatlist_global::ChatlistGlobal,
    /// The pinned-row gesture: press, drag (or its release slide).
    pub(super) pin: PinStage,
    /// Slice CL3: multi-select mode — checked chat ids. Non-empty while
    /// selecting; rows toggle the check instead of opening the chat and
    /// the select bar offers the bulk actions.
    pub(super) selected: HashSet<i64>,
    /// Slice CL2: chat-list category filter (TGX `ChatFilter` unread /
    /// archive categories, `MainController` pager categories) and the
    /// selected folder tab, one value. `All` is the unfiltered list;
    /// `Unread` filters to unread chats; `Archived` shows only the archive.
    pub(super) view: super::chat_list::ChatListView,
    /// The Downloads panel beside the conversation is open. Here, not in
    /// the `Session`, so it starts closed when an account comes back on
    /// screen.
    pub(super) downloads_panel_open: bool,
    /// The Archive menu's "How does it work?" box is open.
    pub(super) archive_hint_open: bool,
    /// The main menu's My Groups or My Channels list.
    pub(super) my_chats: MyChats,
    /// The sidebar tab: the chat list, the contacts list, or the
    /// recent-calls list with its call settings. One value, so a view
    /// change sets it in one write.
    pub(super) tab: SidebarTab,
    /// Phase B1: pending "Close secret chat" confirm for the open chat
    /// (`closeSecretChat`, schema 1.8.67 line 15242).
    pub(super) pending_close_secret_chat: Option<ChatId>,
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
            global,
            pin: PinStage::Idle,
            selected: HashSet::new(),
            view: Default::default(),
            archive_hint_open: false,
            my_chats: MyChats::Closed,
            tab: SidebarTab::Chats,
            pending_close_secret_chat: None,
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
        let mut peek = ChatOverlay::None;
        peek.press(ChatId(5), Task::ready(()));
        assert_eq!(peek.pressing(), Some(ChatId(5)));
        peek.cancel_press();
        assert!(peek.peek_idle());
        peek.open_peek(ChatPreviewState {
            chat_id: ChatId(5),
            anchor: Point::default(),
        });
        peek.cancel_press();
        assert!(peek.peek().is_some(), "an open panel stays");
        assert!(!peek.peek_idle());
        // A new press leaves the open panel under it; its release restores it.
        peek.press(ChatId(6), Task::ready(()));
        assert!(peek.peek().is_some() && peek.pressing() == Some(ChatId(6)));
        peek.cancel_press();
        assert!(matches!(peek, ChatOverlay::Peek(_)));
    }

    #[core::prelude::v1::test]
    fn the_menu_and_the_peek_replace_each_other_and_close_alone() {
        let menu = ChatMenuState::at(ChatId(1), Point::default());
        let preview = ChatPreviewState {
            chat_id: ChatId(2),
            anchor: Point::default(),
        };
        let mut overlay = ChatOverlay::None;
        overlay.open_peek(preview);
        overlay.open_row(menu);
        assert!(overlay.peek().is_none() && overlay.row().is_some());
        overlay.close_peek();
        assert!(overlay.row().is_some(), "a peek close leaves the menu");
        overlay.open_peek(preview);
        assert!(!overlay.menu_open() && overlay.peek().is_some());
        overlay.close_menu();
        assert!(overlay.peek().is_some(), "a menu close leaves the peek");
        overlay.open_archive(Point::default());
        assert!(overlay.peek().is_none() && overlay.archive().is_some());
        assert!(overlay.row().is_none(), "one menu at a time");
        overlay.press(ChatId(3), Task::ready(()));
        assert!(!overlay.menu_open(), "a press ends a menu");
        overlay.open_row(menu);
        overlay.cancel_press();
        assert!(overlay.row().is_some(), "a release does not close a menu");
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

    /// One release of the button reaches the list more than once (the row's
    /// drop, the list's mouse-up): only the first finds a held row, and the
    /// later ones leave the slide, and a press made during it, alone.
    #[core::prelude::v1::test]
    fn a_second_release_leaves_the_slide_and_a_press_alone() {
        let now = Instant::now();
        let ids = vec![4, 5];
        let heights = ids.iter().map(|id| (*id, 10.)).collect();
        let reorder = quill::pin_reorder::PinReorder::begin(ids, heights, 4, 1., now);
        assert!(reorder.is_some());
        if let Some(reorder) = reorder {
            let mut pin = PinStage::Dragging(PinDrag {
                archived: false,
                reorder,
                next_press: None,
            });
            assert!(pin.release(now).is_some(), "the held row is let go");
            assert!(matches!(pin, PinStage::Sliding(_)));
            pin.press(5, 12.);
            assert_eq!(pin.release(now), None, "nothing is held any more");
            assert!(matches!(pin, PinStage::Sliding(_)), "the slide goes on");
            assert_eq!(pin.take_press(5), Some(12.), "its press is kept");
        }
        let mut pressed = PinStage::Pressed { chat: 4, y: 30. };
        assert_eq!(pressed.release(now), None);
        assert!(matches!(pressed, PinStage::Pressed { chat: 4, .. }));
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
