//! A turned photo's picture leaves memory once the viewer closes, not when
//! the next photo opens (needs the `demo-capture` feature for gpui-kit's
//! test support).
use super::super::navigation::wheel_ui_tests::{new_app, open, settle};
use crate::ui::app::QuillApp;
use gpui_kit::{Entity, RenderImage, TestAppContext, VisualTestContext};
use quill::ids::{FileId, MessageId};
use quill::media_viewer::ViewerOrientation;
use quill::telegram::envelope::{MessageContent, PhotoContent, PhotoSizeView};
use smallvec::SmallVec;
use std::path::PathBuf;
use std::sync::Arc;

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

#[gpui_kit::test]
fn closing_the_viewer_lets_go_of_a_turned_photo(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    add_photo(&app, vcx, 8901);
    open(&app, vcx, 8901);
    // What the decode of a turn hands back.
    let turned = Arc::new(RenderImage::new(SmallVec::from_buf([image::Frame::new(
        image::RgbaImage::new(4, 2),
    )])));
    let weak = Arc::downgrade(&turned);
    app.update_in(vcx, |app, _, cx| {
        let generation = app.viewer.reorient_gen;
        let target = ViewerOrientation {
            turns: 1,
            ..Default::default()
        };
        let path = PathBuf::from("turned.jpg");
        app.finish_viewer_reorient(generation, path, target, Some(turned), cx);
    });
    settle(vcx);
    assert!(weak.upgrade().is_some(), "the open viewer keeps it");

    app.update_in(vcx, |app, _, cx| app.close_media_viewer(cx));
    settle(vcx);
    assert!(weak.upgrade().is_none(), "nothing keeps it once closed");
}
