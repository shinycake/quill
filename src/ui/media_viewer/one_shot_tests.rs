//! The viewer's one-shots last as long as what armed them: a timestamp
//! link's start ends with the viewer, "Share at" ends with its picker, and
//! one window's viewer stopping leaves another window's parked clip
//! (needs the `demo-capture` feature for gpui-kit's test support).
use super::super::navigation::wheel_ui_tests::{add_photo, new_app, open};
use crate::ui::forward_stage::{ForwardStage, ShareAt};
use crate::ui::play_intent::{PlayIntent, PlayTarget};
use quill::composer::ForwardDraft;
use quill::ids::{ChatId, ChatMessage, FileId, MessageId, TopicId};

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
fn stepping_to_another_item_drops_a_timestamp_start(cx: &mut gpui_kit::TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    add_photo(&app, vcx, 8801);
    add_photo(&app, vcx, 8802);
    open(&app, vcx, 8801);
    let key = ChatMessage::new(ChatId(11), MessageId(8801));
    app.update(vcx, |app, cx| {
        app.viewer.pending_seek = Some((key, 45.0));
        // The call that armed it (an open, a quality change) keeps it.
        app.reset_viewer_item_state(cx);
        assert_eq!(app.viewer.pending_seek, Some((key, 45.0)));
        app.step_media_viewer(1, cx);
        assert_eq!(app.viewer.pending_seek, None);
    });
}

#[gpui_kit::test]
fn stepping_ends_a_pan_but_not_a_rest_on_a_control(cx: &mut gpui_kit::TestAppContext) {
    use crate::ui::viewer_parts::Pointer;
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    add_photo(&app, vcx, 8801);
    add_photo(&app, vcx, 8802);
    open(&app, vcx, 8801);
    app.update(vcx, |app, cx| {
        app.viewer.pointer = Pointer::Panning((1., 1.));
        app.step_media_viewer(1, cx);
        assert_eq!(app.viewer.pointer, Pointer::Away);
        app.viewer_note_pointer(true, cx);
        app.step_media_viewer(-1, cx);
        assert_eq!(app.viewer.pointer, Pointer::OverControls);
        app.close_media_viewer(cx);
        assert_eq!(app.viewer.pointer, Pointer::Away);
    });
}

#[gpui_kit::test]
fn closing_the_forward_picker_drops_share_at(cx: &mut gpui_kit::TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.share.forward = ForwardStage::Selecting(
            ForwardDraft::from_message(ChatId(11), TopicId::None, MessageId(101), false)
                .expect("a message to forward"),
        );
        assert!(app.share.forward.open_picker());
        app.share.forward.set_share_at(ShareAt {
            chat_id: ChatId(11),
            message_id: MessageId(101),
            secs: 83,
        });
        app.close_forward_picker(window, cx);
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

#[gpui_kit::test]
fn the_idle_clock_arms_no_timer_while_the_pointer_rests_on_a_control(
    cx: &mut gpui_kit::TestAppContext,
) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    add_photo(&app, vcx, 8801);
    open(&app, vcx, 8801);
    app.update(vcx, |app, cx| {
        app.viewer_note_pointer(true, cx);
        assert!(app.viewer.visibility.has_timer(), "the open armed it");
    });
    // Its turn comes with the pointer on a control: it ends, no re-arm.
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(3));
    vcx.run_until_parked();
    app.update(vcx, |app, cx| {
        assert!(!app.viewer.visibility.has_timer(), "held: nothing runs");
        // The pointer leaving the controls arms the real deadline.
        app.viewer_note_pointer(false, cx);
        assert!(app.viewer.visibility.has_timer());
    });
}

#[gpui_kit::test]
fn every_open_counts_once(cx: &mut gpui_kit::TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    add_photo(&app, vcx, 8801);
    let before = app.read_with(vcx, |app, _| app.viewer.state.opened());
    open(&app, vcx, 8801);
    let once = app.read_with(vcx, |app, _| app.viewer.state.opened());
    assert_ne!(once, before, "one open, one new count");
    app.update(vcx, |app, cx| app.close_media_viewer(cx));
    assert_eq!(app.read_with(vcx, |app, _| app.viewer.state.opened()), once);
}
