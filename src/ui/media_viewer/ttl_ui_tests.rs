//! A timed photo opens alone in the viewer, starts its countdown and keeps
//! Save and Forward away (needs the `demo-capture` feature for gpui-kit's
//! test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, FileId, MessageId};
use quill::telegram::envelope::{
    MessageContent, MessageSelfDestruct, PhotoContent, PhotoSizeView, SelfDestructKind,
};

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

fn add_photo(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    id: i64,
    kind: SelfDestructKind,
    expires_in_ms: i64,
) {
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        let history = session.histories.get_mut(&11).expect("history");
        let mut message = history.messages.get(&101).expect("a message").clone();
        message.id = MessageId(id);
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
            is_secret: true,
            has_spoiler: false,
            has_stickers: false,
            minithumbnail: None,
        });
        message.self_destruct = Some(MessageSelfDestruct {
            kind,
            expires_in_ms,
            fetched_at_ms: quill::state::unix_ms_now(),
        });
        history.messages.insert(id, message);
        cx.notify();
    });
}

#[gpui_kit::test]
fn a_timed_photo_opens_alone_and_shows_its_countdown(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    add_photo(&app, vcx, 8801, SelfDestructKind::Timer { secs: 10 }, 6_500);
    add_photo(&app, vcx, 8802, SelfDestructKind::Immediately, 0);
    // View-once media stays closed.
    app.update_in(vcx, |app, _, cx| {
        app.open_media_viewer(ChatId(11), MessageId(8802), cx);
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| assert!(!app.viewer.state.is_open()));
    // A timed photo opens by itself with its countdown running.
    app.update_in(vcx, |app, _, cx| {
        app.open_media_viewer(ChatId(11), MessageId(8801), cx);
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(app.viewer.state.is_open());
        assert_eq!(app.viewer.state.position(), Some((1, 1)));
        assert!(app.viewer_is_timed());
        let ttl = app.viewer_ttl().expect("countdown");
        assert_eq!(ttl.total_secs, 10);
        assert!(ttl.is_running());
        assert!(
            ttl.date_suffix()
                .is_some_and(|s| s.starts_with("Disappears in "))
        );
    });
}
