//! Work started on one screen does not run on the next.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::AccountKey;
use std::cell::Cell;
use std::rc::Rc;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    (app, vcx)
}

fn switch(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, window, cx| {
        app.switch_account(&AccountKey("other".into()), "", window, cx);
    });
    vcx.run_until_parked();
}

/// What a task's update came back with: `Some(true)` ran, `Some(false)`
/// was `Gone`, `None` has not finished.
type Outcome = Rc<Cell<Option<bool>>>;

/// Start a screen task that waits for `go`, then records whether its
/// update ran.
fn start_screen_task(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
) -> (futures_channel::oneshot::Sender<()>, Outcome) {
    let (go, wait) = futures_channel::oneshot::channel::<()>();
    let outcome: Outcome = Rc::default();
    let seen = outcome.clone();
    app.update(vcx, |app, cx| {
        app.spawn_screen(cx, async move |this, cx| {
            let _ = wait.await;
            let ran = this.update(cx, |_, _| ()).is_ok();
            seen.set(Some(ran));
        })
        .detach();
    });
    (go, outcome)
}

#[gpui_kit::test]
fn a_screen_task_started_before_a_switch_does_not_run_after_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let before = app.read_with(&vcx, |app, _| app.screen_epoch());
    let (go, outcome) = start_screen_task(&app, &mut vcx);
    switch(&app, &mut vcx);
    let after = app.read_with(&vcx, |app, _| app.screen_epoch());
    assert_ne!(before, after, "the switch is a new screen");
    let _ = go.send(());
    vcx.run_until_parked();
    assert_eq!(outcome.get(), Some(false), "the update came back Gone");
}

#[gpui_kit::test]
fn a_screen_task_started_after_a_switch_runs(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    switch(&app, &mut vcx);
    let (go, outcome) = start_screen_task(&app, &mut vcx);
    let _ = go.send(());
    vcx.run_until_parked();
    assert_eq!(outcome.get(), Some(true));
}

#[gpui_kit::test]
fn an_app_task_runs_across_a_switch(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let (go, wait) = futures_channel::oneshot::channel::<()>();
    let outcome: Outcome = Rc::default();
    let seen = outcome.clone();
    app.update(&mut vcx, |_, cx| {
        QuillApp::spawn_app(cx, async move |this, cx| {
            let _ = wait.await;
            seen.set(Some(this.update(cx, |_, _| ()).is_ok()));
        })
        .detach();
    });
    switch(&app, &mut vcx);
    let _ = go.send(());
    vcx.run_until_parked();
    assert_eq!(outcome.get(), Some(true));
}

/// A `cx.defer` body queued before the switch in the same update (the
/// platform's late window-close report, round-5 2f) runs after it: as a
/// screen deferral it is dropped.
#[gpui_kit::test]
fn a_deferred_screen_body_is_dropped_by_a_switch_in_the_same_update(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let ran = Rc::new(Cell::new(false));
    let seen = ran.clone();
    app.update_in(&mut vcx, |app, window, cx| {
        app.defer_screen(cx, move |_, _| seen.set(true));
        app.switch_account(&AccountKey("other".into()), "", window, cx);
    });
    vcx.run_until_parked();
    assert!(!ran.get(), "queued on the old screen");

    let ran = Rc::new(Cell::new(false));
    let seen = ran.clone();
    app.update(&mut vcx, |app, cx| {
        app.defer_screen(cx, move |_, _| seen.set(true));
    });
    vcx.run_until_parked();
    assert!(ran.get(), "queued and run on the same screen");
}

/// A task of a chat window carries the screen too: the window the switch
/// closed is gone, and so is its screen.
#[gpui_kit::test]
fn in_screen_scope_runs_only_on_its_screen(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let epoch = app.read_with(&vcx, |app, _| app.screen_epoch());
    let ran = app.update(&mut vcx, |app, cx| app.in_screen_scope(epoch, cx, |_, _| 7));
    assert_eq!(ran, Some(7));
    switch(&app, &mut vcx);
    let ran = app.update(&mut vcx, |app, cx| app.in_screen_scope(epoch, cx, |_, _| 7));
    assert_eq!(ran, None);
}
