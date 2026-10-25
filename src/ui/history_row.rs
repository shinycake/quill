#![deny(clippy::indexing_slicing)]
use quill::ids::MessageId;
use quill::state::{HistoryMessage, OutboxReceipt};
use std::path::PathBuf;
/// kit Phase 3: everything `session_history_row` needs for one message,
/// snapshotted when the rows are built so the `MessageScroller` renderer
/// can build visible rows without re-deriving them per frame. Playback
/// (the seek bar, a looping GIF) is read when the row renders instead: it
/// changes every frame and never changes the row's size.
#[derive(Clone)]
pub(crate) struct HistoryRowInputs {
    pub(crate) message: HistoryMessage,
    pub(crate) chrome: RowChrome,
    /// `HistoryUi::chrome_epoch` when `chrome` was derived.
    pub(crate) chrome_stamp: u64,
    pub(crate) spot: ChromeSpot,
    pub(crate) highlighted: bool,
    /// First row of a sender run (gets extra top spacing).
    pub(crate) run_start: bool,
    /// Continues the previous bubble of the same sender (no day divider or
    /// unread marker between): the bubble joins it with a small corner.
    pub(crate) joined_above: bool,
    /// Local-day separator rendered above the row ("Today", "Monday",
    /// "12 March") when this row starts a new day.
    pub(crate) day_label: Option<String>,
    /// First unread incoming message: the "Unread messages" divider sits
    /// above it.
    pub(crate) unread_divider: bool,
    /// The message's fact-check block is expanded (`message_ui`): the
    /// row's height depends on it.
    pub(crate) fact_check_expanded: bool,
    pub(crate) is_secret: bool,
    /// The deleted message is dissolving; its progress is read at render
    /// (`vanish_progress`), so the row stays the same while it runs.
    pub(crate) vanishing: bool,
}

/// The part of a row derived from the session rather than from its
/// messages (`conversation::row_chrome`): after a session change only this
/// is derived again.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RowChrome {
    /// kit Phase 4: incoming sender name for the kit `MessageHeader`;
    /// `None` for outgoing rows and when the header collapses (same
    /// direction as the previous row).
    pub(crate) sender: Option<SenderLabel>,
    pub(crate) receipt: OutboxReceipt,
    /// kit Phase 4: `(name, photo)` for the kit `Message` avatar slot.
    /// `None` where the sender can't be identified (groups) or the row is
    /// outgoing — never invented.
    pub(crate) sender_avatar: Option<(String, Option<PathBuf>)>,
    pub(crate) reply_header: Option<quill::state::ReplyHeader>,
    pub(crate) forward_header: Option<quill::state::ForwardHeader>,
    pub(crate) via_bot: Option<String>,
}

/// A row's userpic as it renders: the chrome's `sender_avatar` (an empty
/// name is a spacer) and the sender's playing clip, if any.
#[derive(Clone)]
pub(crate) struct SenderAvatar {
    pub(crate) name: String,
    pub(crate) photo: Option<PathBuf>,
    pub(crate) video: Option<super::userpic_videos::UserpicFrame>,
}

/// Where a row sits among its neighbours, decided by the messages alone:
/// it is kept with the row so its chrome can be derived again without
/// walking the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ChromeSpot {
    pub(crate) sender_changed: bool,
    pub(crate) next_same_sender: bool,
    pub(crate) with_headers: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RowCacheStats {
    /// Whole row lists built.
    pub(crate) rebuilds: u64,
    /// Rows replaced in place: a message changed, or its chrome did.
    pub(crate) rows_patched: u64,
    /// Messages copied out of the session to build or patch rows.
    pub(crate) messages_cloned: u64,
    /// Runs of rows built again around messages that came or went
    /// (`RowsInput::Splice`), the rest of the list kept.
    pub(crate) splices: u64,
    /// Rows those runs built.
    pub(crate) rows_spliced: u64,
}

impl RowCacheStats {
    pub(crate) fn note_built(&mut self, started: std::time::Instant) {
        self.rebuilds += 1;
        super::frame_clock::trace_history_rows("rebuilds", 1);
        super::frame_clock::trace_history_rows("rebuild us", micros(started));
    }

    pub(crate) fn note_spliced(&mut self, rows: usize, started: std::time::Instant) {
        self.splices += 1;
        self.rows_spliced += rows as u64;
        super::frame_clock::trace_history_rows("splices", 1);
        super::frame_clock::trace_history_rows("rows spliced", rows);
        super::frame_clock::trace_history_rows("splice us", micros(started));
    }

    pub(crate) fn note_patched(&mut self, rows: usize, started: std::time::Instant) {
        self.rows_patched += rows as u64;
        super::frame_clock::trace_history_rows("chrome passes", 1);
        super::frame_clock::trace_history_rows("chrome us", micros(started));
        super::frame_clock::trace_history_rows("rows patched", rows);
    }
}

fn micros(started: std::time::Instant) -> usize {
    usize::try_from(started.elapsed().as_micros()).unwrap_or(usize::MAX)
}

/// A message row's sender header: display name plus the sender's
/// Telegram name color (`accent_color_id`), when known.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SenderLabel {
    pub(crate) name: String,
    pub(crate) accent: Option<i32>,
    /// What follows the name: `Some(id)` a Premium emoji status, `Some(0)`
    /// the bare Premium star (`Session::sender_status`).
    pub(crate) status: Option<i64>,
}

#[derive(Clone)]
pub(crate) enum HistoryRow {
    Single(Box<HistoryRowInputs>),
    Album {
        album_id: i64,
        messages: Vec<HistoryMessage>,
        chrome: Box<RowChrome>,
        chrome_stamp: u64,
        spot: ChromeSpot,
        day_label: Option<String>,
        unread_divider: bool,
    },
}

impl HistoryRow {
    pub(crate) fn day_label(&self) -> Option<&str> {
        match self {
            HistoryRow::Single(inputs) => inputs.day_label.as_deref(),
            HistoryRow::Album { day_label, .. } => day_label.as_deref(),
        }
    }

    pub(crate) fn unread_divider(&self) -> bool {
        match self {
            HistoryRow::Single(inputs) => inputs.unread_divider,
            HistoryRow::Album { unread_divider, .. } => *unread_divider,
        }
    }

    pub(crate) fn chrome(&self) -> &RowChrome {
        match self {
            HistoryRow::Single(inputs) => &inputs.chrome,
            HistoryRow::Album { chrome, .. } => chrome,
        }
    }

    pub(crate) fn set_chrome(&mut self, fresh: RowChrome) {
        match self {
            HistoryRow::Single(inputs) => inputs.chrome = fresh,
            HistoryRow::Album { chrome, .. } => **chrome = fresh,
        }
    }

    /// The `HistoryUi::chrome_epoch` the chrome was last derived at.
    pub(crate) fn chrome_stamp(&self) -> u64 {
        match self {
            HistoryRow::Single(inputs) => inputs.chrome_stamp,
            HistoryRow::Album { chrome_stamp, .. } => *chrome_stamp,
        }
    }

    pub(crate) fn set_chrome_stamp(&mut self, stamp: u64) {
        match self {
            HistoryRow::Single(inputs) => inputs.chrome_stamp = stamp,
            HistoryRow::Album { chrome_stamp, .. } => *chrome_stamp = stamp,
        }
    }

    /// The row's copy of message `id`.
    pub(crate) fn message(&self, id: MessageId) -> Option<&HistoryMessage> {
        match self {
            HistoryRow::Single(inputs) => Some(&inputs.message).filter(|m| m.id == id),
            HistoryRow::Album { messages, .. } => messages.iter().find(|m| m.id == id),
        }
    }

    pub(crate) fn message_mut(&mut self, id: MessageId) -> Option<&mut HistoryMessage> {
        match self {
            HistoryRow::Single(inputs) => Some(&mut inputs.message).filter(|m| m.id == id),
            HistoryRow::Album { messages, .. } => messages.iter_mut().find(|m| m.id == id),
        }
    }

    pub(crate) fn spot(&self) -> ChromeSpot {
        match self {
            HistoryRow::Single(inputs) => inputs.spot,
            HistoryRow::Album { spot, .. } => *spot,
        }
    }

    pub(crate) fn first_message(&self) -> Option<&HistoryMessage> {
        match self {
            HistoryRow::Single(inputs) => Some(&inputs.message),
            HistoryRow::Album { messages, .. } => messages.first(),
        }
    }

    /// Same rendered inputs as `other` (height-relevant comparison for the
    /// virtualized list).
    pub(crate) fn renders_like(&self, other: &HistoryRow) -> bool {
        match (self, other) {
            (HistoryRow::Single(a), HistoryRow::Single(b)) => {
                a.message == b.message
                    && a.chrome == b.chrome
                    && a.day_label == b.day_label
                    && a.unread_divider == b.unread_divider
                    && a.fact_check_expanded == b.fact_check_expanded
                    && a.run_start == b.run_start
                    && a.joined_above == b.joined_above
            }
            (
                HistoryRow::Album {
                    messages: a,
                    chrome: ca,
                    ..
                },
                HistoryRow::Album {
                    messages: b,
                    chrome: cb,
                    ..
                },
            ) => {
                a == b
                    && ca.sender == cb.sender
                    && self.day_label() == other.day_label()
                    && self.unread_divider() == other.unread_divider()
            }
            _ => false,
        }
    }

    /// The messages the row shows, oldest first.
    pub(crate) fn messages(&self) -> &[HistoryMessage] {
        match self {
            HistoryRow::Single(inputs) => std::slice::from_ref(&inputs.message),
            HistoryRow::Album { messages, .. } => messages,
        }
    }

    pub(crate) fn first_id(&self) -> Option<MessageId> {
        self.first_message().map(|m| m.id)
    }

    pub(crate) fn last_id(&self) -> Option<MessageId> {
        match self {
            HistoryRow::Single(inputs) => Some(inputs.message.id),
            HistoryRow::Album { messages, .. } => messages.last().map(|m| m.id),
        }
    }

    /// Every message id the row shows (one, or each album item).
    pub(crate) fn message_ids(&self) -> Vec<MessageId> {
        match self {
            HistoryRow::Single(inputs) => vec![inputs.message.id],
            HistoryRow::Album { messages, .. } => messages.iter().map(|m| m.id).collect(),
        }
    }

    pub(crate) fn contains(&self, id: MessageId) -> bool {
        match self {
            HistoryRow::Single(inputs) => inputs.message.id == id,
            HistoryRow::Album { messages, .. } => messages.iter().any(|m| m.id == id),
        }
    }
}
