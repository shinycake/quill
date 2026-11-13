//! Where a widget is drawn, with two windows showing it at once.

use super::{
    Contact, Lander, Live, Presence, PresenceChange, Showing, WidgetEntry, prune_hidden, ticks,
    write_pending,
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
    let mut live = Live::new((), (String::new(), [0; 3]));
    live.wants_frames = true;
    let mut presence = Presence::Hidden { since: 0 };
    assert!(!ticks(&presence, Some(&live)), "hidden");
    assert_eq!(presence.show(MAIN, at(300.0)), PresenceChange::CameBack);
    assert!(
        ticks(&presence, Some(&live)),
        "back on screen it ticks again"
    );
    presence.leave(MAIN, 1);
    assert!(!ticks(&presence, Some(&live)));
    assert!(
        live.wants_frames,
        "the runtime's ask is kept for the return"
    );
    // A widget with no runtime asks for nothing.
    presence.show(MAIN, at(300.0));
    assert!(!ticks::<()>(&presence, None));
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

fn entry() -> WidgetEntry {
    WidgetEntry::new(Stored::default(), 100.0)
}

/// A press whose release cannot arrive (the widget went off screen) must
/// not turn the next moves into a drag.
#[test]
fn a_released_pointer_has_no_press_or_hover_left() {
    let mut live = Live::new((), (String::new(), [0; 3]));
    live.pointer.contact = Contact::Pressed(0);
    live.pointer.hold = true;
    live.pointer.release();
    assert_eq!(
        (live.pointer.contact, live.pointer.hold),
        (Contact::Away, false)
    );
}

/// The selected text outlives a stopped runtime and goes with the content.
#[test]
fn the_selection_stays_until_the_content_changes() {
    let mut entry = entry();
    entry.selection = "kept".into();
    entry.lifecycle = quill::message_widget::Lifecycle::Idle;
    assert_eq!(entry.selection, "kept");
    entry.replace_content(
        super::Identity {
            html: "new".into(),
            is_copy: false,
            can_send_queries: false,
            can_send_background_queries: false,
        },
        50.0,
    );
    assert!(entry.selection.is_empty());
}

/// A bot edits the message to new HTML: nothing of the old content's
/// picture, height or press stays on the entry.
#[test]
fn new_content_replaces_what_was_bound_to_the_old() {
    let identity = |html: &str| super::Identity {
        html: html.into(),
        is_copy: false,
        can_send_queries: false,
        can_send_background_queries: false,
    };
    let mut entry = entry();
    entry.identity = Some(identity("old"));
    entry.lifecycle = quill::message_widget::Lifecycle::Stopped {
        reason: "cpu".into(),
        streak: 2,
    };
    entry.polled = Some("{}".into());
    entry.poll = super::Poll::Asking {
        request: quill::ids::RequestId(1),
        asked: std::time::Instant::now(),
    };
    entry.replace_content(identity("new"), 150.0);
    assert_eq!(
        entry.identity.as_ref().map(|i| i.html.as_str()),
        Some("new")
    );
    assert!(entry.frame.is_none() && entry.polled.is_none());
    // The runtime, what it asked for and the stops in a row went whole.
    assert_eq!(entry.height, 150.0);
    assert!(matches!(
        entry.lifecycle,
        quill::message_widget::Lifecycle::Idle
    ));
    assert_eq!(entry.lifecycle.streak(), 0);
    assert_eq!(entry.poll, super::Poll::Off);
}

/// A lander loop that ended (its screen's updates stopped) is not a running
/// one: the next runtime start runs it again.
#[test]
fn a_finished_lander_is_not_running() {
    assert!(!Lander::NotStarted.running());
    assert!(!Lander::Running(gpui_kit::Task::ready(())).running());
}
