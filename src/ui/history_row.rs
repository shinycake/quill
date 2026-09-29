use super::*;

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
    pub(crate) sender: Option<String>,
    /// kit Phase 4: outbox delivery state for the in-bubble footer
    /// (`✓` sent, `✓✓` read, `…` while pending).
    pub(crate) receipt: OutboxReceipt,
    /// kit Phase 4: `(name, photo)` for the kit `Message` avatar slot.
    /// `None` where the sender can't be identified (groups) or the row is
    /// outgoing — never invented.
    pub(crate) sender_avatar: Option<(String, Option<PathBuf>)>,
    pub(crate) highlighted: bool,
    pub(crate) selected_forward: bool,
    pub(crate) quote_preview: Option<String>,
    pub(crate) forward_from: Option<String>,
    pub(crate) reaction_open: bool,
    pub(crate) seek_bar: Option<SeekBarView>,
    pub(crate) animation_playing: bool,
    pub(crate) animation_frame: Option<PathBuf>,
    pub(crate) video_playing: bool,
    pub(crate) video_frame: Option<PathBuf>,
    pub(crate) is_secret: bool,
}

/// kit Phase 3: one virtualized history row — a single message or a media
/// album group (albums render as one row, as before).
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
        sender: Option<String>,
        receipt: OutboxReceipt,
        sender_avatar: Option<(String, Option<PathBuf>)>,
    },
}

impl HistoryRow {
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

    pub(crate) fn contains(&self, id: MessageId) -> bool {
        match self {
            HistoryRow::Single(inputs) => inputs.message.id == id,
            HistoryRow::Album { messages, .. } => messages.iter().any(|m| m.id == id),
        }
    }
}
