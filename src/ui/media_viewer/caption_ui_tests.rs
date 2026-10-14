//! A long caption collapses in the viewer, expands on "Show more", and
//! Escape collapses it before closing the viewer (needs the
//! `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, FileId, MessageId};
use quill::telegram::envelope::{MessageContent, PhotoContent, PhotoSizeView};

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

fn add_photo(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: i64, caption: &str) {
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        let history = session.histories.get_mut(&11).expect("history");
        let mut message = history.messages.get(&101).expect("a message").clone();
        message.id = MessageId(id);
        message.content = MessageContent::Photo(PhotoContent {
            caption: caption.to_string(),
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

/// Draw a few frames so the caption is laid out and measured.
fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
    }
}

#[gpui_kit::test]
fn a_long_caption_collapses_and_expands(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let long = "A long trail report. ".repeat(400);
    add_photo(&app, vcx, 8901, &long);
    add_photo(&app, vcx, 8902, "Short caption");
    app.update_in(vcx, |app, _, cx| {
        app.open_media_viewer(ChatId(11), MessageId(8901), cx);
    });
    settle(vcx);
    // Measured taller than its collapsed height: cut, not expanded.
    app.read_with(vcx, |app, _| {
        let caption = &app.viewer.extra.caption;
        assert_eq!(caption.key, Some((11, 8901)));
        assert!(caption.content_height() > 400.);
        assert!(!caption.expanded());
    });
    app.update_in(vcx, |app, _, cx| app.toggle_viewer_caption(cx));
    settle(vcx);
    app.read_with(vcx, |app, _| assert!(app.viewer.extra.caption.expanded()));
    // Escape collapses first and keeps the viewer open.
    app.update_in(vcx, |app, _, cx| {
        assert!(app.close_media_viewer_on_escape(cx));
    });
    app.read_with(vcx, |app, _| {
        assert!(app.viewer.state.is_open());
        assert!(!app.viewer.extra.caption.expanded());
    });
    // Another item starts collapsed.
    app.update_in(vcx, |app, _, cx| app.toggle_viewer_caption(cx));
    app.update_in(vcx, |app, _, cx| {
        app.open_media_viewer(ChatId(11), MessageId(8902), cx);
    });
    settle(vcx);
    app.read_with(vcx, |app, _| {
        let caption = &app.viewer.extra.caption;
        assert_eq!(caption.key, Some((11, 8902)));
        assert!(!caption.expanded());
    });
    // With nothing expanded, Escape closes the viewer.
    app.update_in(vcx, |app, _, cx| {
        assert!(app.close_media_viewer_on_escape(cx));
    });
    app.read_with(vcx, |app, _| assert!(!app.viewer.state.is_open()));
    // Closed while expanded (the close button), the caption reopens
    // collapsed.
    app.update_in(vcx, |app, _, cx| {
        app.open_media_viewer(ChatId(11), MessageId(8901), cx);
    });
    settle(vcx);
    app.update_in(vcx, |app, _, cx| {
        app.toggle_viewer_caption(cx);
        app.close_media_viewer(cx);
        app.open_media_viewer(ChatId(11), MessageId(8901), cx);
    });
    settle(vcx);
    app.read_with(vcx, |app, _| assert!(!app.viewer.extra.caption.expanded()));
}
