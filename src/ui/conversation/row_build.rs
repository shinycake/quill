//! Building history rows from messages: the whole list, or a run of rows
//! between kept neighbours (`row_splice`). A row's place among its
//! neighbours (sender run, day separator, unread divider) comes from the
//! rows above it, handed down as a `RowSeed`, and from the sender of the
//! row below it.
#![deny(clippy::indexing_slicing)]

use super::*;
use crate::ui::history_row::{ChromeSpot, HistoryRow, HistoryRowInputs};

/// Who a row is from: (outgoing, sender) of its first message.
pub(super) type Identity = (bool, Option<MessageSender>);

pub(super) fn identity(row: &HistoryRow) -> Option<Identity> {
    row.first_message()
        .map(|message| (message.is_outgoing, message.sender))
}

fn day_of(date: i32) -> i64 {
    quill::local_time::civil_local(i64::from(date)).day_number()
}

/// What the rows above hand down to the next one.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct RowSeed {
    /// The sender of the row just above.
    pub(super) previous: Option<Identity>,
    /// The local day of the nearest row above that has a date.
    pub(super) previous_day: Option<i64>,
    /// The "Unread messages" divider sits above.
    pub(super) divider_placed: bool,
    /// Rows above.
    pub(super) index: usize,
}

impl RowSeed {
    /// What `rows` above `ix` hand down.
    pub(super) fn before(rows: &[HistoryRow], ix: usize) -> Self {
        let above = rows.get(..ix).unwrap_or(rows);
        RowSeed {
            previous: above.last().and_then(identity),
            previous_day: above
                .iter()
                .rev()
                .filter_map(HistoryRow::first_message)
                .find(|message| message.date > 0)
                .map(|message| day_of(message.date)),
            divider_placed: above.iter().any(HistoryRow::unread_divider),
            index: above.len(),
        }
    }

    /// Hands the rows below the same as `other` (the row count aside).
    pub(super) fn carries_like(&self, other: &Self) -> bool {
        self.previous == other.previous
            && self.previous_day == other.previous_day
            && self.divider_placed == other.divider_placed
    }
}

/// The parts of a render every built row reads.
pub(super) struct RowBuild<'a> {
    pub(super) chrome: &'a super::row_chrome::ChromeInputs<'a>,
    /// Chrome generation the new rows are stamped with.
    pub(super) epoch: u64,
    pub(super) highlight: Option<MessageId>,
    pub(super) unread_anchor: Option<MessageId>,
    pub(super) is_secret: bool,
}

impl QuillApp {
    /// Rows for `messages` (oldest first) under rows that hand down `seed`
    /// and above a row from `below` (`None`: the end of the list). Returns
    /// the rows and what they hand down.
    pub(super) fn build_rows(
        &self,
        messages: Vec<HistoryMessage>,
        seed: RowSeed,
        below: Option<Identity>,
        build: &RowBuild,
    ) -> (Vec<HistoryRow>, RowSeed) {
        let RowSeed {
            mut previous,
            mut previous_day,
            mut divider_placed,
            index: rows_above,
        } = seed;
        let mut first_unread = |message: &HistoryMessage| {
            let first = !divider_placed
                && !message.is_outgoing
                && build
                    .unread_anchor
                    .is_some_and(|anchor| message.id.0 > anchor.0);
            divider_placed |= first;
            first
        };
        let groups = quill::album::group_media_albums(
            &messages,
            |message| message.media_album_id,
            |message| message.is_outgoing,
            |message| quill::album::album_kind(&message.content),
        );
        let mut rows: Vec<HistoryRow> = Vec::with_capacity(groups.len());
        // Sender of each row (first item for albums): a run is consecutive
        // rows from one sender. The name heads a run; the avatar sits
        // beside its last row, with a spacer keeping earlier rows aligned.
        // The row below the built ones ends the list of senders.
        let identities: Vec<Identity> = groups
            .iter()
            .map(|group| match group {
                quill::album::HistoryGroup::Album { messages, .. } => messages
                    .first()
                    .map(|m| (m.is_outgoing, m.sender))
                    .unwrap_or((false, None)),
                quill::album::HistoryGroup::Single(message) => {
                    (message.is_outgoing, message.sender)
                }
            })
            .chain(below)
            .collect();
        // Each group's span over `messages` (album id, item count): the
        // rows below take the messages by value instead of cloning them.
        let spans: Vec<(Option<i64>, usize)> = groups
            .iter()
            .map(|group| match group {
                quill::album::HistoryGroup::Album { album_id, messages } => {
                    (Some(*album_id), messages.len())
                }
                quill::album::HistoryGroup::Single(_) => (None, 1),
            })
            .collect();
        drop(groups);
        let mut owned = messages.into_iter();
        let mut spot_of = |message: &HistoryMessage, next_same_sender: bool, with_headers: bool| {
            let identity = (message.is_outgoing, message.sender);
            let sender_changed = previous != Some(identity);
            previous = Some(identity);
            ChromeSpot {
                sender_changed,
                next_same_sender,
                with_headers,
            }
        };
        let now = quill::local_time::civil_local(quill::local_time::now_unix());
        let mut day_label = |date: i32| {
            if date <= 0 {
                return None;
            }
            let civil = quill::local_time::civil_local(i64::from(date));
            let day = civil.day_number();
            (previous_day.replace(day) != Some(day))
                .then(|| quill::local_time::day_label(&civil, &now))
        };
        for (index, (album, len)) in spans.into_iter().enumerate() {
            let next_same_sender = identities
                .get(index + 1)
                .is_some_and(|next| Some(next) == identities.get(index));
            match album {
                Some(album_id) => {
                    let album_messages: Vec<HistoryMessage> = owned.by_ref().take(len).collect();
                    let Some(first) = album_messages.first() else {
                        continue;
                    };
                    // Same chrome rule as single rows, from the first item.
                    let spot = spot_of(first, next_same_sender, false);
                    let chrome = Box::new(build.chrome.chrome(first, spot));
                    let day_label = day_label(first.date);
                    let unread_divider = first_unread(first);
                    rows.push(HistoryRow::Album {
                        album_id,
                        messages: album_messages,
                        chrome,
                        chrome_stamp: build.epoch,
                        spot,
                        day_label,
                        unread_divider,
                    })
                }
                None => {
                    let Some(message) = owned.next() else {
                        break;
                    };
                    let spot = spot_of(&message, next_same_sender, true);
                    let chrome = build.chrome.chrome(&message, spot);
                    let row_day_label = day_label(message.date);
                    let row_unread_divider = first_unread(&message);
                    rows.push(HistoryRow::Single(Box::new(HistoryRowInputs {
                        chrome,
                        chrome_stamp: build.epoch,
                        spot,
                        highlighted: build.highlight == Some(message.id),
                        run_start: spot.sender_changed && rows_above + index > 0,
                        joined_above: !spot.sender_changed
                            && row_day_label.is_none()
                            && !row_unread_divider,
                        day_label: row_day_label,
                        unread_divider: row_unread_divider,
                        fact_check_expanded: self
                            .message_ui
                            .fact_check_expanded
                            .contains(&(message.chat_id.0, message.id.0)),
                        is_secret: build.is_secret,
                        vanishing: self.vanish_progress(message.id).is_some(),
                        message,
                    })));
                }
            }
        }
        let seed = RowSeed {
            previous,
            previous_day,
            divider_placed,
            index: rows_above + rows.len(),
        };
        (rows, seed)
    }

    /// A bot's streaming reply (`updatePendingMessage`) is its next
    /// incoming message, growing as it arrives (Telegram Desktop): the
    /// row after the rows that hand down `seed`.
    pub(super) fn streaming_bot_row(
        &self,
        seed: &RowSeed,
        build: &RowBuild,
        cx: &mut Context<Self>,
    ) -> Option<HistoryRow> {
        let message = self.streaming_bot_message(cx)?;
        let spot = ChromeSpot {
            sender_changed: seed.previous != Some((message.is_outgoing, message.sender)),
            next_same_sender: true,
            with_headers: false,
        };
        let chrome = build.chrome.chrome(&message, spot);
        Some(HistoryRow::Single(Box::new(HistoryRowInputs {
            chrome,
            chrome_stamp: build.epoch,
            spot,
            highlighted: false,
            run_start: spot.sender_changed,
            joined_above: !spot.sender_changed,
            day_label: None,
            unread_divider: false,
            fact_check_expanded: false,
            is_secret: false,
            vanishing: false,
            message,
        })))
    }
}
