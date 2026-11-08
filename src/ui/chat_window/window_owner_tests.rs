//! A surface opened from a chat window is drawn in that window, or opens
//! in the main window, which draws it.
#![cfg(feature = "demo-capture")]

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AnyWindowHandle, AppContext, Entity, TestAppContext, px, size};
use quill::ids::ChatId;
use std::cell::Cell;
use std::rc::Rc;

pub(super) const OTHER: ChatId = ChatId(12);

pub(super) fn new_app(
    cx: &mut TestAppContext,
    demo: &'static str,
) -> (Entity<QuillApp>, AnyWindowHandle) {
    cx.update(gpui_kit::init);
    let slot = Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(demo))));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let main: AnyWindowHandle = handle.into();
    cx.run_until_parked();
    (app, main)
}

pub(super) fn open_chat_window(
    app: &Entity<QuillApp>,
    main: AnyWindowHandle,
    cx: &mut TestAppContext,
) {
    cx.update_window(main, |_, window, cx| {
        app.update(cx, |app, cx| app.open_chat_in_new_window(OTHER, window, cx));
    })
    .expect("the main window");
    cx.run_until_parked();
}

/// The story viewer and the profile modal hand their open to the main
/// window: it runs there, with the main window's state in place.
#[gpui_kit::test]
fn a_main_window_surface_opened_in_a_chat_window_opens_in_the_main_window(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx, "ready-chats");
    open_chat_window(&app, main, cx);
    let ran_in: Rc<Cell<Option<Option<ChatId>>>> = Rc::default();
    let seen = ran_in.clone();
    let handed = app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, cx| {
            app.hand_to_main_window(cx, move |app, _, _| seen.set(Some(app.windows.scope)))
        })
    });
    assert_eq!(handed, Some(true));
    cx.run_until_parked();
    assert_eq!(ran_in.get(), Some(None), "ran as the main window");

    let in_main = app.update(cx, |app, cx| app.hand_to_main_window(cx, |_, _, _| {}));
    assert!(!in_main, "the main window goes on at once");
}

/// A picture decoded for an editor lands in the window that asked: a chat
/// window's tray or message edit opens its editor there.
#[gpui_kit::test]
fn a_decoded_picture_lands_in_the_window_that_asked(cx: &mut TestAppContext) {
    let (app, main) = new_app(cx, "ready-chats");
    open_chat_window(&app, main, cx);
    let landed: Rc<Cell<Option<Option<ChatId>>>> = Rc::default();
    let seen = landed.clone();
    app.update(cx, |app, cx| {
        app.in_window_scope(Some(OTHER), cx, |app, cx| {
            let path = std::path::PathBuf::from("/nonexistent/quill-test.png");
            app.decode_image_then(path, move |app, _, _| seen.set(Some(app.windows.scope)), cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(landed.get(), Some(Some(OTHER)));
}

/// A clip parked for a chat window's viewer or a GIF file pressed there
/// names that window, so its close drops it instead of the main window
/// starting another chat's clip with the same id.
#[gpui_kit::test]
fn a_parked_viewer_clip_or_gif_belongs_to_its_window(cx: &mut TestAppContext) {
    use crate::ui::play_intent::{PlayIntent, PlayTarget};
    use quill::ids::{FileId, MessageId};
    let (app, main) = new_app(cx, "ready-chats");
    open_chat_window(&app, main, cx);
    for target in [
        PlayTarget::Viewer {
            chat: ChatId(11),
            message: MessageId(7),
            window: Some(OTHER),
        },
        PlayTarget::Gif {
            message: MessageId(7),
            window: Some(OTHER),
        },
    ] {
        let dropped = app.update(cx, |app, _| {
            app.playback.intent = Some(PlayIntent {
                file: FileId(1),
                target,
            });
            app.drop_play_intent_in_window(OTHER);
            app.playback.intent.is_none()
        });
        assert!(dropped);
    }
}

/// A secret chat in its own window keeps that window out of capture,
/// whatever the main window shows.
#[gpui_kit::test]
fn a_secret_chats_window_blocks_capture_of_itself(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;
    use quill::ids::UserId;
    use quill::telegram::envelope::ChatKind;
    let (app, main) = new_app(cx, "ready-chats");
    app.update(cx, |app, _| {
        let chat = app
            .demo_session
            .as_mut()
            .and_then(|session| session.chats.get_mut(&OTHER.0));
        if let Some(chat) = chat {
            chat.kind = ChatKind::Secret {
                secret_chat_id: 9,
                user_id: UserId(OTHER.0),
            };
        }
    });
    open_chat_window(&app, main, cx);
    let window = cx
        .windows()
        .into_iter()
        .find(|handle| handle.window_id() != main.window_id())
        .expect("the chat window");
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("the chat window draws");
    let view = app
        .read_with(cx, |app, _| {
            app.windows
                .open
                .first()
                .and_then(|open| open.view.upgrade())
        })
        .expect("the window view");
    let (window_blocked, main_blocked) = app.update(cx, |app, cx| {
        (
            view.read(cx).is_capture_blocked(),
            app.frame.capture_blocked,
        )
    });
    assert!(window_blocked, "the secret chat window");
    assert!(!main_blocked, "the main window shows another chat");
}

/// A GIF plays in the row of the window that pressed it: the main window
/// showing the same message does not play it too, and closing the window
/// stops it.
#[gpui_kit::test]
fn a_gif_plays_in_its_window_and_stops_when_the_window_closes(cx: &mut TestAppContext) {
    use crate::ui::playback_state::{ChatMessage, PlayingGif};
    use quill::ids::MessageId;
    let (app, main) = new_app(cx, "ready-chats");
    open_chat_window(&app, main, cx);
    let message = MessageId(40);
    let (here, in_main) = app.update(cx, |app, cx| {
        app.playback.playing_animation = Some(PlayingGif {
            window: Some(OTHER),
            message: ChatMessage::new(OTHER, message),
        });
        let here = app.in_window_scope(Some(OTHER), cx, |app, _| app.gif_playing(OTHER, message));
        (here, app.gif_playing(OTHER, message))
    });
    assert_eq!(here, Some(true), "the chat window's row plays it");
    assert!(!in_main, "the main window's row does not");
    app.update(cx, |app, cx| app.forget_chat_window(OTHER, cx));
    let playing = app.read_with(cx, |app, _| app.playback.playing_animation.is_some());
    assert!(!playing, "the window's GIF stopped with it");
}
