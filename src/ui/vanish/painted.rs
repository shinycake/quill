//! Where each history row and its bubble were last painted: a delete
//! starts its dust there.
#![deny(clippy::indexing_slicing)]

use gpui_kit::*;
use quill::ids::ChatMessage;
use std::cell::RefCell;
use std::collections::HashMap;

/// A row's bounds and its bubble's, in window coordinates.
pub(super) type Painted = (Bounds<Pixels>, Option<Bounds<Pixels>>);

/// A window's draw: the frame a row was painted in.
type Stamp = (WindowId, u64);

thread_local! {
    /// Where each history row and its bubble were last painted, by
    /// chat and message id, and in which frame: a delete starts its dust
    /// there.
    static PAINTED: RefCell<HashMap<ChatMessage, (Painted, Stamp)>> =
        RefCell::new(HashMap::new());
    /// The latest frame of each window that painted a row. A row painted
    /// in an older frame is not on screen now (scrolled away, or another
    /// chat), so its bounds are not where the dust belongs.
    static LATEST: RefCell<HashMap<WindowId, u64>> = RefCell::new(HashMap::new());
}

fn record(id: ChatMessage, window: &Window, update: impl FnOnce(&mut Painted)) {
    let stamp = (window.window_handle().window_id(), window.frame_number());
    LATEST.with(|latest| {
        if let Ok(mut latest) = latest.try_borrow_mut() {
            let frame = latest.entry(stamp.0).or_insert(stamp.1);
            *frame = (*frame).max(stamp.1);
        }
    });
    PAINTED.with(|painted| {
        // A conflict skips this frame's record; the next paint writes it.
        let Ok(mut painted) = painted.try_borrow_mut() else {
            return;
        };
        if painted.len() > 4096 {
            painted.clear();
        }
        let entry = painted
            .entry(id)
            .or_insert(((Bounds::default(), None), stamp));
        if entry.1 != stamp {
            // The other half of the last frame's record is out of date.
            *entry = ((Bounds::default(), None), stamp);
        }
        update(&mut entry.0);
    });
}

/// Where a message's row and bubble were painted, when that was in the
/// latest frame of its window: otherwise the message is not on screen.
pub(super) fn painted_now(id: ChatMessage) -> Option<Painted> {
    let (bounds, (window, frame)) =
        PAINTED.with(|painted| painted.try_borrow().ok()?.get(&id).copied())?;
    let latest = LATEST.with(|latest| latest.try_borrow().ok()?.get(&window).copied());
    (latest == Some(frame)).then_some(bounds)
}

/// The window `closed` went: its frame counter and the rows painted in it
/// are of no use any more (one entry per window ever shown would stay).
pub(in crate::ui) fn window_closed(closed: WindowId) {
    LATEST.with(|latest| {
        if let Ok(mut latest) = latest.try_borrow_mut() {
            latest.remove(&closed);
        }
    });
    PAINTED.with(|painted| {
        if let Ok(mut painted) = painted.try_borrow_mut() {
            painted.retain(|_, (_, (window, _))| *window != closed);
        }
    });
}

/// Where a message's row and bubble were last painted.
#[cfg(test)]
#[cfg(feature = "demo-capture")]
pub(in crate::ui) fn painted(id: ChatMessage) -> Option<Painted> {
    PAINTED.with(|painted| painted.borrow().get(&id).map(|(bounds, _)| *bounds))
}

/// An invisible child that notes where a history row is painted.
pub(in crate::ui) fn row_tracker(id: ChatMessage) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            record(id, window, |entry| entry.0 = bounds);
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}

/// An invisible child that notes where a message's bubble is painted.
pub(in crate::ui) fn bubble_tracker(id: ChatMessage) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            record(id, window, |entry| entry.1 = Some(bounds));
        },
    )
    .absolute()
    .inset_0()
    .size_full()
}
