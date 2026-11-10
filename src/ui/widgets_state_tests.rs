//! Where a widget is drawn, with two windows showing it at once.

use super::{Presence, PresenceChange, Showing, next_query_token, write_pending};
use quill::ids::ChatId;

const MAIN: Option<ChatId> = None;
const OTHER: Option<ChatId> = Some(ChatId(7));

fn at(width: f32) -> Showing {
    Showing { width, view: None }
}

/// Two windows draw the same widget at different widths, frame after
/// frame: only the owner's width reaches the runtime, so it never flips
/// between them (round 18 B #1).
#[test]
fn a_second_window_never_moves_the_owners_width() {
    let mut presence = Presence::Hidden { since: 0 };
    assert_eq!(presence.show(MAIN, at(400.0)), PresenceChange::CameBack);
    for _ in 0..3 {
        assert_eq!(presence.show(OTHER, at(260.0)), PresenceChange::Same);
        assert_eq!(presence.show(MAIN, at(400.0)), PresenceChange::Same);
    }
    assert_eq!(presence.width(), 400.0);
    assert_eq!(presence.owner(), Some(MAIN));
    assert!(presence.drawn_in(OTHER));
    // The owner's own resize does move it.
    assert_eq!(presence.show(MAIN, at(380.0)), PresenceChange::Width(380.0));
}

/// The owner stops drawing it: the other window owns it at its width; the
/// last window leaving hides it.
#[test]
fn the_owner_leaving_hands_the_widget_on() {
    let mut presence = Presence::Hidden { since: 0 };
    presence.show(MAIN, at(400.0));
    presence.show(OTHER, at(260.0));
    assert_eq!(presence.leave(MAIN, 5), PresenceChange::Width(260.0));
    assert_eq!(presence.owner(), Some(OTHER));
    assert!(!presence.drawn_in(MAIN));
    assert_eq!(presence.leave(MAIN, 6), PresenceChange::Same);
    assert_eq!(presence.leave(OTHER, 7), PresenceChange::Hid);
    assert_eq!(presence, Presence::Hidden { since: 7 });
    assert_eq!(presence.hidden_since(), 7);
}

/// A window that is not the owner leaving changes nothing for the runtime.
#[test]
fn another_window_leaving_keeps_the_owner() {
    let mut presence = Presence::Hidden { since: 0 };
    presence.show(MAIN, at(400.0));
    presence.show(OTHER, at(260.0));
    assert_eq!(presence.leave(OTHER, 3), PresenceChange::Same);
    assert_eq!(presence.owner(), Some(MAIN));
    assert_eq!(presence.views().len(), 1);
}

/// A narrower window draws the owner's frame scaled down; a wider one at
/// its own size.
#[test]
fn a_narrower_window_scales_the_frame_into_its_card() {
    use crate::ui::widget_bubble::frame_scale;
    assert_eq!(frame_scale(400.0, 200.0), 0.5);
    assert_eq!(frame_scale(400.0, 400.0), 1.0);
    assert_eq!(frame_scale(400.0, 600.0), 1.0);
    assert_eq!(frame_scale(0.0, 200.0), 1.0);
}

/// Widget query tokens never repeat, whatever screen asks.
#[test]
fn query_tokens_are_unique_for_the_run() {
    let first = next_query_token();
    let second = next_query_token();
    assert_ne!(first, second);
    assert!(second > first);
}

/// A widget's storage that could not be written stays pending, so the
/// farewell, the next timer or the prune writes it again; only a write
/// that took makes it clean (round 18 B #55).
#[test]
fn a_failed_storage_write_stays_pending_until_one_takes() {
    let mut unsaved = true;
    write_pending(&mut unsaved, || {
        Err(std::io::Error::other("no space left on device"))
    });
    assert!(unsaved, "the failed write left it pending");
    write_pending(&mut unsaved, || Ok(()));
    assert!(!unsaved, "the write that took made it clean");
    // A clean entry writes nothing.
    write_pending(&mut unsaved, || panic!("nothing to write"));
}
