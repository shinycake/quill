//! What the reader saw: the history rows whose bottom edge was on screen
//! at the last paint, reported as `viewMessages`.

use gpui_kit::{Bounds, Pixels};
use quill::ids::{ChatId, MessageId};

/// Turn the rows painted this frame into the messages the reader saw: a
/// row counts once its bottom edge is inside the viewport (tdesktop
/// `HistoryInner::paintEvent` reads an unread message when `top + height`
/// is shown), so overdraw rows above or below the view are not read.
/// `changed` runs when the seen set differs from the last frame's.
pub(super) fn note_seen(
    painted: &std::cell::RefCell<Vec<(Bounds<Pixels>, Vec<MessageId>)>>,
    seen: &std::cell::RefCell<Option<(ChatId, Vec<MessageId>)>>,
    chat: Option<ChatId>,
    viewport: Bounds<Pixels>,
    changed: impl FnOnce(),
) {
    let Ok(mut painted) = painted.try_borrow_mut() else {
        return;
    };
    let mut ids: Vec<MessageId> = painted
        .drain(..)
        .filter(|(row, _)| row.bottom() >= viewport.top() && row.bottom() <= viewport.bottom())
        .flat_map(|(_, ids)| ids)
        .collect();
    drop(painted);
    ids.sort_unstable_by_key(|id| id.0);
    ids.dedup();
    let next = chat.map(|chat| (chat, ids));
    let Ok(mut seen) = seen.try_borrow_mut() else {
        return;
    };
    if *seen != next {
        *seen = next;
        drop(seen);
        changed();
    }
}

#[cfg(test)]
#[cfg(feature = "demo-capture")]
#[path = "history_reads_tests.rs"]
mod tests;
