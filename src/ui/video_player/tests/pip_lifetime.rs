//! How long Picture-in-Picture lives, and where "back to the viewer"
//! lands: it outlives its message leaving the loaded history window and
//! reopens the viewer on it anyway; it closes on a real deletion, on lock
//! and on its own account's logout (not on a switch to another account);
//! a two-video album comes back to the video it was opened from; opening
//! the clip again from the chat continues the window's player.

use super::pip::{open_pip_on, pip_of, pip_player, settle};
use super::{Script, calls};
use crate::ui::app::QuillApp;
use crate::ui::demo::{demo_file_json, demo_media_allowlist};
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use quill::diagnostics::DiagnosticSink;
use quill::ids::{AccountKey, ChatId, FileId, MessageId};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

fn ready_script() -> Rc<RefCell<Script>> {
    Rc::new(RefCell::new(Script {
        ready: true,
        picture: true,
        ..Script::default()
    }))
}

/// Apply a TDLib update to the demo session.
fn apply_json(app: &mut QuillApp, json: &str) {
    let sink: Arc<dyn DiagnosticSink> = app.demo_ui.sink.clone();
    let owned = quill::telegram::client::copy_and_parse(json, &app.demo_ui.seq, &sink)
        .expect("a valid update");
    app.demo_session
        .as_mut()
        .expect("a demo session")
        .apply(owned);
}

/// The chat's loaded history window moves away (paging, a jump to a
/// date or a reply): its messages leave memory without being deleted.
fn unload_history(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update_in(vcx, |app, _, cx| {
        if let Some(history) = app
            .demo_session
            .as_mut()
            .and_then(|session| session.histories.get_mut(&11))
        {
            history.messages.clear();
        }
        cx.notify();
    });
    settle(vcx);
}

fn pip_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.viewer.pip.is_some())
}

/// Click the window's back-to-viewer button.
fn enlarge(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, cx: &mut TestAppContext) {
    let (window, view) = pip_of(app, vcx);
    drop(view);
    let mut pip = VisualTestContext::from_window(window.into(), cx);
    pip.run_until_parked();
    pip.update(|window, cx| {
        window.hover("pip-enlarge", cx);
        window.click("pip-enlarge", cx);
    });
    pip.run_until_parked();
    settle(vcx);
}

#[gpui_kit::test]
fn pip_outlives_the_loaded_history_and_closes_when_the_message_is_deleted(cx: &mut TestAppContext) {
    let script = ready_script();
    let (app, mut vcx, player) = open_pip_on(cx, &script, |_| {}, MessageId(204), 0);
    drop(player);
    let vcx = &mut vcx;
    unload_history(&app, vcx);
    assert!(pip_open(&app, vcx), "paging the history keeps it");
    let (_, view) = pip_of(&app, vcx);
    assert!(pip_player(&view, vcx).is_some());
    drop(view);
    assert!(!script.borrow().dropped);
    // A real deletion (the message was not even loaded) closes it.
    app.update_in(vcx, |app, _, cx| {
        apply_json(
            app,
            r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[204],"is_permanent":true,"from_cache":false}"#,
        );
        cx.notify();
    });
    settle(vcx);
    assert!(!pip_open(&app, vcx));
    assert!(script.borrow().dropped, "the player stopped");
}

#[gpui_kit::test]
fn back_to_the_viewer_works_when_the_message_is_no_longer_loaded(cx: &mut TestAppContext) {
    let script = ready_script();
    let (app, mut vcx, viewer_player) = open_pip_on(cx, &script, |_| {}, MessageId(204), 0);
    unload_history(&app, &mut vcx);
    calls(&script);
    enlarge(&app, &mut vcx, cx);
    app.read_with(&vcx, |app, _| {
        assert!(app.viewer.pip.is_none());
        let item = app.viewer.state.current().expect("the viewer is back");
        assert_eq!(item.message_id, MessageId(204));
        let player = app.viewer.player.as_ref().expect("it plays");
        assert!(player.same_as(&viewer_player));
    });
    assert!(!script.borrow().dropped);
    let back = calls(&script);
    assert!(!back.iter().any(|c| c.starts_with("seek")), "{back:?}");
}

#[gpui_kit::test]
fn locking_quill_closes_pip_without_a_main_window_render(cx: &mut TestAppContext) {
    let script = ready_script();
    let (app, mut vcx, player) = open_pip_on(cx, &script, |_| {}, MessageId(204), 0);
    drop(player);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        app.account.passcode.enabled = true;
        app.lock_by_passcode(cx);
    });
    // No refresh of the main window: the close is not a render's.
    vcx.run_until_parked();
    assert!(!pip_open(&app, vcx));
    assert!(script.borrow().dropped, "the player stopped");
}

#[gpui_kit::test]
fn switching_accounts_keeps_pip_and_its_own_logout_closes_it(cx: &mut TestAppContext) {
    let script = ready_script();
    let (app, mut vcx, player) = open_pip_on(cx, &script, |_| {}, MessageId(204), 0);
    drop(player);
    let vcx = &mut vcx;
    let original = app.read_with(vcx, |app, _| {
        app.session().map(|session| session.account.clone())
    });
    let original = original.expect("a demo account");
    // Another account is active now (tdesktop keeps the first one's
    // session, and its window).
    app.update_in(vcx, |app, _, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.account = AccountKey("another".into());
        }
        cx.notify();
    });
    settle(vcx);
    assert!(pip_open(&app, vcx), "a switch keeps it");
    // Another account's logout does nothing; its own closes it.
    app.update_in(vcx, |app, _, cx| {
        app.video_pip_account_ended(&AccountKey("another".into()), cx)
    });
    vcx.run_until_parked();
    assert!(pip_open(&app, vcx));
    app.update_in(vcx, |app, _, cx| app.video_pip_account_ended(&original, cx));
    vcx.run_until_parked();
    assert!(!pip_open(&app, vcx));
    assert!(script.borrow().dropped);
}

/// A bought paid-media album (message 205) with two videos, files 98 and
/// 99, both the demo clip.
fn album(app: &mut QuillApp) {
    let path = demo_media_allowlist()
        .join("demo-clip-12s.mp4")
        .to_string_lossy()
        .into_owned();
    let video = |id: i32| {
        format!(
            r#"{{"@type":"paidMediaVideo","video":{{"@type":"video","duration":12,"width":320,"height":180,"file_name":"clip.mp4","mime_type":"video/mp4","has_stickers":false,"supports_streaming":true,"minithumbnail":null,"thumbnail":null,"video":{file}}},"cover":null,"start_timestamp":0}}"#,
            file = demo_file_json(id, &path, true)
        )
    };
    let json = format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":205,"chat_id":11,"is_outgoing":false,"content":{{"@type":"messagePaidMedia","star_count":5,"media":[{first},{second}],"caption":{{"@type":"formattedText","text":"","entities":[]}},"show_caption_above_media":false}}}}}}"#,
        first = video(98),
        second = video(99)
    );
    apply_json(app, &json);
}

#[gpui_kit::test]
fn an_album_video_comes_back_to_its_own_item(cx: &mut TestAppContext) {
    let script = ready_script();
    let (app, mut vcx, viewer_player) = open_pip_on(cx, &script, album, MessageId(205), 1);
    assert!(pip_open(&app, &mut vcx), "the second video went to PiP");
    enlarge(&app, &mut vcx, cx);
    app.read_with(&vcx, |app, _| {
        let item = app.viewer.state.current().expect("the viewer is back");
        assert_eq!(item.message_id, MessageId(205));
        assert_eq!(item.play_file_id, Some(FileId(99)), "the second video");
        assert_eq!(app.viewer.state.current_offset(), 1);
        let player = app.viewer.player.as_ref().expect("it plays");
        assert!(player.same_as(&viewer_player));
    });
}

#[gpui_kit::test]
fn opening_the_clip_from_the_chat_continues_the_pip_player(cx: &mut TestAppContext) {
    let script = ready_script();
    let (app, mut vcx, viewer_player) = open_pip_on(cx, &script, |_| {}, MessageId(204), 0);
    let vcx = &mut vcx;
    calls(&script);
    app.update_in(vcx, |app, _, cx| {
        app.open_media_viewer(ChatId(11), MessageId(204), cx)
    });
    settle(vcx);
    app.read_with(vcx, |app, _| {
        assert!(app.viewer.pip.is_none(), "the window closed");
        let player = app.viewer.player.as_ref().expect("the viewer plays");
        assert!(
            player.same_as(&viewer_player),
            "the same player, no restart"
        );
    });
    assert!(!script.borrow().dropped);
    let calls = calls(&script);
    assert!(!calls.iter().any(|c| c.starts_with("seek")), "{calls:?}");
}
