#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! A click on a message notification ends in the chat with the keys in its
//! composer, as in Telegram Desktop (`Manager::openNotificationMessage`).

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, FileId, MessageId};
use quill::locks::LockExt;
use quill::notify::NotificationAction;
use quill::telegram::envelope::{MessageContent, PhotoContent, PhotoSizeView};

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(cx: &mut TestAppContext, kind: &str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(kind);
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

/// A chat that is not the open one, and the first that can be written to.
fn other_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> ChatId {
    app.read_with(vcx, |app, _| {
        let open = app.open_chat_id();
        app.session()
            .into_iter()
            .flat_map(|session| session.chats.values())
            .filter(|chat| chat.can_post() && Some(chat.id) != open)
            .map(|chat| chat.id)
            .min_by_key(|id| id.0)
            .expect("a chat to click")
    })
}

fn click(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, chat: ChatId) {
    app.update(vcx, |app, cx| {
        let account = app.notification_account(None);
        app.delivery
            .notify_clicks
            .lock_or_recover()
            .push(quill::notify_accounts::ChatClick {
                account,
                chat_id: chat,
                action: NotificationAction::Open,
            });
        cx.notify();
    });
    frames(vcx);
}

fn composer_focused(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.update_in(vcx, |app, window, cx| {
        app.composer.read(cx).focus_handle(cx).is_focused(window)
    })
}

#[gpui_kit::test]
fn a_click_opens_the_chat_with_the_composer_focused(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, "ready-chats");
    let vcx = &mut vcx;
    let chat = other_chat(&app, vcx);
    assert!(!composer_focused(&app, vcx));
    click(&app, vcx, chat);
    assert_eq!(app.read_with(vcx, |app, _| app.open_chat_id()), Some(chat));
    assert!(composer_focused(&app, vcx));
}

#[gpui_kit::test]
fn behind_the_passcode_a_click_opens_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, "ready-chats");
    let vcx = &mut vcx;
    let chat = other_chat(&app, vcx);
    let before = app.update(vcx, |app, _| {
        app.account.passcode.locked = true;
        app.open_chat_id()
    });
    click(&app, vcx, chat);
    assert_eq!(app.read_with(vcx, |app, _| app.open_chat_id()), before);
    assert!(!composer_focused(&app, vcx));
}

#[gpui_kit::test]
fn a_dialog_over_the_chat_keeps_the_keys(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, "ready-chats");
    let vcx = &mut vcx;
    let chat = other_chat(&app, vcx);
    app.update(vcx, |app, cx| {
        app.payments.marketplace_open = true;
        cx.notify();
    });
    frames(vcx);
    click(&app, vcx, chat);
    assert!(app.read_with(vcx, |app, _| app.payments.marketplace_open));
    assert_eq!(app.read_with(vcx, |app, _| app.open_chat_id()), Some(chat));
    assert!(!composer_focused(&app, vcx));
}

#[gpui_kit::test]
fn a_click_closes_the_media_viewer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, "ready-privacy");
    let vcx = &mut vcx;
    let chat = other_chat(&app, vcx);
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        let history = session.histories.get_mut(&11).expect("history");
        let mut message = history.messages.get(&101).expect("a message").clone();
        message.id = MessageId(8901);
        message.content = MessageContent::Photo(PhotoContent {
            caption: String::new(),
            caption_entities: Vec::new(),
            show_caption_above_media: false,
            sizes: vec![PhotoSizeView {
                type_name: "m".into(),
                width: 100,
                height: 100,
                file_id: FileId(8901),
            }],
            is_secret: false,
            has_spoiler: false,
            has_stickers: false,
            minithumbnail: None,
        });
        history.messages.insert(8901, message);
        app.open_media_viewer(ChatId(11), MessageId(8901), cx);
    });
    assert!(app.read_with(vcx, |app, _| app.viewer.state.is_open()));
    click(&app, vcx, chat);
    assert!(!app.read_with(vcx, |app, _| app.viewer.state.is_open()));
    assert!(composer_focused(&app, vcx));
}
