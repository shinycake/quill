use super::*;
use quill::ids::MessageId;
use quill::state::{HistoryMessage, OutboxReceipt};
use std::path::PathBuf;
/// kit Phase 3: everything `session_history_row` needs for one message,
/// snapshotted per render so the `MessageScroller` renderer can build
/// visible rows without re-deriving per frame.
#[derive(Clone)]
pub(crate) struct HistoryRowInputs {
    pub(crate) message: HistoryMessage,
    /// kit Phase 4: incoming sender name for the kit `MessageHeader`;
    /// `None` for outgoing rows and when the header collapses (same
    /// direction as the previous row). Replaces the old `"You · sent"`
    /// label — delivery state now lives in the in-bubble footer.
    pub(crate) sender: Option<SenderLabel>,
    /// kit Phase 4: outbox delivery state for the in-bubble footer
    /// (`✓` sent, `✓✓` read, `…` while pending).
    pub(crate) receipt: OutboxReceipt,
    /// kit Phase 4: `(name, photo)` for the kit `Message` avatar slot.
    /// `None` where the sender can't be identified (groups) or the row is
    /// outgoing — never invented.
    pub(crate) sender_avatar: Option<(String, Option<PathBuf>)>,
    pub(crate) highlighted: bool,
    /// First row of a sender run (gets extra top spacing).
    pub(crate) run_start: bool,
    /// Local-day separator rendered above the row ("Today", "Monday",
    /// "12 March") when this row starts a new day.
    pub(crate) day_label: Option<String>,
    /// First unread incoming message: the "Unread messages" divider sits
    /// above it.
    pub(crate) unread_divider: bool,
    pub(crate) quote_preview: Option<String>,
    pub(crate) forward_from: Option<String>,
    pub(crate) seek_bar: Option<SeekBarView>,
    pub(crate) animation_playing: bool,
    pub(crate) animation_frame: Option<std::sync::Arc<gpui_kit::RenderImage>>,
    pub(crate) video_playing: bool,
    pub(crate) video_frame: Option<PathBuf>,
    pub(crate) is_secret: bool,
    /// Delete animation progress (0..1) while the deleted message
    /// dissolves; `None` for live messages.
    pub(crate) vanishing: Option<f32>,
}

/// kit Phase 3: one virtualized history row — a single message or a media
/// album group (albums render as one row, as before).
/// A message row's sender header: display name plus the sender's
/// Telegram name color (`accent_color_id`), when known.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SenderLabel {
    pub(crate) name: String,
    pub(crate) accent: Option<i32>,
}

#[derive(Clone)]
pub(crate) enum HistoryRow {
    // Boxed: the per-row inputs are ~880 bytes; the album variant is
    // small (clippy `large_enum_variant`).
    Single(Box<HistoryRowInputs>),
    Album {
        album_id: i64,
        messages: Vec<HistoryMessage>,
        // kit Phase 4: precomputed per-row chrome (sender header /
        // outbox receipt / avatar) — keeps the large `ChatSummary` out
        // of the variant.
        sender: Option<SenderLabel>,
        receipt: OutboxReceipt,
        sender_avatar: Option<(String, Option<PathBuf>)>,
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

    /// Same rendered inputs as `other` (height-relevant comparison for the
    /// virtualized list). Playback frames are excluded: they never change
    /// a row's size.
    pub(crate) fn renders_like(&self, other: &HistoryRow) -> bool {
        match (self, other) {
            (HistoryRow::Single(a), HistoryRow::Single(b)) => {
                a.message == b.message
                    && a.sender == b.sender
                    && a.receipt == b.receipt
                    && a.quote_preview == b.quote_preview
                    && a.forward_from == b.forward_from
                    && a.day_label == b.day_label
                    && a.unread_divider == b.unread_divider
                    && a.run_start == b.run_start
                    && a.sender_avatar == b.sender_avatar
            }
            (
                HistoryRow::Album {
                    messages: a,
                    sender: sa,
                    ..
                },
                HistoryRow::Album {
                    messages: b,
                    sender: sb,
                    ..
                },
            ) => {
                a == b
                    && sa == sb
                    && self.day_label() == other.day_label()
                    && self.unread_divider() == other.unread_divider()
            }
            _ => false,
        }
    }

    pub(crate) fn first_id(&self) -> Option<MessageId> {
        match self {
            HistoryRow::Single(inputs) => Some(inputs.message.id),
            HistoryRow::Album { messages, .. } => messages.first().map(|m| m.id),
        }
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
