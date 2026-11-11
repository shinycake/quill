//! Chat window upkeep: reopening a closed window, the compose field an
//! activation focuses, and the places saved while a window is leased.
#![cfg(feature = "demo-capture")]

use super::saved::SaveTimer;
use super::tests::{Harness, OPEN, OTHER, new_app};
use gpui_kit::{AppContext, Bounds, Task, TestAppContext, point, px, size};
use quill::ids::ChatId;

fn bounds() -> Bounds<gpui_kit::Pixels> {
    Bounds {
        origin: point(px(50.), px(60.)),
        size: size(px(500.), px(400.)),
    }
}

fn reopen(h: &Harness, cx: &mut TestAppContext) -> bool {
    let app = h.app.clone();
    let opened = cx
        .update_window(h.main, |_, window, cx| {
            app.update(cx, |app, cx| app.reopen_last_closed_chat_window(window, cx))
        })
        .expect("the main window");
    cx.run_until_parked();
    opened
}

#[gpui_kit::test]
fn a_reopen_the_lock_refuses_keeps_the_closed_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    h.app.update(cx, |app, cx| {
        app.windows.note_closed(OTHER, bounds());
        app.account.passcode.enabled = true;
        app.lock_by_passcode(cx);
    });
    assert!(!reopen(&h, cx), "the lock refuses the window");
    assert_eq!(h.app.read_with(cx, |app, _| app.windows.closed.len()), 1);
    assert!(h.app.read_with(cx, |app, _| !app.windows.in_window()));
}

#[gpui_kit::test]
fn a_reopen_brings_the_window_back_and_a_vanished_chat_is_dropped(cx: &mut TestAppContext) {
    let h = new_app(cx);
    h.app.update(cx, |app, _| {
        app.windows.note_closed(OTHER, bounds());
        // Newer, but its chat is not in the account any more.
        app.windows.note_closed(ChatId(9_999), bounds());
    });
    assert!(reopen(&h, cx), "the older window opened");
    assert!(h.app.read_with(cx, |app, _| app.windows.closed.is_empty()));
    assert!(h.app.read_with(cx, |app, _| app.windows.has_window(OTHER)));
}

#[gpui_kit::test]
fn activating_the_window_in_place_focuses_its_own_compose_field(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let window = h.open_in_new_window(OTHER, cx);
    let app = h.app.clone();
    // The chat window's state is in place, as inside its own input.
    let composer = app
        .update(cx, |app, cx| {
            app.in_window_scope(Some(OTHER), cx, |app, cx| {
                let composer = app.composer.clone();
                app.windows.activate(OTHER, &composer, cx);
                composer
            })
        })
        .expect("the window is open");
    cx.run_until_parked();
    let focused = cx
        .update_window(window, |_, window, cx| {
            gpui_kit::Focusable::focus_handle(&composer, cx).is_focused(window)
        })
        .expect("the chat window");
    assert!(focused, "the window's own field has the keyboard");
    assert_ne!(OPEN, OTHER);
}

#[gpui_kit::test]
fn places_are_saved_from_what_the_windows_recorded(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let window = h.open_in_new_window(OTHER, cx);
    // Read while the window is leased: the slot holds what it last drew.
    let places = cx
        .update_window(window, |_, _, cx| {
            h.app
                .read(cx)
                .windows
                .open_places()
                .map(|(chat, _)| chat)
                .collect::<Vec<_>>()
        })
        .expect("the chat window");
    assert_eq!(places, vec![OTHER]);
}

#[gpui_kit::test]
fn the_save_timer_is_due_only_on_the_screen_it_was_started_on(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let old = h.app.read_with(cx, |app, _| app.screen_epoch());
    let app = h.app.clone();
    cx.update_window(h.main, |_, window, cx| {
        app.update(cx, |app, cx| {
            assert!(matches!(app.windows.save, SaveTimer::Idle));
            // A timer started on the old screen that never fired.
            app.windows.save = SaveTimer::Due(old, Task::ready(()));
            app.end_account_surfaces(window, cx);
            // The screen was not replaced under it (no main window to
            // replace it): the stale timer is still in place.
            app.windows.save = SaveTimer::Due(old, Task::ready(()));
        });
    })
    .expect("the main window");
    let (due_old, due_now) = h.app.read_with(cx, |app, _| {
        (
            app.windows.save.due_on(old),
            app.windows.save.due_on(app.screen_epoch()),
        )
    });
    assert!(due_old);
    assert!(!due_now, "a new screen does not wait on the old timer");
}
