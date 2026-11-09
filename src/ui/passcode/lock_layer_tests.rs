//! The lock is a render state: while locked, the main window draws only
//! the lock screen. No kit dialog is drawn, one opened before the lock
//! or after it, and none takes a key; an armed key capture is disarmed
//! and the keys typed reach only the passcode field.

use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Focusable, IntoElement, ParentElement, TestAppContext, VisualTestContext,
    px, size,
};
use quill::ids::ChatId;

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;

const CHAT: ChatId = ChatId(11);

fn open_chat(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("demo app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.select_listed_chat(CHAT, window, cx);
    });
    frame(&mut vcx);
    (app, vcx)
}

fn frame(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
}

fn lock(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| {
        app.account.passcode.enabled = true;
        app.lock_by_passcode(cx);
    });
    frame(vcx);
}

fn unlock(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| app.set_locked(false, cx));
    frame(vcx);
}

/// A box opened with `open_alert_dialog` (Clear History, Delete): the
/// shell tracks no flag for it.
fn open_alert(vcx: &mut VisualTestContext) {
    vcx.update(|window, cx| {
        window.open_alert_dialog(cx, |alert, _, _| {
            alert.content(|content, _, _| {
                content.child(Button::new("lock-test-box").label("OK").into_any_element())
            })
        });
    });
    frame(vcx);
}

fn drawn(vcx: &mut VisualTestContext, id: &'static str) -> bool {
    vcx.update(|window, _| window.try_find(id).is_some())
}

/// Finding 1: a box open when the lock comes is not drawn under it.
#[gpui_kit::test]
fn a_box_open_at_the_lock_is_not_drawn_until_the_unlock(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_chat(cx);
    open_alert(&mut vcx);
    assert!(drawn(&mut vcx, "lock-test-box"), "the box shows");
    lock(&app, &mut vcx);
    assert!(drawn(&mut vcx, "lock-submit"), "the lock screen shows");
    assert!(!drawn(&mut vcx, "lock-test-box"), "the box is not drawn");
    unlock(&app, &mut vcx);
    assert!(
        drawn(&mut vcx, "lock-test-box"),
        "it is back after the unlock"
    );
}

/// A box opened while locked (a late answer, a deferred open) is not
/// drawn either: no surface can bypass the lock.
#[gpui_kit::test]
fn a_box_opened_while_locked_is_not_drawn(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_chat(cx);
    lock(&app, &mut vcx);
    open_alert(&mut vcx);
    assert!(drawn(&mut vcx, "lock-submit"));
    assert!(
        !drawn(&mut vcx, "lock-test-box"),
        "the new box is not drawn"
    );
    unlock(&app, &mut vcx);
    assert!(drawn(&mut vcx, "lock-test-box"));
}

/// Finding 5: the main window builds the lock screen alone, so none of
/// the chat's elements (the composer, its deferred overlays) is drawn.
#[gpui_kit::test]
fn the_main_window_draws_only_the_lock_screen(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_chat(cx);
    assert!(drawn(&mut vcx, "composer-send"), "the chat shows");
    lock(&app, &mut vcx);
    assert!(drawn(&mut vcx, "lock-submit"));
    assert!(!drawn(&mut vcx, "composer-send"), "the chat is not drawn");
    unlock(&app, &mut vcx);
    assert!(drawn(&mut vcx, "composer-send"));
}

/// Finding 4: an armed push-to-talk or shortcut capture does not take the
/// passcode's first key.
#[gpui_kit::test]
fn a_key_capture_does_not_take_the_passcodes_first_key(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_chat(cx);
    let ptt_key = |app: &QuillApp| {
        app.session()
            .map(|s| s.calls.prefs.push_to_talk.key.clone())
            .unwrap_or_default()
    };
    let before = app.read_with(&vcx, |app, _| ptt_key(app));
    app.update(&mut vcx, |app, _| {
        app.group_call.ptt_capture = true;
        app.preferences.keybinding_capture = Some("toggle_sidebar".into());
    });
    lock(&app, &mut vcx);
    app.read_with(&vcx, |app, _| {
        assert!(!app.group_call.ptt_capture, "the PTT capture is off");
        assert!(
            app.preferences.keybinding_capture.is_none(),
            "no binding is armed"
        );
    });
    // Armed again under the lock, a key still goes to the lock screen only.
    app.update(&mut vcx, |app, _| app.group_call.ptt_capture = true);
    vcx.simulate_keystrokes("q");
    frame(&mut vcx);
    app.read_with(&vcx, |app, _| {
        assert_eq!(ptt_key(app), before, "no key was bound");
    });
}
