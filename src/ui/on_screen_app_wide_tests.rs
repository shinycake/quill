//! What the app-wide structs used to hold for the account on screen goes
//! with the screen: the Account dialog, the call ids of the call window and
//! its sounds, the video chat's boxes and pending join, the screen share
//! listing, and the status line a restart set.

use crate::ui::app::QuillApp;
use crate::ui::group_call_stream_layout::StreamJoinIntent;
use crate::ui::on_screen::OnScreen;
use crate::ui::screen_chooser_demo::DemoScreens;
use crate::ui::screen_share_state::ScreenFor;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::status_note::StatusNote;
use gpui_kit::component::{Root, WindowExt};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::calls::capture::screen::ScreenAccess;
use quill::ids::AccountKey;
use quill::state::ActiveGroupCall;
use std::sync::Arc;

fn new_app(cx: &mut TestAppContext, demo: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(demo))));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn switch(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.switch_account(&AccountKey("other".into()), "", window, cx);
    });
}

fn dialog_up(vcx: &mut VisualTestContext) -> bool {
    vcx.update(|window, cx| window.has_active_dialog(cx))
}

/// The structs moved off the app are fields of the screen.
#[test]
fn the_account_bound_parts_are_screen_fields() {
    for field in ["account_boxes", "call_screen", "group_call_screen"] {
        assert!(OnScreen::FIELD_NAMES.contains(&field), "{field}");
    }
}

/// Delete Account armed, with the password typed, does not come back over
/// the next account: its final button would delete that account.
#[gpui_kit::test]
fn the_account_dialog_does_not_reopen_over_the_next_account(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    frames(&mut vcx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_account_lifecycle(cx);
        app.account_boxes.lifecycle.confirm_delete = true;
        app.account_boxes
            .lifecycle
            .password
            .update(cx, |input, cx| input.set_value("hunter2", window, cx));
    });
    frames(&mut vcx);
    assert!(dialog_up(&mut vcx), "the Account dialog is up");

    switch(&app, &mut vcx);
    frames(&mut vcx);
    assert!(!dialog_up(&mut vcx), "it did not come back");
    app.read_with(&vcx, |app, cx| {
        let lifecycle = &app.account_boxes.lifecycle;
        assert!(!lifecycle.open && !lifecycle.confirm_delete && !lifecycle.confirm_logout);
        assert_eq!(lifecycle.password.read(cx).value().to_string(), "");
        assert!(QuillShell::open_dialog_names(app).is_empty());
    });
}

/// Call ids repeat across clients: the call whose window was closed, the
/// end time and the sound marks of one account do not hold the next
/// account's call of the same id; neither do the video chat's boxes, its
/// composer text and the stream join it waits for.
#[gpui_kit::test]
fn call_ids_and_video_chat_boxes_do_not_carry_over(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    frames(&mut vcx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.call_screen.window_closed_by_user = Some(1);
        app.call_screen.ended_at = Some((1, std::time::Instant::now()));
        app.group_call_screen.window_closed_by_user = Some(1);
        app.group_call_screen.settings_open = true;
        app.group_call_screen.stream_join = Some((StreamJoinIntent::Call(1), None));
        app.group_call_screen.stream_joining = Some(1);
        app.group_call_screen
            .composer
            .update(cx, |input, cx| input.set_value("for A's chat", window, cx));
    });
    switch(&app, &mut vcx);
    frames(&mut vcx);
    app.read_with(&vcx, |app, cx| {
        assert_eq!(app.call_screen.window_closed_by_user, None);
        assert!(app.call_screen.ended_at.is_none());
        let group = &app.group_call_screen;
        assert_eq!(group.window_closed_by_user, None);
        assert!(!group.settings_open);
        assert!(group.stream_join.is_none(), "no join waits for B's call 1");
        assert_eq!(group.stream_joining, None, "no Join button waits for it");
        assert_eq!(group.composer.read(cx).value().to_string(), "");
    });
}

/// A switch while the share chooser lists its sources drops the listing's
/// continuation. Share Screen still works afterwards: the listing is the
/// screen's, not a flag only that continuation would clear.
#[gpui_kit::test]
fn share_screen_works_after_a_switch_during_the_listing(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-privacy");
    let call = || ActiveGroupCall {
        is_joined: true,
        ..ActiveGroupCall::fresh(7)
    };
    app.update_in(&mut vcx, |app, _, cx| {
        app.screen_share.backend = Some(Arc::new(DemoScreens {
            access: ScreenAccess::Granted,
            system_picker: false,
        }));
        if let Some(session) = app.demo_session.as_mut() {
            session.calls.active_group_call = Some(call());
        }
        assert!(app.open_screen_chooser(ScreenFor::Group, cx));
    });
    // Switched before the listing comes back.
    switch(&app, &mut vcx);
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| assert!(app.screen_share.window.is_none()));
    app.update_in(&mut vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.calls.active_group_call = Some(call());
        }
        assert!(app.open_screen_chooser(ScreenFor::Group, cx));
    });
    vcx.run_until_parked();
    app.read_with(&vcx, |app, _| {
        assert!(app.screen_share.window.is_some(), "the chooser opened");
    });
}

/// A client that leaves with no window at hand (a restart) has its screen
/// replaced at the end of the update; the "Connecting" note set meanwhile
/// stays on the status line.
#[gpui_kit::test]
fn a_deferred_replacement_keeps_the_status_note(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    frames(&mut vcx);
    let before = app.read_with(&vcx, |app, _| app.screen_epoch());
    app.update(&mut vcx, |app, cx| {
        app.screen_leaves_later(cx);
        assert!(app.goodbyes_due());
        app.set_note(StatusNote::silent("Connecting to Telegram…"));
    });
    frames(&mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(!app.goodbyes_due(), "the goodbyes ran");
        assert_ne!(app.screen_epoch(), before);
        assert_eq!(app.connection.note().text(), "Connecting to Telegram…");
    });
}
