//! Message history: rows, window, scroll probes, highlights and pinned bar.
#![deny(clippy::indexing_slicing)]

use super::history::HistoryShared;
use super::*;
use gpui_kit::component::message_scroller::MessageScrollerState;
use gpui_kit::*;
use quill::ids::MessageId;
use quill::ids::{AccountChat, ChatId};
use std::collections::HashMap;

/// A place the reader asked the history to go that may have to wait for
/// pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ScrollGoal {
    /// The first row of the thread `thread_id`, its root post: due once
    /// the older replies are all loaded ("jump to root"). Another thread
    /// open (or none) ends it.
    ThreadRoot { thread_id: i64 },
}

/// Where the open history is being taken: nothing, a goal the reader asked
/// for that waits for its rows, or a landing that scrolled to its target
/// and corrects after each paint. A newer aim replaces the older; the
/// reader taking over the view ends only a landing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) enum ScrollAim {
    #[default]
    Free,
    Goal(ScrollGoal),
    Landing(super::conversation::landing::Landing),
}

impl ScrollAim {
    pub(super) fn goal(&self) -> Option<ScrollGoal> {
        match *self {
            Self::Goal(goal) => Some(goal),
            Self::Free | Self::Landing(_) => None,
        }
    }

    pub(super) fn landing(&self) -> Option<super::conversation::landing::Landing> {
        match *self {
            Self::Landing(landing) => Some(landing),
            Self::Free | Self::Goal(_) => None,
        }
    }

    /// The goal is reached or gone: the aim is free (a landing stays).
    pub(super) fn take_goal(&mut self) -> Option<ScrollGoal> {
        let goal = self.goal()?;
        *self = Self::Free;
        Some(goal)
    }

    /// The reader took over the view (a goal waiting for its rows stays).
    pub(super) fn end_landing(&mut self) {
        if matches!(self, Self::Landing(_)) {
            *self = Self::Free;
        }
    }
}

/// The user's choice, on a narrow window where the forum's topic column
/// replaces the chat list, to see the list again. It belongs to the forum
/// it was made over: it counts only while that chat is the open one, and
/// opening any chat ends it. Where the column sits is a pure function of
/// this, the session and the window's width (`forum_column_layout`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ForumPeek {
    #[default]
    Off,
    /// The chat list is shown over the topics of `forum`.
    Chats { forum: ChatId },
}

impl ForumPeek {
    /// Whether the peek is on while `open` is the open chat.
    pub(crate) fn is_for(self, open: Option<ChatId>) -> bool {
        matches!(self, Self::Chats { forum } if Some(forum) == open)
    }
}

pub(crate) struct HistoryUi {
    /// kit Phase 3: message-history virtualization — scroller state with
    /// tail-following, owned by the app so prepend/append keep the anchor.
    pub(super) scroller: Entity<MessageScrollerState>,
    /// kit Phase 3: per-row render inputs for the visible history window.
    /// Rebuilt when `rows_key` changes and patched in place when only the
    /// chrome changes; the `MessageScroller` renderer only builds
    /// elements for visible indices.
    pub(super) rows: Vec<HistoryRow>,
    /// Each painted row's bounds and message ids, collected during the
    /// list's paint (overdraw rows included); the history's own canvas
    /// turns them into `seen`.
    pub(super) painted_ids: std::rc::Rc<std::cell::RefCell<Vec<(Bounds<Pixels>, Vec<MessageId>)>>>,
    /// The user's peek at the chat list over the forum's topic column.
    pub(super) forum_peek: ForumPeek,
    /// Messages whose row's bottom edge was inside the viewport at the last
    /// paint (tdesktop `HistoryInner` reads an unread message once its
    /// bottom is shown), not the list's overdraw. Reported as
    /// `viewMessages` on the next render.
    pub(super) seen: std::rc::Rc<std::cell::RefCell<Option<(ChatId, Vec<MessageId>)>>>,
    /// Messages of the rows inside the viewport at the last paint.
    pub(super) on_screen: std::rc::Rc<std::cell::RefCell<Vec<MessageId>>>,
    /// What was last reported as visible, to skip repeats within one open.
    pub(super) reported_visible: Option<ReportedVisible>,
    /// Window activity at the last render (message effects play only
    /// while it is active).
    pub(super) window_active: bool,
    /// kit Phase 3: per-render shared inputs for history rows (files,
    /// downloads, media roots) so visible-row rendering doesn't re-clone.
    pub(super) shared: HistoryShared,
    /// kit Phase 3: the `(open_chat_id, open_topic, thread)` and window the
    /// scroller state was last synced for — a change means reset + anchor:
    /// the saved position, else the jump highlight, else the "Unread
    /// messages" divider, else the bottom.
    pub(super) anchor: super::history_anchor::WindowAnchor,
    /// Where the history is being taken: a goal the reader asked for that
    /// waits for its rows, or the landing that scrolled there and corrects
    /// after each paint. One value, so a newer aim replaces the older.
    pub(super) aim: ScrollAim,
    /// kit Phase 3: first/last message ids of the last-synced history, to
    /// tell appends apart from prepends without re-scanning.
    pub(super) ends: Option<(MessageId, MessageId)>,
    /// Where each chat, topic or thread was left away from the tail.
    pub(super) scroll_memory: quill::state::ScrollMemory,
    /// The window stopped short of the latest message at the last render:
    /// rows appended since are a newer page, not live messages.
    pub(super) had_newer: bool,
    /// What `history_rows` were last built from (`None`: not built yet).
    /// Cleared only by [`HistoryUi::invalidate_rows`]. `ends`, `had_newer`,
    /// the chrome marks and `media_signature` describe the rows that are
    /// still there, which the next build is compared against (the splice
    /// keeps the reader's anchor); they are not reset with the key.
    pub(super) rows_key: Option<super::conversation::HistoryRowsKey>,
    /// Generation of the rows' chrome (`HistoryRow::chrome_stamp`): rows
    /// stamped with an older one are derived again when drawn.
    pub(super) chrome_epoch: u64,
    /// `Session::revision_without_file_progress` `chrome_epoch` stands for.
    pub(super) chrome_seen: Option<u64>,
    /// Rows drawn with stale chrome since the last render, and whether the
    /// notify that renders them again is on its way.
    pub(super) chrome_due: std::cell::RefCell<super::history_anchor::ChromeDue>,
    pub(super) row_stats: RowCacheStats,
    /// Tests: trust `Session::revision` the way a live session does.
    #[cfg(test)]
    pub(super) follow_revision_in_tests: bool,
    /// (ready files, downloading files) at the last history render; a
    /// change remeasures the virtualized rows (media grew in place).
    pub(super) media_signature: (u64, usize),
    /// The channel's ads drawn in the main history (`sponsored_spots`).
    pub(super) sponsored: super::sponsored_spots::SponsoredInHistory,
    /// `(jump serial, start)` of the running jump-highlight fade.
    pub(super) highlight_fade: Option<(u64, std::time::Instant)>,
    /// B11: the reaction that just flew from the message (message, glyph,
    /// start). The first frame after the fly's run takes it out
    /// (`reaction_fly_for`), so none outlives its animation.
    pub(super) reaction_fly: std::cell::Cell<Option<super::history_fx::ReactionFly>>,
    /// Floating date pill state (shown while scrolling the history).
    pub(super) scroll_date: super::history_fx::ScrollDate,
    /// Rows painted this frame: `(row, bounds, starts its day)`.
    pub(super) scroll_probe:
        std::rc::Rc<std::cell::RefCell<Vec<super::history_fx::ScrollProbeRow>>>,
    /// Top visible row of the last paint: `(row, its day separator is at
    /// the top edge)`.
    pub(super) scroll_top_probe: std::rc::Rc<std::cell::Cell<Option<(usize, bool)>>>,
    /// First and last row on screen at the last paint (the page keys scroll
    /// by this many rows).
    pub(super) scroll_view_probe: std::rc::Rc<std::cell::Cell<Option<(usize, usize)>>>,
    /// Where the "Unread messages" bar was painted in the last frame.
    pub(super) bar_probe: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// Where the bar sat in its row at the last open (`BarOffsets`).
    pub(super) bar_offsets: super::conversation::landing::BarOffsets,
    /// Rows painted in the last frame with their window bounds, kept until
    /// the next paint: drag selection hit-tests the pointer against them.
    pub(super) hit_rows: std::rc::Rc<std::cell::RefCell<Vec<(usize, Bounds<Pixels>)>>>,
    pub(super) row_hover: super::history::RowHover,
    /// The list's window bounds at the last paint (drag selection
    /// autoscrolls when the pointer leaves them).
    pub(super) viewport: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// The open chat's palette (its emoji theme, its wallpaper's tone),
    /// set when the history renders and applied while rows are built.
    pub(super) palette: std::cell::Cell<Option<super::chat_palette::ChatPalette>>,
    /// Pinned bar position per chat: index into the pinned list, newest
    /// first. A click on the bar jumps there and steps to the next older.
    pub(super) pinned_cursor: HashMap<AccountChat, usize>,
    /// Chats whose pinned bar was hidden, with the newest pinned message
    /// at the time: the bar returns when a newer message is pinned.
    pub(super) hidden_pinned: HashMap<AccountChat, MessageId>,
    /// The chat whose pinned-messages list is open (the bar's list
    /// button); another chat's bar shows it closed.
    pub(super) pinned_list_open: Option<AccountChat>,
    /// Middle-click autoscroll over the history (`autoscroll_ui`).
    pub(super) autoscroll: super::autoscroll_ui::AutoscrollUi,
    /// Smooth reveal of a bot's streaming reply (`bot_stream`).
    pub(super) stream_reveal: std::cell::RefCell<super::bot_stream::StreamReveal>,
    /// Deleted messages still dissolving (`vanish`).
    pub(super) vanishing: std::cell::RefCell<Vec<super::vanish::Vanishing>>,
    /// The instant the rows being worked out are judged at, so the rows key
    /// and the build agree on how far each dissolve has got.
    pub(super) vanish_now: std::cell::Cell<Option<std::time::Instant>>,
    /// Frames in a row the dissolve list was borrowed elsewhere
    /// (`vanish::conflict_keeps_running`).
    pub(super) vanish_conflicts: std::cell::Cell<u16>,
    /// Where the dissolves read the time: the wall clock in the app, a
    /// clock that only moves when told to in tests.
    pub(super) vanish_clock: std::cell::Cell<super::vanish::VanishClock>,
    /// The open chat whose live-location countdowns are being refreshed,
    /// with the task that does it: dropping the owner stops the tick, and
    /// the task clears this itself when it ends (see `live_location_tick`).
    pub(super) live_location_tick: Option<(ChatId, Task<()>)>,
    /// The history's width, class and pane: seeded from the pane it is laid
    /// out in until the paint probe measures it (`chat_wide_ui`).
    pub(super) measure: std::rc::Rc<std::cell::Cell<super::history_anchor::HistoryMeasure>>,
    /// The pull to the next unread channel (`pull_next_ui`).
    pub(super) pull: super::pull_next_ui::PullUi,
    /// The subsection tab strip over this window's conversation (forum
    /// topics, bot topics, channel direct messages): each window scrolls
    /// its own.
    pub(super) subsection_tabs: SubsectionTabsScroll,
}

/// Where a window's subsection tabs are scrolled (`subsection_tabs`).
#[derive(Default)]
pub(super) struct SubsectionTabsScroll {
    /// The horizontal strip (top or bottom layout).
    pub(super) strip: gpui_kit::ScrollHandle,
    /// The Left layout's column.
    pub(super) column: gpui_kit::ScrollHandle,
}

/// The open chat is left as any leave: the draft is saved through the old
/// client (`leave_chat_for_switch`).
impl super::on_screen::Farewell for HistoryUi {
    fn farewell(
        app: &mut super::app::QuillApp,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<super::app::QuillApp>,
    ) {
        app.leave_chat_for_switch(window, cx);
    }
}

impl HistoryUi {
    /// The deferred chrome notify landed (or its window is gone): none is on
    /// its way any more. A borrowed list is left to the next render.
    pub(super) fn chrome_notify_done(&self) {
        if let Ok(mut due) = self.chrome_due.try_borrow_mut() {
            due.notified();
        }
    }

    #[cfg(test)]
    #[cfg(feature = "demo-capture")]
    pub(super) fn painted_row_center(&self, id: MessageId) -> Point<Pixels> {
        let rows = self.hit_rows.borrow();
        let first_id = |ix: usize| self.rows.get(ix).and_then(|row| row.first_id());
        let row = rows.iter().find(|(ix, _)| first_id(*ix) == Some(id));
        row.expect("the row is painted").1.center()
    }

    /// The rows must be built again on the next render (a setting that
    /// changes how they look, a resize): the one place that clears
    /// `rows_key`.
    pub(in crate::ui) fn invalidate_rows(&mut self) {
        self.rows_key = None;
    }

    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            scroller: cx.new(|cx| MessageScrollerState::new(0, cx)),
            rows: Vec::new(),
            painted_ids: Default::default(),
            forum_peek: ForumPeek::Off,
            seen: Default::default(),
            on_screen: Default::default(),
            reported_visible: None,
            window_active: false,
            shared: HistoryShared::default(),
            anchor: Default::default(),
            aim: ScrollAim::Free,
            ends: None,
            scroll_memory: Default::default(),
            had_newer: false,
            rows_key: None,
            chrome_epoch: 0,
            chrome_seen: None,
            chrome_due: Default::default(),
            row_stats: RowCacheStats::default(),
            #[cfg(test)]
            follow_revision_in_tests: false,
            media_signature: (0, 0),
            sponsored: Default::default(),
            highlight_fade: None,
            reaction_fly: Default::default(),
            scroll_date: Default::default(),
            scroll_probe: Default::default(),
            scroll_top_probe: Default::default(),
            scroll_view_probe: Default::default(),
            bar_probe: Default::default(),
            bar_offsets: Default::default(),
            hit_rows: Default::default(),
            row_hover: Default::default(),
            viewport: Default::default(),
            palette: Default::default(),
            pinned_cursor: HashMap::new(),
            hidden_pinned: HashMap::new(),
            pinned_list_open: None,
            autoscroll: Default::default(),
            stream_reveal: Default::default(),
            vanishing: Default::default(),
            vanish_now: Default::default(),
            vanish_conflicts: Default::default(),
            vanish_clock: Default::default(),
            live_location_tick: None,
            measure: Default::default(),
            pull: Default::default(),
            subsection_tabs: Default::default(),
        }
    }
}

/// The rows last reported as visible, and the open of the chat's history
/// they were reported in: reopening the chat reports its rows again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ReportedVisible {
    pub(super) chat: ChatId,
    pub(super) open: u64,
    pub(super) ids: Vec<MessageId>,
}
