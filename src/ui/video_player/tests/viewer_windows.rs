//! A chat window's media viewer on scripted players while the main window
//! shows another chat: a Space hold ends when that window goes to the
//! background, and a clip it parked for its download is not cancelled by
//! the main window's viewer starting one of its own.

use super::{Script, queue_backend};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::time::Duration;

const CHAT: ChatId = ChatId(11);
/// Another chat of the demo: the main window shows it.
const OTHER: ChatId = ChatId(12);
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

/// Backends for every player the test may open.
fn scripts() -> Vec<Rc<RefCell<Script>>> {
    (0..50)
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

/// The demo clip in `CHAT`, its file local or not; the main window shows
/// `OTHER` and `CHAT` has a window of its own, returned.
fn chat_window_with_clip(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    local: bool,
) -> AnyWindowHandle {
    vcx.update(|window, _| window.activate_window());
    app.update(vcx, |app, _| {
        let Some(session) = app.demo_session.as_mut() else {
            panic!("the demo session");
        };
        app.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        crate::ui::demos::video_fixtures::apply_ready_video_viewer(
            session,
            &app.demo_ui.sink,
            &app.demo_ui.seq,
        );
        session.media.files.update(CLIP_FILE, |file| {
            file.local.is_downloading_completed = local;
            file.local.is_downloading_active = false;
        });
        if let Some(chat) = session.chats.get_mut(&CHAT.0) {
            chat.unread_count = 0;
        }
    });
    settle(vcx);
    app.update_in(vcx, |app, window, cx| {
        app.open_chat_window_at(CHAT, None, window, cx)
    });
    vcx.run_until_parked();
    app.update_in(vcx, |app, window, cx| {
        app.select_listed_chat(OTHER, window, cx)
    });
    settle(vcx);
    let main = vcx.update(|window, _| window.window_handle().window_id());
    vcx.windows()
        .into_iter()
        .find(|handle| handle.window_id() != main)
        .expect("the chat window opened")
}

/// Open the clip in the chat window's viewer.
fn open_in_chat_window(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |app, cx| {
        app.in_window_scope(Some(CHAT), cx, |app, cx| {
            app.open_media_viewer(CHAT, CLIP, cx);
        })
        .expect("the chat window's scope");
    });
    settle(vcx);
}

/// The chat window's viewer plays at 2x on a Space hold, then that window
/// goes to the background before the key-up: the key-up goes to another
/// window, so the hold ends there and then.
#[gpui_kit::test]
fn a_chat_windows_hold_ends_when_it_goes_to_the_background(cx: &mut TestAppContext) {
    let _scripts = scripts();
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let chat_window = chat_window_with_clip(&app, vcx, true);
    let mut wcx = VisualTestContext::from_window(chat_window, cx);
    wcx.update(|window, _| window.activate_window());
    wcx.run_until_parked();
    open_in_chat_window(&app, vcx);
    let boosted = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.update(vcx, |app, cx| {
            app.in_window_scope(Some(CHAT), cx, |app, _| {
                app.viewer.controls.boost.is_active()
            })
            .expect("the chat window's scope")
        })
    };
    app.update(vcx, |app, cx| {
        app.in_window_scope(Some(CHAT), cx, |app, cx| {
            assert!(app.viewer.player.is_some(), "the clip plays in its window");
            let now = cx.background_executor().now();
            let _ = app.viewer.controls.boost.press(now);
            let later = now + Duration::from_millis(quill::media_viewer::SPEED_BOOST_HOLD_MS + 50);
            assert!(app.viewer.controls.boost.tick(later), "the hold boosts");
        })
        .expect("the chat window's scope");
    });
    assert!(boosted(&app, vcx));

    wcx.deactivate_window();
    wcx.run_until_parked();
    assert!(
        !boosted(&app, vcx),
        "a window in the background ends its hold"
    );
}

/// The chat window's viewer parks its clip until the file lands; the
/// main window's viewer then opens a local GIF, which plays at once. The
/// parked clip is the chat window's and still waits.
#[gpui_kit::test]
fn the_main_viewer_playing_leaves_a_chat_windows_parked_clip(cx: &mut TestAppContext) {
    let _scripts = scripts();
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let _chat_window = chat_window_with_clip(&app, vcx, false);
    open_in_chat_window(&app, vcx);
    let parked = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.playback
                .intent
                .as_ref()
                .map(|intent| intent.target.window())
        })
    };
    assert_eq!(
        parked(&app, vcx),
        Some(Some(CHAT)),
        "the chat window's clip waits for its download"
    );

    const GIF: (i64, i32) = (9301, 9302);
    app.update(vcx, |app, cx| {
        let Some(session) = app.demo_session.as_mut() else {
            panic!("the demo session");
        };
        app.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
        crate::ui::demos::video_fixtures::apply_clip_animation(
            session,
            &app.demo_ui.sink,
            &app.demo_ui.seq,
            GIF,
            "video/mp4",
        );
        app.open_media_viewer(CHAT, MessageId(GIF.0), cx);
    });
    settle(vcx);
    assert!(
        app.read_with(vcx, |app, _| app.viewer.player.is_some()),
        "the main window's GIF plays"
    );
    assert_eq!(
        parked(&app, vcx),
        Some(Some(CHAT)),
        "the chat window's clip still waits"
    );
}
