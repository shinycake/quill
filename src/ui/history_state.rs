//! Message history: rows, window, scroll probes, highlights and pinned bar.
#![deny(clippy::indexing_slicing)]

use super::history::HistoryShared;
use super::*;
use gpui_kit::component::message_scroller::MessageScrollerState;
use gpui_kit::*;
use quill::ids::ChatId;
use quill::ids::MessageId;
use std::collections::HashMap;

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
    /// Messages whose row's bottom edge was inside the viewport at the last
    /// paint (tdesktop `HistoryInner` reads an unread message once its
    /// bottom is shown), not the list's overdraw. Reported as
    /// `viewMessages` on the next render.
    pub(super) seen: std::rc::Rc<std::cell::RefCell<Option<(ChatId, Vec<MessageId>)>>>,
    /// Last `(chat, message ids)` reported as visible, to skip repeats.
    pub(super) reported_visible: Option<(ChatId, Vec<MessageId>)>,
    /// Window activity at the last render (message effects play only
    /// while it is active).
    pub(super) window_active: bool,
    /// kit Phase 3: per-render shared inputs for history rows (files,
    /// downloads, media roots) so visible-row rendering doesn't re-clone.
    pub(super) shared: HistoryShared,
    /// kit Phase 3: `(open_chat_id, open_topic)` the scroller state was
    /// last synced for — a change means reset + scroll to bottom.
    pub(super) key: Option<(i64, Option<i32>, i64)>,
    /// "Jump to root" was asked while older replies were still loading.
    pub(super) thread_root_jump: bool,
    /// kit Phase 3: first/last message ids of the last-synced history, to
    /// tell appends apart from prepends without re-scanning.
    pub(super) ends: Option<(MessageId, MessageId)>,
    /// `HistoryState::window_epoch` the scroller last anchored for: a
    /// change (window replaced) re-anchors like opening the chat.
    pub(super) window_epoch: u64,
    /// The scroller must anchor once rows exist: the saved position, else
    /// the jump highlight, else the "Unread messages" divider, else the
    /// bottom.
    pub(super) anchor_pending: bool,
    /// Where each chat, topic or thread was left away from the tail.
    pub(super) scroll_memory: quill::state::ScrollMemory,
    /// The window stopped short of the latest message at the last render:
    /// rows appended since are a newer page, not live messages.
    pub(super) had_newer: bool,
    /// What `history_rows` were last built from (`None`: not built yet).
    pub(super) rows_key: Option<super::conversation::HistoryRowsKey>,
    /// Generation of the rows' chrome (`HistoryRow::chrome_stamp`): rows
    /// stamped with an older one are derived again when drawn.
    pub(super) chrome_epoch: u64,
    /// `Session::revision_without_file_progress` `chrome_epoch` stands for.
    pub(super) chrome_seen: Option<u64>,
    /// Rows drawn with stale chrome since the last render.
    pub(super) chrome_due: std::cell::RefCell<Vec<usize>>,
    /// A deferred notify for `chrome_due` is on its way (renders that skip
    /// the kept-rows branch leave the list undrained).
    pub(super) chrome_notify_pending: std::cell::Cell<bool>,
    pub(super) row_stats: RowCacheStats,
    /// Tests: trust `Session::revision` the way a live session does.
    #[cfg(test)]
    pub(super) follow_revision_in_tests: bool,
    /// (ready files, downloading files) at the last history render; a
    /// change remeasures the virtualized rows (media grew in place).
    pub(super) media_signature: (u64, usize),
    /// kit Phase 3: last chat-search highlight the scroller jumped to —
    /// avoids re-scrolling every frame while the highlight is set.
    pub(super) last_highlight: Option<MessageId>,
    /// `(jump serial, start)` of the running jump-highlight fade.
    pub(super) highlight_fade: Option<(u64, std::time::Instant)>,
    /// B11: the reaction that just flew from the message (message, glyph, start).
    pub(super) reaction_fly: Option<super::history_fx::ReactionFly>,
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
    /// Rows painted in the last frame with their window bounds, kept until
    /// the next paint: drag selection hit-tests the pointer against them.
    pub(super) hit_rows: std::rc::Rc<std::cell::RefCell<Vec<(usize, Bounds<Pixels>)>>>,
    /// The list's window bounds at the last paint (drag selection
    /// autoscrolls when the pointer leaves them).
    pub(super) viewport: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// The open chat's palette (its emoji theme, its wallpaper's tone),
    /// set when the history renders and applied while rows are built.
    pub(super) palette: std::cell::Cell<Option<super::chat_palette::ChatPalette>>,
    /// Pinned bar position per chat: index into the pinned list, newest
    /// first. A click on the bar jumps there and steps to the next older.
    pub(super) pinned_cursor: HashMap<i64, usize>,
    /// Chats whose pinned bar was hidden, with the newest pinned message
    /// at the time: the bar returns when a newer message is pinned.
    pub(super) hidden_pinned: HashMap<i64, MessageId>,
    /// The open chat's pinned-messages list (bar's list button).
    pub(super) pinned_list_open: bool,
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
    /// Phase B3: the open chat whose self-destruct countdown badges are
    /// ticking (`Some` exactly while the 1s tick task runs). Mirrors
    /// `slow_mode_tick_chat`.
    pub(super) self_destruct_tick_chat: Option<ChatId>,
    /// The open chat whose live-location countdowns are being refreshed
    /// (`Some` exactly while that task runs; see `live_location_tick`).
    pub(super) live_location_tick_chat: Option<ChatId>,
    /// The info card under the open forum topic's strip.
    pub(super) topic_info_open: bool,
    /// The info card under the open reply thread's root bar.
    pub(super) thread_info_open: bool,
    /// The history's width class at the last paint (tdesktop
    /// `Adaptive::ChatLayout`, `chat_wide_ui`).
    pub(super) chat_layout: std::rc::Rc<std::cell::Cell<quill::chat_wide::ChatLayout>>,
    /// The history's width at the last paint, as bubble layout reads it:
    /// a narrow history narrows the bubbles (`bubble_width::BubbleRoom`).
    pub(super) width: std::rc::Rc<std::cell::Cell<quill::bubble_layout::HistoryWidth>>,
    /// Whether the history has painted and measured `width` itself; until
    /// it has, `width` is seeded from the pane the history is laid out in
    /// (`seed_history_width`).
    pub(super) width_probed: std::rc::Rc<std::cell::Cell<bool>>,
    /// The pull to the next unread channel (`pull_next_ui`).
    pub(super) pull: super::pull_next_ui::PullUi,
}

impl HistoryUi {
    pub(super) fn new(cx: &mut App) -> Self {
        Self {
            scroller: cx.new(|cx| MessageScrollerState::new(0, cx)),
            rows: Vec::new(),
            painted_ids: Default::default(),
            seen: Default::default(),
            reported_visible: None,
            window_active: false,
            shared: HistoryShared::default(),
            key: None,
            thread_root_jump: false,
            ends: None,
            window_epoch: 0,
            anchor_pending: false,
            scroll_memory: Default::default(),
            had_newer: false,
            rows_key: None,
            chrome_epoch: 0,
            chrome_seen: None,
            chrome_due: Default::default(),
            chrome_notify_pending: Default::default(),
            row_stats: RowCacheStats::default(),
            #[cfg(test)]
            follow_revision_in_tests: false,
            media_signature: (0, 0),
            last_highlight: None,
            highlight_fade: None,
            reaction_fly: None,
            scroll_date: Default::default(),
            scroll_probe: Default::default(),
            scroll_top_probe: Default::default(),
            scroll_view_probe: Default::default(),
            hit_rows: Default::default(),
            viewport: Default::default(),
            palette: Default::default(),
            pinned_cursor: HashMap::new(),
            hidden_pinned: HashMap::new(),
            pinned_list_open: false,
            autoscroll: Default::default(),
            stream_reveal: Default::default(),
            vanishing: Default::default(),
            vanish_now: Default::default(),
            vanish_conflicts: Default::default(),
            vanish_clock: Default::default(),
            self_destruct_tick_chat: None,
            live_location_tick_chat: None,
            topic_info_open: false,
            thread_info_open: false,
            chat_layout: Default::default(),
            width: Default::default(),
            width_probed: Default::default(),
            pull: Default::default(),
        }
    }
}
