//! Attaching media to the options of a new poll: a link, a sticker from the
//! panel, a picked photo, removal, and the rows shifting with their media.

use super::AttachTarget;
use crate::ui::app::QuillApp;
use crate::ui::dialogs::OptionMediaSlot;
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::FileId;
use quill::poll::PollOptionInput;
use std::path::PathBuf;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1000.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        Root::new(view, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    (app, VisualTestContext::from_window(handle.into(), cx))
}

fn slot(app: &QuillApp, index: usize) -> OptionMediaSlot {
    app.composer_ui.poll_dialog.as_ref().unwrap().attach[index]
        .media
        .clone()
}

#[gpui_kit::test]
fn a_link_is_attached_with_done_and_an_empty_one_is_refused(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_poll_dialog(window, cx);
        app.start_poll_option_link(AttachTarget::Row(1), window, cx);
        app.finish_poll_option_link(AttachTarget::Row(1), cx);
        assert_eq!(slot(app, 1), OptionMediaSlot::None);
        assert!(app.connection.status_note.contains("link"));

        let input = app.composer_ui.poll_dialog.as_ref().unwrap().attach[1]
            .link_input
            .clone()
            .unwrap();
        input.update(cx, |input, cx| {
            input.set_value("  https://example.com/tacos  ", window, cx)
        });
        app.finish_poll_option_link(AttachTarget::Row(1), cx);
        assert_eq!(
            slot(app, 1),
            OptionMediaSlot::Ready(PollOptionInput::Link {
                url: "https://example.com/tacos".into()
            })
        );
        let dialog = app.composer_ui.poll_dialog.as_ref().unwrap();
        assert!(dialog.attach[1].link_input.is_none());
        // The draft carries it for the second option once it has text.
        let draft = dialog.draft(cx);
        assert_eq!(
            draft.option_media[1],
            Some(PollOptionInput::Link {
                url: "https://example.com/tacos".into()
            })
        );
    });
}

#[gpui_kit::test]
fn a_sticker_chosen_in_the_panel_goes_to_the_option_not_the_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_poll_dialog(window, cx);
        // Nothing waits for a sticker: the pick is not taken.
        assert!(!app.route_poll_option_sticker(FileId(9), "x", (512, 512), cx));

        app.choose_poll_option_sticker(AttachTarget::Row(0), cx);
        assert!(app.sticker_panel_open());
        assert!(app.route_poll_option_sticker(FileId(9), "\u{1F600}", (512, 512), cx));
        assert_eq!(
            slot(app, 0),
            OptionMediaSlot::Ready(PollOptionInput::Sticker {
                file_id: 9,
                width: 512,
                height: 512,
                emoji: "\u{1F600}".into()
            })
        );
        assert!(!app.sticker_panel_open(), "the panel closes");
        // Only one sticker was waited for.
        assert!(!app.route_poll_option_sticker(FileId(10), "x", (1, 1), cx));
    });
}

#[gpui_kit::test]
fn a_picked_photo_is_read_then_ready_and_a_text_file_is_refused(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let photo =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots/fixtures/demo-thumb.png");
    let shown = photo.to_string_lossy().into_owned();
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_poll_dialog(window, cx);
        app.take_poll_option_file(AttachTarget::Row(0), PathBuf::from("/tmp/notes.txt"), cx);
        assert_eq!(slot(app, 0), OptionMediaSlot::None);
        assert!(app.connection.status_note.contains("photo or a video"));

        app.take_poll_option_file(AttachTarget::Row(0), photo, cx);
        assert_eq!(slot(app, 0), OptionMediaSlot::Preparing(shown.clone()));
        assert!(
            app.composer_ui
                .poll_dialog
                .as_ref()
                .unwrap()
                .media_preparing()
        );
    });
    vcx.run_until_parked();
    app.update_in(&mut vcx, |app, _, _| {
        assert_eq!(
            slot(app, 0),
            OptionMediaSlot::Ready(PollOptionInput::Photo { path: shown })
        );
        assert!(
            !app.composer_ui
                .poll_dialog
                .as_ref()
                .unwrap()
                .media_preparing()
        );
    });
}

#[gpui_kit::test]
fn removing_a_row_keeps_the_media_with_its_option(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_poll_dialog(window, cx);
        app.add_poll_option_row(window, cx);
        let link = |url: &str| OptionMediaSlot::Ready(PollOptionInput::Link { url: url.into() });
        {
            let dialog = app.composer_ui.poll_dialog.as_mut().unwrap();
            dialog.attach[0].media = link("a");
            dialog.attach[2].media = link("c");
            dialog.attach[2].menu_open = true;
        }
        app.remove_poll_option_row(1, cx);
        let dialog = app.composer_ui.poll_dialog.as_ref().unwrap();
        let media: Vec<_> = dialog.attach.iter().map(|a| a.media.clone()).collect();
        assert_eq!(media, vec![link("a"), link("c")]);
        assert!(dialog.attach[1].menu_open, "the open menu follows its row");
        app.clear_poll_option_media(AttachTarget::Row(0), cx);
        assert_eq!(slot(app, 0), OptionMediaSlot::None);
    });
}

#[gpui_kit::test]
fn the_option_added_to_a_sent_poll_takes_media_too(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_poll_add_option(quill::ids::ChatId(11), quill::ids::MessageId(1), window, cx);
        app.start_poll_option_link(AttachTarget::Added, window, cx);
        let input = app
            .message_ui
            .poll_add_option
            .as_ref()
            .unwrap()
            .attach
            .link_input
            .clone()
            .unwrap();
        input.update(cx, |input, cx| input.set_value("example.com", window, cx));
        app.finish_poll_option_link(AttachTarget::Added, cx);
        let attach = &app.message_ui.poll_add_option.as_ref().unwrap().attach;
        assert_eq!(
            attach.media.ready(),
            Some(&PollOptionInput::Link {
                url: "example.com".into()
            })
        );
        // A sticker choice on the added option is routed there, and only one
        // slot waits at a time.
        app.choose_poll_option_sticker(AttachTarget::Added, cx);
        assert!(app.route_poll_option_sticker(FileId(5), "x", (1, 1), cx));
        assert!(matches!(
            app.message_ui
                .poll_add_option
                .as_ref()
                .unwrap()
                .attach
                .media
                .ready(),
            Some(PollOptionInput::Sticker { file_id: 5, .. })
        ));
    });
}
