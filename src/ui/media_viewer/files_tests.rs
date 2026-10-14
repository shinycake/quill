//! A downloaded HEIC sent as a file opens in the viewer, which pages over
//! the chat's files and shows a card for a file that is not a picture
//! (needs the `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, FileId, MessageId};
use quill::media_viewer::MediaViewerKind;

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
                Some(ScreenshotDemo::named("ready-image-files")),
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

fn card_shown(app: &QuillApp) -> bool {
    let session = app.session().expect("demo session");
    let item = app.viewer.state.current().expect("an item").clone();
    app.viewer_file_card(&item, &session.media.files, &session.media.downloading)
        .is_some()
}

#[gpui_kit::test]
fn a_heic_file_opens_in_the_viewer_and_pages_over_files(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    // Still on the server: the click downloads instead of opening.
    app.update_in(vcx, |app, _, cx| {
        app.open_document(ChatId(11), MessageId(902), FileId(913), cx);
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| assert!(!app.viewer.state.is_open()));
    // Downloaded: the picture opens in the viewer, among the chat's files.
    app.update_in(vcx, |app, _, cx| {
        app.open_document(ChatId(11), MessageId(901), FileId(911), cx);
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        assert!(app.viewer.state.is_open());
        assert_eq!(app.viewer.state.position(), Some((1, 3)));
        let item = app.viewer.state.current().expect("an item");
        assert_eq!(item.kind, MediaViewerKind::ImageFile);
        assert_eq!(item.file_name.as_deref(), Some("IMG_2041.HEIC"));
        assert!(!card_shown(app));
    });
    // The text file further on is a card.
    app.update_in(vcx, |app, _, cx| {
        app.viewer.state.next();
        app.viewer.state.next();
        cx.notify();
    });
    vcx.run_until_parked();
    app.read_with(vcx, |app, _| {
        let item = app.viewer.state.current().expect("an item");
        assert_eq!(item.kind, MediaViewerKind::File);
        assert!(card_shown(app));
    });
}
