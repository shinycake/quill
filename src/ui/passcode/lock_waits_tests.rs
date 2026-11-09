//! What runs while locked (docs/decisions/lock-is-a-render-state.md): a
//! late answer's open waits for the unlock and then runs, the call windows
//! keep the call but lose their boxes, a locked window is not "looked at",
//! a lock asked during a passcode job lands after it, and nothing connects
//! on the cleared key.

use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::platform::{os_launch_attempts, os_opened_last};

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;

const CHAT: ChatId = ChatId(11);
const URL: &str = "https://example.com/after-the-lock";

fn new_app(cx: &mut TestAppContext, demo: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(demo))));
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("demo app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
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
    assert!(app.read_with(vcx, |app, _| app.account.passcode.is_locked()));
}

/// The unlock a right passcode gives (`unlock_answered`).
fn unlock(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.unlock_answered(Ok(()), window, cx)
    });
    frame(vcx);
}

#[gpui_kit::test]
fn a_link_a_late_answer_opens_under_the_lock_opens_after_the_unlock(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    lock(&app, vcx);
    let before = os_launch_attempts();
    app.update(vcx, |app, cx| app.open_link(URL, cx));
    frame(vcx);
    assert_eq!(os_launch_attempts(), before, "nothing opens over the lock");
    assert_eq!(
        app.read_with(vcx, |app, _| app.account.passcode.unlock_intents.len()),
        1,
        "the open waits, it is not dropped"
    );
    unlock(&app, vcx);
    assert_eq!(os_launch_attempts(), before + 1);
    assert_eq!(os_opened_last().as_deref(), Some(URL));
    assert_eq!(
        app.read_with(vcx, |app, _| app.account.passcode.unlock_intents.len()),
        0
    );
}

/// The Grams withdrawal: its page comes from a late answer, through the
/// frame upkeep, which waits under the lock with the URL kept.
#[gpui_kit::test]
fn a_withdrawal_page_that_lands_under_the_lock_opens_after_the_unlock(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-grams");
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| app.open_grams(cx));
    frame(vcx);
    lock(&app, vcx);
    let url = "https://fragment.com/withdraw?token=2";
    app.update(vcx, |app, cx| {
        if let Some(grams) = app.demo_session.as_mut().map(|s| &mut s.payments.hub.grams) {
            grams.withdraw_url = Some(url.to_string());
        }
        cx.notify();
    });
    frame(vcx);
    frame(vcx);
    assert_ne!(os_opened_last().as_deref(), Some(url), "not under the lock");
    let kept = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.payments.hub.grams.withdraw_url.clone())
    });
    assert_eq!(
        kept.as_deref(),
        Some(url),
        "the answer waits where it landed"
    );
    unlock(&app, vcx);
    frame(vcx);
    assert_eq!(os_opened_last().as_deref(), Some(url));
}

#[gpui_kit::test]
fn the_lock_screen_log_out_drops_what_waited_for_the_unlock(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    lock(&app, vcx);
    let before = os_launch_attempts();
    app.update(vcx, |app, cx| app.open_link(URL, cx));
    // The demo has no data on disk: its log out only lifts the lock.
    app.update_in(vcx, |app, window, cx| {
        app.account.passcode.demo = true;
        app.confirm_lock_logout_in(None, window, cx);
        app.account.passcode.demo = false;
    });
    frame(vcx);
    assert!(!app.read_with(vcx, |app, _| app.account.passcode.is_locked()));
    assert_eq!(
        os_launch_attempts(),
        before,
        "the wiped accounts' open goes"
    );
}

#[gpui_kit::test]
fn a_locked_window_is_not_looked_at(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        if let Some(session) = app.session_mut() {
            session.active_window_chats.insert(CHAT);
        }
    });
    lock(&app, vcx);
    app.read_with(vcx, |app, _| {
        let session = app.session().expect("demo session");
        assert!(
            session.active_window_chats.is_empty(),
            "its messages notify and sound under the lock"
        );
        assert!(app.story_playback_paused(), "no story plays behind it");
    });
}

#[gpui_kit::test]
fn a_lock_during_a_passcode_job_lands_after_its_answer(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    // Turn off: the job removes the passcode, so the held lock finds none.
    app.update_in(vcx, |app, window, cx| {
        app.account.passcode.enabled = true;
        app.run_passcode_job(
            || (),
            |app, (), _, _| app.account.passcode.enabled = false,
            window,
            cx,
        );
        app.lock_by_passcode(cx);
        assert!(!app.account.passcode.is_locked(), "held for the answer");
    });
    frame(vcx);
    app.read_with(vcx, |app, _| {
        assert!(
            !app.account.passcode.is_locked(),
            "no lock with no passcode to lift it"
        );
        assert!(!app.account.passcode.job.is_running());
    });
    // A job that keeps the passcode (a failed Turn off): the lock lands.
    app.update_in(vcx, |app, window, cx| {
        app.account.passcode.enabled = true;
        app.run_passcode_job(|| (), |_, (), _, _| {}, window, cx);
        app.lock_by_passcode(cx);
    });
    frame(vcx);
    assert!(app.read_with(vcx, |app, _| app.account.passcode.is_locked()));
}

#[gpui_kit::test]
fn closing_the_passcode_box_keeps_a_lock_waiting_for_the_job(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    // Turn off, then Esc (or "Lock now") on the box before the job answers.
    app.update_in(vcx, |app, window, cx| {
        app.account.passcode.enabled = true;
        app.open_passcode(cx);
        app.run_passcode_job(
            || (),
            |app, (), _, _| app.account.passcode.enabled = false,
            window,
            cx,
        );
        app.close_passcode(window, cx);
        assert!(
            app.account.passcode.job.is_running(),
            "the job outlives its box"
        );
        app.lock_by_passcode(cx);
        assert!(!app.account.passcode.is_locked(), "held for the answer");
    });
    frame(vcx);
    assert!(
        !app.read_with(vcx, |app, _| app.account.passcode.is_locked()),
        "locked with no passcode to lift it"
    );
}

#[gpui_kit::test]
fn the_lock_closes_the_call_windows_boxes_and_refuses_new_ones(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.call_boxes.add_people = Some(crate::ui::call_add_people::AddPeopleBox::new(1));
        app.group_call.stream.key_shown = true;
        app.group_call_screen.settings_open = true;
        app.group_call_screen.title_dialog = Some(
            crate::ui::dialogs::group_call::GroupCallTitleDialog::new(window, cx, "x"),
        );
    });
    lock(&app, vcx);
    app.update_in(vcx, |app, window, cx| {
        assert!(app.call_boxes.add_people.is_none(), "Add People goes");
        assert!(!app.group_call.stream.key_shown, "the stream key hides");
        assert!(!app.group_call_screen.settings_open);
        assert!(app.group_call_screen.title_dialog.is_none());
        app.open_group_call_settings(cx);
        app.open_group_call_title_dialog(window, cx);
        assert!(!app.group_call_screen.settings_open, "refused while locked");
        assert!(app.group_call_screen.title_dialog.is_none());
    });
}

#[gpui_kit::test]
fn nothing_connects_on_the_cleared_key_and_the_unlock_connects(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    lock(&app, vcx);
    let generation = app.update(vcx, |app, cx| {
        let generation = app.connect.generation;
        // A remote log out under the lock restarts the connection.
        app.start_connection(cx);
        assert_eq!(app.connect.generation, generation, "nothing prepared");
        assert!(app.account.passcode.deferred_connect, "it waits");
        // One prepared before the lock lands refused by the cleared key.
        app.open_client(Err(quill::connect::ConnectBlocker::LockedStore), cx);
        assert!(
            !matches!(
                app.current_auth(),
                quill::telegram::envelope::AuthorizationState::StartupFailed(_)
            ),
            "no key-locked card"
        );
        generation
    });
    unlock(&app, vcx);
    app.read_with(vcx, |app, _| {
        assert!(
            app.connect.generation > generation,
            "connects at the unlock"
        );
        assert!(!app.account.passcode.deferred_connect);
    });
}
