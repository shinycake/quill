use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext, px, size};
use quill::connect::ConnectBlocker;
use quill::disk_guard::FLOORS;
use quill::lang::{keys, tr};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn read(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, free: Option<u64>) {
    app.update_in(vcx, |app, _, cx| app.apply_disk_reading(free, cx));
}

fn shown(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (bool, bool, u64) {
    app.update_in(vcx, |app, _, cx| {
        (
            app.disk_low_banner(cx).is_some(),
            app.disk_full_view().is_some(),
            app.connection.generation,
        )
    })
}

#[gpui_kit::test]
fn the_bar_shows_only_while_the_disk_is_low(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let start = shown(&app, &mut vcx).2;
    read(&app, &mut vcx, Some(FLOORS.warn));
    assert_eq!(shown(&app, &mut vcx), (false, false, start));
    read(&app, &mut vcx, Some(FLOORS.warn - 1));
    assert_eq!(shown(&app, &mut vcx), (true, false, start));
    read(&app, &mut vcx, Some(FLOORS.warn));
    assert_eq!(shown(&app, &mut vcx), (true, false, start), "hysteresis");
    read(&app, &mut vcx, Some(FLOORS.warn_clear));
    assert_eq!(shown(&app, &mut vcx), (false, false, start));
}

#[gpui_kit::test]
fn a_full_disk_shows_the_screen_with_what_is_free_and_what_is_needed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    read(&app, &mut vcx, Some(FLOORS.close / 2));
    assert!(!shown(&app, &mut vcx).0, "the screen replaces the bar");
    let view = app.update_in(&mut vcx, |app, _, _| app.disk_full_view().unwrap());
    assert_eq!(view.title, tr(keys::Q_DISK_FULL_TITLE));
    assert!(view.body.contains("Available: 64.0 MB"), "{}", view.body);
    assert!(view.body.contains("Needed: 256.0 MB"), "{}", view.body);
}

#[gpui_kit::test]
fn leaving_a_full_disk_reconnects_once(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let start = shown(&app, &mut vcx).2;
    read(&app, &mut vcx, Some(0));
    assert_eq!(shown(&app, &mut vcx), (false, true, start));
    read(&app, &mut vcx, Some(FLOORS.open - 1));
    assert_eq!(
        shown(&app, &mut vcx),
        (false, true, start),
        "not enough yet"
    );
    read(&app, &mut vcx, None);
    assert_eq!(shown(&app, &mut vcx), (false, true, start), "no reading");
    read(&app, &mut vcx, Some(FLOORS.open));
    assert_eq!(shown(&app, &mut vcx), (true, false, start + 1));
    read(&app, &mut vcx, Some(FLOORS.warn_clear));
    assert_eq!(shown(&app, &mut vcx), (false, false, start + 1));
}

#[gpui_kit::test]
fn a_refused_open_holds_the_screen_until_the_open_floor(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let start = shown(&app, &mut vcx).2;
    app.update_in(&mut vcx, |app, _, cx| {
        app.open_client(Err(ConnectBlocker::DiskFull), cx);
        assert!(app.connection.status_note().is_empty());
    });
    assert_eq!(shown(&app, &mut vcx), (false, true, start));
    read(&app, &mut vcx, Some(FLOORS.close));
    assert_eq!(shown(&app, &mut vcx), (false, true, start));
    read(&app, &mut vcx, Some(FLOORS.open));
    assert_eq!(shown(&app, &mut vcx), (true, false, start + 1));
}

#[gpui_kit::test]
fn recovering_while_locked_connects_at_the_unlock(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let start = shown(&app, &mut vcx).2;
    read(&app, &mut vcx, Some(0));
    app.update_in(&mut vcx, |app, _, _| app.account.passcode.locked = true);
    read(&app, &mut vcx, Some(FLOORS.warn_clear));
    assert_eq!(shown(&app, &mut vcx), (false, false, start));
    assert!(app.update_in(&mut vcx, |app, _, _| app.account.passcode.deferred_connect));
}
