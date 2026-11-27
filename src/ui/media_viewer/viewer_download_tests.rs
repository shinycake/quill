//! The viewer asks for a photo's largest size even when a smaller one is
//! already local: the chat bubble downloads a small size first, and the
//! viewer used to stay on it, blurry (issue 1507). Needs the
//! `demo-capture` feature for gpui-kit's test support.
use super::wheel_ui_tests::{new_app, open};
use crate::ui::demo::demo_media_allowlist;
use quill::ids::{FileId, MessageId};
use quill::telegram::envelope::{
    LocalFileState, MessageContent, ParsedFile, PhotoContent, PhotoSizeView,
};

const SMALL: i32 = 9101;
const LARGE: i32 = 9102;

fn size(type_name: &str, width: i32, file_id: i32) -> PhotoSizeView {
    PhotoSizeView {
        type_name: type_name.into(),
        width,
        height: width,
        file_id: FileId(file_id),
    }
}

#[gpui_kit::test]
fn a_photo_with_only_its_small_size_local_downloads_the_largest(cx: &mut gpui_kit::TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let small_path = demo_media_allowlist().join("demo-gif-still-photo.jpg");
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        session.media.files.insert(ParsedFile {
            id: FileId(SMALL),
            size: 100,
            expected_size: 100,
            local: LocalFileState::from_parts(small_path.to_string_lossy(), true, false, true, 100),
        });
        let history = session.histories.get_mut(&11).expect("history");
        let mut message = history
            .ordered()
            .into_iter()
            .next()
            .expect("a message")
            .clone();
        message.id = MessageId(9100);
        message.content = MessageContent::Photo(PhotoContent {
            caption: String::new(),
            caption_entities: Vec::new(),
            show_caption_above_media: false,
            sizes: vec![size("m", 320, SMALL), size("y", 1280, LARGE)],
            is_secret: false,
            has_spoiler: false,
            has_stickers: false,
            minithumbnail: None,
        });
        history.messages.insert(9100, message);
        cx.notify();
    });
    open(&app, vcx, 9100);
    app.update(vcx, |app, _| {
        let session = app.demo_session.as_ref().expect("demo session");
        assert!(session.media.downloads.is_requested(LARGE));
        assert!(!session.media.downloads.is_requested(SMALL));
    });
}
