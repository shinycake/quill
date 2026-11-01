use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext, VisualTestContext, px, size};
use quill::connect::ConnectBlocker;
use quill::disk_guard::FLOORS;
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::settings_catalog::Page;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    new_demo_app(cx, "ready-chats", 800.)
}

fn new_demo_app(
    cx: &mut TestAppContext,
    demo: &'static str,
    height: f32,
) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(height)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(demo))));
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
    assert!(view.body.contains("Available: 80.0 MB"), "{}", view.body);
    assert!(view.body.contains("Needed: 288.0 MB"), "{}", view.body);
}

#[gpui_kit::test]
fn a_refusal_before_the_first_reading_says_only_what_is_needed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        app.open_client(Err(ConnectBlocker::DiskFull), cx);
    });
    let view = app.update_in(&mut vcx, |app, _, _| app.disk_full_view().unwrap());
    assert!(!view.body.contains("Available"), "{}", view.body);
    assert!(view.body.contains("Needed: 288.0 MB"), "{}", view.body);
}

#[gpui_kit::test]
fn the_low_bar_opens_the_storage_usage_list(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_demo_app(cx, "ready-chats", 320.);
    vcx.run_until_parked();
    read(&app, &mut vcx, Some(FLOORS.warn - 1));
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.update(|window, cx| window.click("disk-low-storage", cx));
    for _ in 0..3 {
        vcx.run_until_parked();
        vcx.update(|window, cx| window.render_frame(cx));
        vcx.update(|window, cx| window.simulate_next_frame(cx));
    }
    app.read_with(&vcx, |app, _| {
        assert!(app.settings.open);
        assert_eq!(app.settings.page, Some(Page::DataStorage));
        assert!(
            app.settings.storage_scroll.page.offset().y < px(0.),
            "the storage usage list is below the fold at this height"
        );
    });
}

#[gpui_kit::test]
fn the_low_bar_has_no_button_on_the_sign_in_screen(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_demo_app(cx, "wait-phone", 800.);
    vcx.run_until_parked();
    read(&app, &mut vcx, Some(FLOORS.warn - 1));
    vcx.update(|window, cx| window.render_frame(cx));
    assert!(shown(&app, &mut vcx).0);
    assert!(vcx.update(|window, _| window.try_find("disk-low-storage").is_none()));
}

#[gpui_kit::test]
fn a_full_disk_closes_chat_windows(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.run_until_parked();
    let main_only = vcx.windows().len();
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_chat_in_new_window(ChatId(11), window, cx);
    });
    vcx.run_until_parked();
    assert_eq!(vcx.windows().len(), main_only + 1, "the chat window opened");
    read(&app, &mut vcx, Some(0));
    vcx.run_until_parked();
    assert_eq!(vcx.windows().len(), main_only);
}

#[gpui_kit::test]
fn going_full_brings_the_main_window_forward_once(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.run_until_parked();
    vcx.deactivate_window();
    read(&app, &mut vcx, Some(0));
    vcx.run_until_parked();
    assert!(vcx.update(|window, _| window.is_window_active()));
    vcx.deactivate_window();
    read(&app, &mut vcx, Some(0));
    vcx.run_until_parked();
    assert!(!vcx.update(|window, _| window.is_window_active()), "once");
}

#[gpui_kit::test]
fn a_refused_open_brings_the_main_window_forward(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.deactivate_window();
    app.update_in(&mut vcx, |app, _, cx| {
        app.open_client(Err(ConnectBlocker::DiskFull), cx);
    });
    vcx.run_until_parked();
    assert!(vcx.update(|window, _| window.is_window_active()));
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
