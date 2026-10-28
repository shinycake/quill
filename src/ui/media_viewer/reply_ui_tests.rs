//! The viewer's Reply button: shown for a sent message of the open chat
//! while it shows its composer, it closes the viewer and arms the composer's reply
//! (needs the `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, FileId, MessageId};
use quill::telegram::envelope::{MessageContent, PhotoContent, PhotoSizeView};

const CHAT: ChatId = ChatId(11);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
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

fn add_photo(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: i64, pending: bool) {
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        session.open_chat(CHAT);
        let history = session.histories.get_mut(&CHAT.0).expect("history");
        let mut message = history.messages.get(&101).expect("a message").clone();
        message.id = MessageId(id);
        message.pending = pending;
        message.content = MessageContent::Photo(PhotoContent {
            caption: String::new(),
            caption_entities: Vec::new(),
            show_caption_above_media: false,
            sizes: vec![PhotoSizeView {
                type_name: "m".into(),
                width: 100,
                height: 100,
                file_id: FileId(id as i32),
            }],
            is_secret: false,
            has_spoiler: false,
            has_stickers: false,
            minithumbnail: None,
        });
        history.messages.insert(id, message);
        cx.notify();
    });
}

fn open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: i64) {
    app.update_in(vcx, |app, _, cx| {
        app.open_media_viewer(CHAT, MessageId(id), cx);
    });
    vcx.run_until_parked();
}

fn reply_button(vcx: &mut VisualTestContext, id: i64) -> bool {
    vcx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(("media-viewer-reply", id as u64)).is_some()
    })
}

#[gpui_kit::test]
fn reply_closes_the_viewer_and_arms_the_composer(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    add_photo(&app, vcx, 8901, false);
    open(&app, vcx, 8901);
    assert!(reply_button(vcx, 8901), "a sent photo offers Reply");
    vcx.update(|window, cx| window.click(("media-viewer-reply", 8901u64), cx));
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(!app.viewer.state.is_open(), "Reply closes the viewer");
        let reply = app.composer_ui.reply().expect("a reply");
        assert_eq!((reply.chat_id, reply.message_id), (CHAT, MessageId(8901)));
    });
}

#[gpui_kit::test]
fn an_unsent_photo_offers_no_reply(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    add_photo(&app, vcx, 8902, true);
    open(&app, vcx, 8902);
    assert!(app.read_with(vcx, |app, _| app.viewer.state.is_open()));
    assert!(!reply_button(vcx, 8902));
}

#[gpui_kit::test]
fn a_chat_without_write_rights_offers_no_reply(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    add_photo(&app, vcx, 8903, false);
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        let chat = session.chats.get_mut(&CHAT.0).expect("chat");
        chat.kind = quill::telegram::envelope::ChatKind::Supergroup {
            supergroup_id: 1,
            is_channel: true,
        };
        chat.my_member_status = None;
        cx.notify();
    });
    open(&app, vcx, 8903);
    assert!(app.read_with(vcx, |app, _| app.viewer.state.is_open()));
    assert!(
        !reply_button(vcx, 8903),
        "a reader of a channel can't reply"
    );
}
