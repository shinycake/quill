use super::super::actions::QuitApp;
use super::super::app::QuillApp;
use super::super::export_progress::take_ended_exports;
use super::super::keybindings::bind_keys;
use super::super::screenshot_demo::ScreenshotDemo;
use super::super::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualContext, VisualTestContext, px, size};
use quill::export::account::{AccountSettings, Section, Sections};
use quill::export::{AccountExport, ChatExport, ExportSettings};
use quill::rich_html::{PageLabels, RichHtmlExport, RichHtmlRequest};
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Wake, Waker};
use std::thread::Thread;
use std::time::{Duration, Instant};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
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
    let app = slot.borrow().clone().unwrap();
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("quill-export-quit-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn running_chat_export(dir: &Path) -> ChatExport {
    let settings = ExportSettings::new(16, "Export Chat".into(), dir.to_path_buf());
    ChatExport::start(settings, dir.to_path_buf(), None).unwrap()
}

fn running_account_export(dir: &Path) -> AccountExport {
    let mut settings = AccountSettings::new(dir.to_path_buf());
    settings.sections = Sections::of(&[Section::Sessions]);
    AccountExport::start(settings, dir.to_path_buf(), None).unwrap()
}

struct Unpark(Thread);

impl Wake for Unpark {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
}

fn wait_for_the_end(ended: impl Fn() -> bool) {
    let waker = Waker::from(Arc::new(Unpark(std::thread::current())));
    let mut poll = Context::from_waker(&waker);
    loop {
        let seen = quill::wake::UPDATES.epoch();
        if ended() {
            return;
        }
        let mut wait = pin!(quill::wake::UPDATES.wait_since(seen));
        while wait.as_mut().poll(&mut poll).is_pending() {
            std::thread::park();
        }
    }
}

fn settle(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    vcx.executor().advance_clock(Duration::from_millis(600));
    vcx.run_until_parked();
}

fn ask(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Rc<Cell<bool>> {
    settle(vcx);
    let exited = Rc::new(Cell::new(false));
    let done = exited.clone();
    app.update_in(vcx, |app, window, cx| {
        app.exit_after_exports(move |_, _| done.set(true), window, cx);
    });
    settle(vcx);
    exited
}

fn app_wide_actions(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let main = vcx.window_handle();
    let weak = app.downgrade();
    vcx.update(|_, cx| crate::ui::os_menus::register_actions(main, weak, cx));
}

fn box_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.dialogs.export_stop.is_some())
}

#[gpui_kit::test]
fn with_nothing_exporting_quit_goes_straight_through(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let exited = ask(&app, &mut vcx);
    assert!(exited.get());
    assert!(!box_open(&app, &mut vcx));
}

#[gpui_kit::test]
fn a_running_export_asks_and_cancel_leaves_it_running(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let dir = scratch("cancel");
    let export = running_chat_export(&dir);
    app.update(vcx, |app, _| {
        app.demo_session.as_mut().unwrap().settings.chat_export = Some(export);
    });
    let exited = ask(&app, vcx);
    assert!(!exited.get(), "held back");
    assert!(box_open(&app, vcx));

    let again = ask(&app, vcx);
    vcx.simulate_keystrokes("escape");
    settle(vcx);
    assert!(!box_open(&app, vcx), "one Cancel closes it");
    assert!(!exited.get() && !again.get());
    app.read_with(vcx, |app, _| {
        let session = app.session().unwrap();
        let export = session.settings.chat_export.as_ref().unwrap();
        assert!(!export.progress().is_settled(), "still exporting");
    });

    assert!(!ask(&app, vcx).get());
    assert!(box_open(&app, vcx));
    app.update(vcx, |app, _| {
        app.demo_session.as_mut().unwrap().settings.chat_export = None;
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn stop_ends_the_export_and_then_quits(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let dir = scratch("stop");
    let export = running_account_export(&dir);
    app.update(vcx, |app, _| {
        app.demo_session.as_mut().unwrap().settings.account_export = Some(export);
    });
    let exited = ask(&app, vcx);
    assert!(!exited.get());
    vcx.simulate_keystrokes("enter");
    settle(vcx);
    assert!(exited.get(), "stopped, then exited");
    let export = app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().unwrap();
        session.settings.account_export.take().unwrap()
    });
    wait_for_the_end(|| export.progress().is_settled());
    drop(export);
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn the_quit_action_asks_while_an_export_runs(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let dir = scratch("action");
    let export = running_chat_export(&dir);
    app.update(vcx, |app, _| {
        app.demo_session.as_mut().unwrap().settings.chat_export = Some(export);
        app.dialogs.chat_export_progress_hidden = true;
    });
    settle(vcx);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| input.focus(window, cx));
        });
    });
    vcx.dispatch_action(QuitApp);
    settle(vcx);
    assert!(box_open(&app, vcx), "asked instead of quitting");
    app.update(vcx, |app, _| {
        app.demo_session.as_mut().unwrap().settings.chat_export = None;
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn an_export_that_ended_holds_nothing_and_reports_from_off_screen(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let dir = scratch("ended");
    let export = running_chat_export(&dir);
    export.cancel();
    wait_for_the_end(|| export.progress().is_settled());
    let running = running_account_export(&dir);
    app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().unwrap();
        session.settings.chat_export = Some(export);
        session.settings.account_export = Some(running);
        let notes = take_ended_exports(session);
        assert_eq!(notes.len(), 1, "the one that ended");
        assert!(session.settings.chat_export.is_none());
        assert!(session.settings.account_export.is_some(), "still running");
        session.settings.account_export = None;
    });
    assert!(ask(&app, vcx).get(), "nothing runs any more");
    let _ = std::fs::remove_dir_all(&dir);
}

fn running_rich_html_export(dir: &Path) -> RichHtmlExport {
    let request = RichHtmlRequest {
        chat_id: quill::ids::ChatId(16),
        message_id: quill::ids::MessageId(5),
        base_dir: dir.to_path_buf(),
        fallback_title: "Chat".into(),
        labels: PageLabels::current(),
    };
    RichHtmlExport::start(request, dir.to_path_buf(), None).unwrap()
}

#[gpui_kit::test]
fn a_running_save_as_html_asks_and_stop_ends_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let dir = scratch("rich-html");
    let export = running_rich_html_export(&dir);
    app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().unwrap();
        session.settings.rich_html_exports.push(export);
    });
    let exited = ask(&app, vcx);
    assert!(!exited.get(), "held back");
    assert!(box_open(&app, vcx));
    vcx.simulate_keystrokes("enter");
    settle(vcx);
    assert!(exited.get(), "stopped, then exited");
    let export = app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().unwrap();
        session.settings.rich_html_exports.remove(0)
    });
    wait_for_the_end(|| export.outcome().is_some());
    drop(export);
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn an_os_quit_goes_ahead_when_nothing_holds_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app_wide_actions(&app, vcx);
    settle(vcx);
    assert!(vcx.cx.simulate_quit_request());
    settle(vcx);
    assert!(!box_open(&app, vcx));
}

#[gpui_kit::test]
fn an_os_quit_during_an_export_is_refused_and_stop_ends_the_app(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app_wide_actions(&app, vcx);
    let dir = scratch("os-quit");
    let export = running_chat_export(&dir);
    app.update(vcx, |app, _| {
        app.demo_session.as_mut().unwrap().settings.chat_export = Some(export);
    });
    settle(vcx);
    assert!(!vcx.cx.simulate_quit_request(), "refused");
    settle(vcx);
    assert!(box_open(&app, vcx), "asked instead");
    assert!(!vcx.cx.simulate_quit_request(), "a Dock Quit again");
    settle(vcx);
    assert!(box_open(&app, vcx));
    assert!(!vcx.cx.windows().is_empty(), "still running");

    vcx.simulate_keystrokes("enter");
    settle(vcx);
    assert!(vcx.cx.windows().is_empty(), "Stop ends the app");
    let export = app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().unwrap();
        session.settings.chat_export.take().unwrap()
    });
    wait_for_the_end(|| export.progress().is_settled());
    drop(export);
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn cmd_q_with_the_box_focused_becomes_what_stop_does(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app_wide_actions(&app, vcx);
    let dir = scratch("cmd-q");
    let export = running_chat_export(&dir);
    app.update(vcx, |app, _| {
        app.demo_session.as_mut().unwrap().settings.chat_export = Some(export);
    });
    let restart = ask(&app, vcx);
    assert!(box_open(&app, vcx));

    let quit = if cfg!(target_os = "macos") {
        "cmd-q"
    } else {
        "ctrl-q"
    };
    vcx.simulate_keystrokes(quit);
    settle(vcx);
    assert!(box_open(&app, vcx), "the box stays up");
    assert!(!vcx.cx.windows().is_empty(), "still running");

    vcx.simulate_keystrokes("enter");
    settle(vcx);
    assert!(!restart.get(), "the first request gave way");
    assert!(vcx.cx.windows().is_empty(), "Stop quits");
    let export = app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().unwrap();
        session.settings.chat_export.take().unwrap()
    });
    wait_for_the_end(|| export.progress().is_settled());
    drop(export);
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn a_quit_waits_for_a_stopped_export_to_end(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let dir = scratch("quit-wait");
    let export = running_chat_export(&dir);
    app.update(vcx, |app, _| {
        app.demo_session.as_mut().unwrap().settings.chat_export = Some(export);
    });
    let far_off = Instant::now() + Duration::from_secs(600);
    app.update(vcx, |app, _| app.stop_exports_and_wait(far_off));
    let settled = app.read_with(vcx, |app, _| {
        let session = app.session().unwrap();
        let export = session.settings.chat_export.as_ref().unwrap();
        export.progress().is_settled()
    });
    assert!(settled, "the worker ended before the wait did");
    app.update(vcx, |app, _| {
        app.demo_session.as_mut().unwrap().settings.chat_export = None;
    });
    let _ = std::fs::remove_dir_all(&dir);
}

#[gpui_kit::test]
fn closing_for_quit_stops_every_export(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let dir = scratch("quit-stop");
    let chat = running_chat_export(&dir);
    let account = running_account_export(&dir);
    let rich_html = running_rich_html_export(&dir);
    app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().unwrap();
        session.settings.chat_export = Some(chat);
        session.settings.account_export = Some(account);
        session.settings.rich_html_exports.push(rich_html);
    });
    app.update(vcx, |app, _| app.close_all_for_quit(Instant::now()));
    app.update(vcx, |app, _| {
        let settings = &mut app.demo_session.as_mut().unwrap().settings;
        assert_eq!(settings.export_links().count(), 3);
        assert!(settings.export_links().all(|link| link.is_cancelled()));
        settings.chat_export = None;
        settings.account_export = None;
        settings.rich_html_exports.clear();
    });
    let _ = std::fs::remove_dir_all(&dir);
}
