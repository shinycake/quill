//! Where a widget is drawn, with two windows showing it at once.

use super::{
    Presence, PresenceChange, Showing, WidgetEntry, next_query_token, prune_hidden, write_pending,
};
use quill::ids::ChatId;
use quill::message_widget::WidgetKey;
use quill::message_widget::storage::Stored;
use std::collections::HashMap;

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

fn key(n: i64) -> WidgetKey {
    WidgetKey {
        chat_id: ChatId(1),
        message_id: quill::ids::MessageId(n),
    }
}

/// A hidden entry holding a frame, gone `since` ticks ago, with storage
/// still to write.
fn heavy(since: u64) -> WidgetEntry {
    let mut entry = WidgetEntry::new(Stored::default(), 100.0);
    let image = std::sync::Arc::new(gpui_kit::RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(image::RgbaImage::new(2, 2)),
    ])));
    entry.frame = Some((image.into(), 10.0, 10.0));
    entry.presence = Presence::Hidden { since };
    entry.unsaved = true;
    entry
}

/// A hidden animated widget keeps the runtime's ask; it ticks only while
/// a window draws it, and again the moment it comes back.
#[test]
fn a_hidden_animated_widget_ticks_only_while_shown() {
    let mut entry = WidgetEntry::new(Stored::default(), 100.0);
    entry.wants_frames = true;
    assert!(!entry.ticks(), "hidden");
    assert_eq!(
        entry.presence.show(MAIN, at(300.0)),
        PresenceChange::CameBack
    );
    assert!(entry.ticks(), "back on screen it ticks again");
    entry.presence.leave(MAIN, 1);
    assert!(!entry.ticks());
    assert!(
        entry.wants_frames,
        "the runtime's ask is kept for the return"
    );
}

/// A pruned widget whose storage cannot be written sheds its runtime and
/// frame at once and keeps only its stored copy; later passes neither
/// write it again nor count it against the pool.
#[test]
fn a_failed_prune_write_sheds_and_is_not_retried() {
    let mut entries: HashMap<WidgetKey, WidgetEntry> = HashMap::new();
    for n in 0..3 {
        entries.insert(key(n), heavy(n as u64 + 1));
    }
    let mut writes = 0;
    for _ in 0..4 {
        prune_hidden(&mut entries, 1, |_, _| {
            writes += 1;
            Err(std::io::Error::other("disk full"))
        });
    }
    assert_eq!(writes, 2, "each excess entry tried once, never again");
    assert_eq!(entries.len(), 3, "nothing unwritten is lost");
    let light = entries.values().filter(|e| !e.holds_weight()).count();
    assert_eq!(light, 2);
    assert!(entries[&key(0)].unsaved && !entries[&key(0)].holds_weight());
    assert!(entries[&key(2)].holds_weight(), "the newest stays warm");
}

/// A prune whose write takes drops the entry.
#[test]
fn a_prune_that_writes_drops_the_oldest() {
    let mut entries: HashMap<WidgetKey, WidgetEntry> = HashMap::new();
    for n in 0..3 {
        entries.insert(key(n), heavy(n as u64 + 1));
    }
    prune_hidden(&mut entries, 2, |_, _| Ok(()));
    assert!(!entries.contains_key(&key(0)));
    assert_eq!(entries.len(), 2);
}
