//! One press on a chat video whose clip is not local: the viewer opens,
//! the clip downloads as the user's (taking over the chat's background
//! prefetch), and it plays by itself once the file lands, with no second
//! press, even when TDLib reports the transfer idle on the way.

use super::{Script, queue_backend};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::diagnostics::DiagnosticSink;
use quill::ids::{ChatId, MessageId};
use quill::telegram::client::copy_and_parse;
use quill::telegram::envelope::LocalFileState;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;

const CHAT: ChatId = ChatId(11);
/// The viewer's clip (`apply_ready_video_viewer`) and its file.
const CLIP: MessageId = MessageId(204);
const CLIP_FILE: i32 = 96;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = Rc::new(RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-video-inline")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..4 {
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
    }
}

fn scripts() -> Vec<Rc<RefCell<Script>>> {
    (0..20)
        .map(|_| {
            let script = Rc::new(RefCell::new(Script {
                ready: true,
                ..Script::default()
            }));
            queue_backend(&script);
            script
        })
        .collect()
}

/// The clip's file as TDLib reports it: `active` while it transfers,
/// `done` once it is whole.
fn set_clip(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, active: bool, done: bool) {
    app.update(vcx, |app, _| {
        let Some(session) = app.demo_session.as_mut() else {
            panic!("the demo session");
        };
        session.media.files.update(CLIP_FILE, |file| {
            file.local = LocalFileState::from_parts(
                file.local.path().to_string(),
                file.local.can_be_downloaded(),
                active,
                done,
                file.local.downloaded_size(),
            );
        });
    });
}

/// The clip's file arriving as a TDLib `updateFile`, through the reducer.
fn update_clip(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, active: bool, done: bool) {
    app.update(vcx, |app, _| {
        let Some(session) = app.demo_session.as_mut() else {
            panic!("the demo session");
        };
        let path = session
            .media
            .files
            .get(&CLIP_FILE)
            .map(|file| file.local.path().to_string())
            .unwrap_or_default();
        let json = format!(
            r#"{{"@type":"updateFile","file":{{"@type":"file","id":{CLIP_FILE},"size":24,"expected_size":24,"local":{{"@type":"localFile","path":{path},"can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":{active},"is_downloading_completed":{done},"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":24}}}}}}"#,
            path = serde_json::to_string(&path).unwrap_or_default(),
        );
        let sink: Arc<dyn DiagnosticSink> = app.demo_ui.sink.clone();
        if let Some(owned) = copy_and_parse(&json, &app.demo_ui.seq, &sink) {
            session.apply(owned);
        }
    });
}

/// What the poll loop runs for parked plays after each batch of updates.
fn poll(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| {
        app.discard_stopped_media_playback(cx);
        app.resume_play_intent(cx);
    });
    settle(vcx);
}

fn viewer_plays(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.viewer_video_playing())
}

/// The demo chat with the viewer's clip in it, not downloaded.
fn chat_with_clip(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    let (app, mut vcx) = new_app(cx);
    vcx.update(|window, _| window.activate_window());
    app.update(&mut vcx, |app, _| {
        let Some(session) = app.demo_session.as_mut() else {
            panic!("the demo session");
        };
        app.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        crate::ui::demos::video_fixtures::apply_ready_video_viewer(
            session,
            &app.demo_ui.sink,
            &app.demo_ui.seq,
        );
    });
    set_clip(&app, &mut vcx, false, false);
    settle(&mut vcx);
    (app, vcx)
}

fn user_downloads_clip(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| {
        app.session()
            .is_some_and(|session| session.media.downloads.is_user(CLIP_FILE))
    })
}

#[gpui_kit::test]
fn one_press_downloads_and_plays_the_clip(cx: &mut TestAppContext) {
    let _scripts = scripts();
    let (app, mut vcx) = chat_with_clip(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| app.open_media_viewer(CHAT, CLIP, cx));
    settle(vcx);
    assert!(!viewer_plays(&app, vcx), "nothing to play yet");

    // TDLib starts the transfer; polls run while it goes.
    set_clip(&app, vcx, true, false);
    poll(&app, vcx);
    poll(&app, vcx);
    assert!(!viewer_plays(&app, vcx), "still downloading");

    set_clip(&app, vcx, false, true);
    poll(&app, vcx);
    assert!(viewer_plays(&app, vcx), "the clip plays once it lands");
}

/// TDLib reports the transfer idle for a moment, then goes on and
/// finishes it: the press still plays the clip.
#[gpui_kit::test]
fn a_transfer_that_idles_and_resumes_still_plays(cx: &mut TestAppContext) {
    let _scripts = scripts();
    let (app, mut vcx) = chat_with_clip(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| app.open_media_viewer(CHAT, CLIP, cx));
    settle(vcx);
    update_clip(&app, vcx, true, false);
    poll(&app, vcx);
    update_clip(&app, vcx, false, false);
    poll(&app, vcx);
    update_clip(&app, vcx, true, false);
    poll(&app, vcx);
    update_clip(&app, vcx, false, true);
    poll(&app, vcx);
    assert!(viewer_plays(&app, vcx), "the clip plays once it lands");
}

/// The chat was already fetching the clip in the background (autoplay's
/// prefetch, at the lowest priority): the press makes it the user's
/// download, and the clip plays once it lands.
#[gpui_kit::test]
fn a_press_takes_over_the_background_prefetch(cx: &mut TestAppContext) {
    let _scripts = scripts();
    let (app, mut vcx) = chat_with_clip(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, _| {
        let Some(session) = app.demo_session.as_mut() else {
            panic!("the demo session");
        };
        session.begin_download(quill::ids::FileId(CLIP_FILE));
    });
    update_clip(&app, vcx, true, false);
    assert!(!user_downloads_clip(&app, vcx), "only the prefetch runs");

    app.update(vcx, |app, cx| app.open_media_viewer(CHAT, CLIP, cx));
    settle(vcx);
    assert!(user_downloads_clip(&app, vcx), "the press asks as the user");
    poll(&app, vcx);
    update_clip(&app, vcx, false, true);
    poll(&app, vcx);
    assert!(viewer_plays(&app, vcx), "the clip plays once it lands");
}
