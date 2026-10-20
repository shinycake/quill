#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! Files dragged over the chat list through GPUI: the row under the pointer
//! lights up, a rest of a second opens its chat, a drop opens it and attaches
//! the files, and a drag that holds nothing sendable is not accepted.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, ExternalPaths, FileDropEvent, PlatformInput, Point, TestAppContext,
    VisualTestContext, px, size,
};
use quill::ids::ChatId;
use std::path::PathBuf;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    crate::ui::interface_zoom::set_initial_zoom(1.);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-forum-row-topics");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn row_centre(vcx: &mut VisualTestContext, chat: u64) -> Point<gpui_kit::Pixels> {
    vcx.update(|window, _| window.find(("chat-row", chat)).bounds().center())
}

fn dropped_file(name: &str) -> PathBuf {
    let dir = quill::scratch::process_dir("quill-drop-on-row");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(name);
    let _ = std::fs::write(&path, b"hello");
    path
}

fn file_drop(vcx: &mut VisualTestContext, event: FileDropEvent) {
    vcx.update(|window, cx| {
        window.dispatch_event(PlatformInput::FileDrop(event), cx);
    });
    frames(vcx);
}

fn entered(vcx: &mut VisualTestContext, at: Point<gpui_kit::Pixels>, path: PathBuf) {
    file_drop(
        vcx,
        FileDropEvent::Entered {
            position: at,
            paths: ExternalPaths(std::iter::once(path).collect()),
        },
    );
    file_drop(vcx, FileDropEvent::Pending { position: at });
}

fn lit(vcx: &mut VisualTestContext, chat: u64) -> bool {
    vcx.update(|window, _| window.try_find(("drop-row-hot", chat)).is_some())
}

fn open_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<ChatId> {
    app.update(vcx, |app, _| app.session().and_then(|s| s.open_chat))
}

fn attached(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> usize {
    app.update(vcx, |app, _| app.pending_attachments().len())
}

#[gpui_kit::test]
fn a_file_drag_lights_up_the_row_under_it_and_only_that_row(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let at = row_centre(vcx, 72);
    entered(vcx, at, dropped_file("lit.txt"));
    assert!(lit(vcx, 72));
    assert!(!lit(vcx, 71));
    // Over another row, the light moves.
    let other = row_centre(vcx, 71);
    file_drop(vcx, FileDropEvent::Pending { position: other });
    assert!(lit(vcx, 71));
    assert!(!lit(vcx, 72));
    // The drag leaves: nothing stays lit.
    file_drop(vcx, FileDropEvent::Exited);
    assert!(!lit(vcx, 71) && !lit(vcx, 72));
}

#[gpui_kit::test]
fn a_drop_on_a_row_opens_that_chat_and_attaches_the_file(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    assert_ne!(open_chat(&app, vcx), Some(ChatId(72)));
    let before = attached(&app, vcx);
    let at = row_centre(vcx, 72);
    entered(vcx, at, dropped_file("sent.txt"));
    file_drop(vcx, FileDropEvent::Submit { position: at });
    assert_eq!(open_chat(&app, vcx), Some(ChatId(72)));
    assert_eq!(attached(&app, vcx), before + 1);
    assert!(!lit(vcx, 72));
}

#[gpui_kit::test]
fn a_drag_with_nothing_sendable_is_not_accepted(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let at = row_centre(vcx, 72);
    let before = open_chat(&app, vcx);
    // A folder next to a file is refused (`classify`).
    let dir = quill::scratch::process_dir("quill-drop-on-row");
    let file = dropped_file("mixed.txt");
    file_drop(
        vcx,
        FileDropEvent::Entered {
            position: at,
            paths: ExternalPaths([dir, file].into_iter().collect()),
        },
    );
    file_drop(vcx, FileDropEvent::Pending { position: at });
    assert!(!lit(vcx, 72));
    file_drop(vcx, FileDropEvent::Submit { position: at });
    assert_eq!(open_chat(&app, vcx), before);
}

#[gpui_kit::test]
fn resting_on_a_row_for_a_second_opens_its_chat_while_the_drag_goes_on(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let at = row_centre(vcx, 72);
    entered(vcx, at, dropped_file("rest.txt"));
    assert_ne!(open_chat(&app, vcx), Some(ChatId(72)));
    // Less than a second: still the old chat.
    vcx.executor().advance_clock(std::time::Duration::from_millis(500));
    frames(vcx);
    assert_ne!(open_chat(&app, vcx), Some(ChatId(72)));
    vcx.executor().advance_clock(std::time::Duration::from_millis(1500));
    frames(vcx);
    assert_eq!(open_chat(&app, vcx), Some(ChatId(72)));
}

#[gpui_kit::test]
fn moving_the_pointer_restarts_the_wait(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let at = row_centre(vcx, 72);
    entered(vcx, at, dropped_file("restart.txt"));
    vcx.executor().advance_clock(std::time::Duration::from_millis(800));
    frames(vcx);
    file_drop(vcx, FileDropEvent::Pending { position: at });
    vcx.executor().advance_clock(std::time::Duration::from_millis(800));
    frames(vcx);
    assert_ne!(open_chat(&app, vcx), Some(ChatId(72)), "the move restarted it");
    vcx.executor().advance_clock(std::time::Duration::from_millis(400));
    frames(vcx);
    assert_eq!(open_chat(&app, vcx), Some(ChatId(72)));
}
