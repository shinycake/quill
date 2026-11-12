//! A click on the picture beside a poll option.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, FileId, MessageId};
use quill::media_viewer::ViewerSource;
use quill::telegram::envelope::{MessageContent, PhotoContent, PhotoSizeView, PollOptionMedia};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-showcase");
    let handle = cx.open_window(size(px(1000.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| crate::ui::shell::QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    (app, VisualTestContext::from_window(handle.into(), cx))
}

/// The first poll of the demo session, with media on its first options.
fn poll_with_media(app: &mut QuillApp, media: [PollOptionMedia; 2]) -> (ChatId, MessageId) {
    let session = app.demo_session.as_mut().expect("demo session");
    for history in session.histories.values_mut() {
        for message in history.messages.values_mut() {
            if let MessageContent::Poll(poll) = &mut message.content {
                for (option, media) in poll.poll.options.iter_mut().zip(media) {
                    option.media = Some(media);
                }
                return (message.chat_id, message.id);
            }
        }
    }
    panic!("the showcase has a poll");
}

fn photo() -> PollOptionMedia {
    PollOptionMedia::Photo(PhotoContent {
        caption: String::new(),
        caption_entities: Vec::new(),
        show_caption_above_media: false,
        sizes: vec![PhotoSizeView {
            type_name: "x".into(),
            width: 800,
            height: 600,
            file_id: FileId(7001),
        }],
        is_secret: false,
        has_spoiler: false,
        has_stickers: false,
        minithumbnail: None,
    })
}

#[gpui_kit::test]
fn a_photo_opens_in_the_viewer_without_the_message_actions(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        let link = PollOptionMedia::Link {
            url: "https://example.com/tacos".into(),
            photo: None,
        };
        let (chat, message) = poll_with_media(app, [photo(), link]);
        app.open_poll_option_media(chat, message, 0, cx);
        assert_eq!(app.viewer.state.source(), ViewerSource::Poll);
        let item = app.viewer.state.current().expect("the viewer is open");
        assert_eq!((item.chat_id, item.message_id), (chat, message));
        assert_eq!(item.download_file_id, FileId(7001));
        assert_eq!(app.viewer.state.len(), 1, "one picture, no paging");
        app.close_media_viewer(cx);

        // A link asks first, with its address.
        app.open_poll_option_media(chat, message, 1, cx);
        let confirm = app.message_ui.open_link_confirm.clone().expect("asks");
        assert_eq!(confirm.url, "https://example.com/tacos");
        assert!(confirm.from_poll_option, "tdesktop's Open Link box");
        assert!(!app.viewer.state.is_open());

        // An option without media, or past the end, does nothing.
        app.message_ui.open_link_confirm = None;
        app.open_poll_option_media(chat, message, 9, cx);
        assert!(app.message_ui.open_link_confirm.is_none());
    });
}

/// Sweep r4-08 round 3: a refusal is dropped by the next Add, so it does
/// not stay under the field once the text was fixed.
#[gpui_kit::test]
fn a_new_add_clears_the_last_refusal(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        let (chat, message) = poll_with_media(app, [photo(), photo()]);
        app.open_poll_add_option(chat, message, window, cx);
        app.submit_poll_add_option(cx);
        let refused = |app: &QuillApp| {
            app.message_ui
                .poll_add_option
                .as_ref()
                .and_then(|panel| panel.error())
        };
        assert!(refused(app).is_some(), "an empty option is refused");
        let Some(input) = app
            .message_ui
            .poll_add_option
            .as_ref()
            .map(|p| p.input.clone())
        else {
            return;
        };
        input.update(cx, |input, cx| {
            input.set_value("A brand new option", window, cx)
        });
        // A video still being read keeps the panel open past the judgement,
        // so an empty refusal below is the cleared one, not a closed panel.
        if let Some(panel) = app.message_ui.poll_add_option.as_mut() {
            panel.attach.media = crate::ui::dialogs::OptionMediaSlot::Preparing(
                crate::ui::dialogs::Preparing::new("clip.mp4".into(), gpui_kit::Task::ready(())),
            );
        }
        app.submit_poll_add_option(cx);
        assert!(
            app.message_ui.poll_add_option.is_some(),
            "the panel waits for the video"
        );
        assert!(refused(app).is_none(), "the fixed text is judged anew");
    });
}
