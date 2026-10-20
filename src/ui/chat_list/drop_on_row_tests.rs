#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! Files dragged over the chat list through GPUI: the row under the pointer
//! lights up, a rest of a second opens its chat, a drop opens it and attaches
//! the files, a drag that holds nothing sendable is not accepted, and the list
//! order stays put while the drag hovers it.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, ExternalPaths, FileDropEvent, PlatformInput, Point, TestAppContext,
    VisualTestContext, point, px, size,
};
use quill::ids::ChatId;
use quill::telegram::client::copy_and_parse;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

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

fn row_top(vcx: &mut VisualTestContext, chat: u64) -> f32 {
    vcx.update(|window, _| window.find(("chat-row", chat)).bounds().origin.y.into())
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

fn entered(vcx: &mut VisualTestContext, at: Point<gpui_kit::Pixels>, paths: Vec<PathBuf>) {
    file_drop(
        vcx,
        FileDropEvent::Entered {
            position: at,
            paths: ExternalPaths(paths.into_iter().collect()),
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

fn wait(vcx: &mut VisualTestContext, millis: u64) {
    vcx.executor().advance_clock(Duration::from_millis(millis));
    frames(vcx);
}

#[gpui_kit::test]
fn a_file_drag_lights_up_the_row_under_it_and_only_that_row(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let at = row_centre(vcx, 72);
    entered(vcx, at, vec![dropped_file("lit.txt")]);
    assert!(lit(vcx, 72));
    assert!(!lit(vcx, 71));
    // Over another row, the light moves.
    let other = row_centre(vcx, 71);
    file_drop(vcx, FileDropEvent::Pending { position: other });
    assert!(lit(vcx, 71));
    assert!(!lit(vcx, 72));
    // The drag leaves the window: nothing stays lit.
    file_drop(vcx, FileDropEvent::Exited);
    assert!(!lit(vcx, 71) && !lit(vcx, 72));
}

#[gpui_kit::test]
fn a_drop_on_a_row_opens_that_chat_and_attaches_the_file(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    assert_ne!(open_chat(&app, vcx), Some(ChatId(12)));
    let before = attached(&app, vcx);
    let at = row_centre(vcx, 12);
    entered(vcx, at, vec![dropped_file("sent.txt")]);
    file_drop(vcx, FileDropEvent::Submit { position: at });
    assert_eq!(open_chat(&app, vcx), Some(ChatId(12)));
    assert_eq!(attached(&app, vcx), before + 1);
    assert!(!lit(vcx, 12));
}

#[gpui_kit::test]
fn a_single_folder_is_dropped_as_one_archive(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let folder = quill::scratch::process_dir("quill-drop-on-row-folder");
    let _ = std::fs::create_dir_all(&folder);
    let _ = std::fs::write(folder.join("inside.txt"), b"hello");
    let before = attached(&app, vcx);
    let at = row_centre(vcx, 12);
    entered(vcx, at, vec![folder]);
    assert!(lit(vcx, 12), "a single folder is accepted");
    file_drop(vcx, FileDropEvent::Submit { position: at });
    assert_eq!(open_chat(&app, vcx), Some(ChatId(12)));
    // The archive is built in the background.
    for _ in 0..50 {
        if attached(&app, vcx) > before {
            break;
        }
        vcx.executor().advance_clock(Duration::from_millis(50));
        frames(vcx);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(attached(&app, vcx), before + 1);
}

#[gpui_kit::test]
fn a_drop_on_a_forum_row_opens_nothing_and_attaches_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let chat = open_chat(&app, vcx);
    let before = attached(&app, vcx);
    let at = row_centre(vcx, 72);
    entered(vcx, at, vec![dropped_file("forum.txt")]);
    file_drop(vcx, FileDropEvent::Submit { position: at });
    assert_eq!(open_chat(&app, vcx), chat);
    assert_eq!(attached(&app, vcx), before);
}

#[gpui_kit::test]
fn a_drop_on_a_chat_that_takes_no_files_does_not_open_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    // A group that allows text but no files.
    app.update(vcx, |app, _| {
        let id = ChatId(13);
        if let Some(summary) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.chats.get_mut(&id.0))
        {
            summary.kind = quill::telegram::envelope::ChatKind::Supergroup {
                supergroup_id: 4242,
                is_channel: false,
            };
            summary.permissions = Some(quill::telegram::envelope::ChatPermissions {
                can_send_basic_messages: true,
                ..Default::default()
            });
        }
        assert!(app.start_files_refusal(id).is_some());
    });
    frames(vcx);
    let chat = open_chat(&app, vcx);
    let before = attached(&app, vcx);
    let at = row_centre(vcx, 13);
    entered(vcx, at, vec![dropped_file("channel.txt")]);
    file_drop(vcx, FileDropEvent::Submit { position: at });
    assert_eq!(open_chat(&app, vcx), chat);
    assert_eq!(attached(&app, vcx), before);
}

#[gpui_kit::test]
fn a_drag_with_nothing_sendable_is_not_accepted(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let at = row_centre(vcx, 12);
    let before = open_chat(&app, vcx);
    // A folder next to a file is refused (`classify`).
    let dir = quill::scratch::process_dir("quill-drop-on-row");
    entered(vcx, at, vec![dir, dropped_file("mixed.txt")]);
    assert!(!lit(vcx, 12));
    file_drop(vcx, FileDropEvent::Submit { position: at });
    assert_eq!(open_chat(&app, vcx), before);
}

#[gpui_kit::test]
fn resting_on_a_row_for_a_second_opens_its_chat_while_the_drag_goes_on(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let at = row_centre(vcx, 12);
    entered(vcx, at, vec![dropped_file("rest.txt")]);
    wait(vcx, 500);
    assert_ne!(open_chat(&app, vcx), Some(ChatId(12)));
    wait(vcx, 600);
    assert_eq!(open_chat(&app, vcx), Some(ChatId(12)));
}

#[gpui_kit::test]
fn updates_at_the_same_point_do_not_restart_the_wait(cx: &mut TestAppContext) {
    // macOS and Windows resend the update while the pointer rests.
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let at = row_centre(vcx, 12);
    entered(vcx, at, vec![dropped_file("still.txt")]);
    wait(vcx, 400);
    file_drop(vcx, FileDropEvent::Pending { position: at });
    wait(vcx, 400);
    file_drop(vcx, FileDropEvent::Pending { position: at });
    assert_ne!(open_chat(&app, vcx), Some(ChatId(12)));
    wait(vcx, 300);
    assert_eq!(open_chat(&app, vcx), Some(ChatId(12)));
}

#[gpui_kit::test]
fn moving_the_pointer_restarts_the_wait(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let at = row_centre(vcx, 12);
    entered(vcx, at, vec![dropped_file("restart.txt")]);
    wait(vcx, 800);
    file_drop(
        vcx,
        FileDropEvent::Pending {
            position: point(at.x + px(4.), at.y),
        },
    );
    wait(vcx, 800);
    assert_ne!(
        open_chat(&app, vcx),
        Some(ChatId(12)),
        "the move restarted it"
    );
    wait(vcx, 400);
    assert_eq!(open_chat(&app, vcx), Some(ChatId(12)));
}

#[gpui_kit::test]
fn leaving_the_window_cancels_the_wait(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let at = row_centre(vcx, 12);
    entered(vcx, at, vec![dropped_file("leave.txt")]);
    wait(vcx, 500);
    file_drop(vcx, FileDropEvent::Exited);
    wait(vcx, 1500);
    assert_ne!(open_chat(&app, vcx), Some(ChatId(12)));
    assert!(!lit(vcx, 12));
}

fn move_chat_to_top(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, chat: i64) {
    app.update(vcx, |app, _| {
        let sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> =
            app.demo_ui.sink.clone();
        let Some(session) = app.demo_session.as_mut() else {
            return;
        };
        let seq = AtomicU64::new(session.last_seq);
        let json = format!(
            r#"{{"@type":"updateChatPosition","chat_id":{chat},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"9223372036854775000","is_pinned":false}}}}"#
        );
        if let Some(owned) = copy_and_parse(&json, &seq, &sink) {
            session.apply(owned);
        }
    });
    frames(vcx);
}

#[gpui_kit::test]
fn the_list_keeps_its_order_while_a_drag_hovers_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = open(cx);
    let vcx = &mut vcx;
    let at = row_centre(vcx, 12);
    let lit_row_top = row_top(vcx, 12);
    entered(vcx, at, vec![dropped_file("frozen.txt")]);
    // A chat below jumps to the top; the rows stay where they were.
    move_chat_to_top(&app, vcx, 13);
    assert_eq!(row_top(vcx, 12), lit_row_top);
    // Two seconds after the last update the new order shows.
    wait(vcx, 2100);
    assert_ne!(row_top(vcx, 12), lit_row_top);
}
