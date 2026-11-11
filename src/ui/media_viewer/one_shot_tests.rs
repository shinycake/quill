//! The viewer's one-shots last as long as what armed them: a timestamp
//! link's start ends with the viewer, "Share at" ends with its picker, and
//! one window's viewer stopping leaves another window's parked clip
//! (needs the `demo-capture` feature for gpui-kit's test support).
use super::super::navigation::wheel_ui_tests::{add_photo, new_app, open};
use crate::ui::play_intent::{PlayIntent, PlayTarget};
use quill::ids::{ChatId, ChatMessage, FileId, MessageId};

#[gpui_kit::test]
fn closing_the_viewer_drops_a_timestamp_start(cx: &mut gpui_kit::TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    add_photo(&app, vcx, 8801);
    open(&app, vcx, 8801);
    app.update(vcx, |app, cx| {
        app.viewer.pending_seek = Some((ChatMessage::new(ChatId(11), MessageId(8802)), 45.0));
        app.close_media_viewer(cx);
        assert_eq!(app.viewer.pending_seek, None);
    });
}

#[gpui_kit::test]
fn closing_the_forward_picker_drops_share_at(cx: &mut gpui_kit::TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        let draft = quill::composer::ForwardDraft::from_message(
            ChatId(11),
            Default::default(),
            MessageId(101),
            false,
        )
        .expect("a draft");
        app.share.forward = crate::ui::forward_stage::ForwardStage::Selecting(draft);
        app.open_forward_picker(window, cx);
        app.share
            .forward
            .set_share_at(crate::ui::forward_stage::ShareAt {
                chat_id: ChatId(11),
                message_id: MessageId(101),
                secs: 83,
            });
        app.close_forward_picker(window, cx);
        // Opening the box over the same message again holds no position.
        app.open_forward_picker(window, cx);
        assert_eq!(
            app.share.forward.take_share_at(ChatId(11), MessageId(101)),
            None
        );
    });
}

#[gpui_kit::test]
fn a_main_viewer_stop_keeps_a_chat_windows_parked_clip(cx: &mut gpui_kit::TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let theirs = PlayIntent {
        file: FileId(9),
        target: PlayTarget::Viewer {
            chat: ChatId(12),
            message: MessageId(5),
            window: Some(ChatId(12)),
        },
    };
    app.update(vcx, |app, _| {
        assert_eq!(app.windows.scope, None, "the main window is in place");
        app.playback.intent = Some(theirs.clone());
        app.stop_viewer_video();
        assert_eq!(app.playback.intent.as_ref(), Some(&theirs));
        // Its own window's stop drops it.
        app.windows.scope = Some(ChatId(12));
        app.stop_viewer_video();
        app.windows.scope = None;
        assert_eq!(app.playback.intent, None);
    });
}
