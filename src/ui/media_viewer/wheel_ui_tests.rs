//! A wheel notch over the viewer moves to the next item, over the
//! picture and over the black around it; with Cmd (Ctrl) it zooms
//! instead, and the last item stays put (needs the `demo-capture`
//! feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, Modifiers, Pixels, Point, ScrollDelta, ScrollWheelEvent};
use gpui_kit::{TestAppContext, VisualTestContext, point, px, size};
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

fn add_photo(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: i64) {
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
            is_secret: false,
            has_spoiler: false,
            has_stickers: false,
            minithumbnail: None,
        });
        history.messages.insert(id, message);
        cx.notify();
    });
}

fn settle(vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
    }
}

fn open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: i64) {
    app.update_in(vcx, |app, _, cx| {
        app.privacy.open = false;
        app.open_media_viewer(ChatId(11), MessageId(id), cx);
    });
    settle(vcx);
}

fn notch(vcx: &mut VisualTestContext, at: Point<Pixels>, lines: f32, modifiers: Modifiers) {
    vcx.simulate_mouse_move(at, None, Modifiers::default());
    settle(vcx);
    vcx.simulate_event(ScrollWheelEvent {
        position: at,
        delta: ScrollDelta::Lines(point(0., lines)),
        modifiers,
        ..Default::default()
    });
    settle(vcx);
}

fn current(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> i64 {
    app.read_with(vcx, |app, _| {
        app.viewer.state.current().expect("an item").message_id.0
    })
}

/// The picture's center and a point on the black left of it.
fn targets(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> (Point<Pixels>, Point<Pixels>) {
    let (bounds, _) = app
        .update_in(vcx, |app, window, cx| app.viewer_media_bounds(window, cx))
        .expect("the media box");
    let center = bounds.center();
    assert!(f32::from(bounds.left()) > 200., "room left of the picture");
    (center, point(bounds.left() - px(60.), center.y))
}

#[gpui_kit::test]
fn a_wheel_notch_moves_to_the_next_item_and_cmd_wheel_zooms(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    for id in [8901, 8902, 8903] {
        add_photo(&app, vcx, id);
    }
    open(&app, vcx, 8901);
    let (picture, backdrop) = targets(&app, vcx);

    notch(vcx, picture, -1., Modifiers::default());
    assert_eq!(current(&app, vcx), 8902);
    notch(vcx, backdrop, -1., Modifiers::default());
    assert_eq!(current(&app, vcx), 8903);
    notch(vcx, picture, -1., Modifiers::default());
    assert_eq!(current(&app, vcx), 8903, "the last item stays put");
    notch(vcx, picture, 1., Modifiers::default());
    assert_eq!(current(&app, vcx), 8902);

    notch(vcx, picture, 1., Modifiers::secondary_key());
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.viewer.state.current().map(|i| i.message_id.0),
            Some(8902)
        );
        assert!(app.viewer.zoom.is_zoomed());
    });
    notch(vcx, picture, -2., Modifiers::default());
    app.read_with(vcx, |app, _| {
        assert_eq!(
            app.viewer.state.current().map(|i| i.message_id.0),
            Some(8902),
            "zoomed in, a plain notch zooms instead of moving"
        );
        assert!(!app.viewer.zoom.is_zoomed(), "back at fit");
    });
}
