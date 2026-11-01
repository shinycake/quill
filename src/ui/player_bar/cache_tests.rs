//! The player bar's playlist is kept per state of the chat's messages and
//! gives what a fresh scan gives, and the bar's Previous / Next follow
//! it, over the `ready-audio` demo (two music files in chat 11, the
//! first playing). Nothing reaches Telegram.
use super::PlaylistCache;
use crate::ui::app::QuillApp;
use crate::ui::frame_clock::frames_over as frames;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::ChatId;
use quill::lang::{keys, tr};
use quill::state::RedrawNeed;
use quill::telegram::envelope::MessageContent;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const CHAT: ChatId = ChatId(11);

fn open_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-audio")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    (app, vcx)
}

fn apply(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, json: &str) {
    app.update(vcx, |app, _| {
        let session = app.demo_session.as_mut().expect("demo session");
        let seq = AtomicU64::new(session.last_seq + 1);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let owned = quill::telegram::client::copy_and_parse(json, &seq, &sink)
            .unwrap_or_else(|| panic!("parses: {json}"));
        session.apply(owned);
    });
}

/// Apply `json` as the poll does an update for a chat not on screen:
/// it redraws the chat list and nothing else, and pokes `wake::CHROME`.
fn apply_off_screen(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, json: &str) {
    app.update(vcx, |app, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        let seq = AtomicU64::new(session.last_seq + 1);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        let owned = quill::telegram::client::copy_and_parse(json, &seq, &sink)
            .unwrap_or_else(|| panic!("parses: {json}"));
        let need = quill::state::redraw_need(session, &owned.envelope);
        assert_eq!(need, RedrawNeed::ChatList, "only the chat list redraws");
        session.apply(owned);
        app.notify_chat_list(cx);
        // The poll pokes the chrome after what it applied.
        quill::wake::CHROME.poke();
    });
}

fn new_track(id: i64) -> String {
    format!(
        r#"{{"@type":"updateNewMessage","message":{{"id":{id},"chat_id":11,"is_outgoing":false,"content":{{"@type":"messageAudio","audio":{{"@type":"audio","duration":30,"title":"T","performer":"","file_name":"t.mp3","mime_type":"audio/mpeg","album_cover_minithumbnail":null,"album_cover_thumbnail":null,"external_album_covers":[],"audio":{{"@type":"file","id":{id},"size":10,"expected_size":10,"local":{{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0}},"remote":{{"@type":"remoteFile","id":"x","unique_id":"u","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}}}}}},"caption":{{"@type":"formattedText","text":"","entities":[]}}}}}}}}"#
    )
}

/// What the bar shows from the loaded messages, scanned fresh.
fn scanned(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<i64> {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.histories.get(&CHAT.0))
            .map(|h| {
                h.ordered()
                    .into_iter()
                    .filter(|m| matches!(m.content, MessageContent::Audio(_)))
                    .map(|m| m.id.0)
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn kept(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Rc<[i64]> {
    app.read_with(vcx, |app, _| app.playlist_audio_ids(CHAT))
}

#[gpui_kit::test]
fn the_playlist_follows_the_messages_and_is_kept_between(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    let first = kept(&app, vcx);
    assert_eq!(&*first, scanned(&app, vcx).as_slice());
    assert_eq!(&*first, &[801, 802]);
    assert!(
        Rc::ptr_eq(&first, &kept(&app, vcx)),
        "unchanged messages keep the list"
    );
    assert_eq!(
        app.read_with(vcx, |app, _| app.playlist_can_move()),
        (true, true)
    );

    apply(&app, vcx, &new_track(803));
    let grown = kept(&app, vcx);
    assert_eq!(&*grown, &[801, 802, 803]);
    assert_eq!(&*grown, scanned(&app, vcx).as_slice());

    apply(
        &app,
        vcx,
        r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[802,803],"is_permanent":true,"from_cache":false}"#,
    );
    let shrunk = kept(&app, vcx);
    assert_eq!(&*shrunk, &[801]);
    assert_eq!(&*shrunk, scanned(&app, vcx).as_slice());
    assert_eq!(
        app.read_with(vcx, |app, _| app.playlist_can_move()),
        (false, false),
        "one track has no neighbour"
    );
}

#[test]
fn the_cache_recomputes_for_another_chat_or_revision() {
    let cache = PlaylistCache::default();
    let mut computed = 0;
    let mut ids = |chat: i64, revision: u64, value: &[i64]| {
        cache.ids(chat, revision, || {
            computed += 1;
            value.to_vec()
        })
    };
    let a = ids(1, 10, &[1, 2]);
    assert!(Rc::ptr_eq(&a, &ids(1, 10, &[9])), "same chat and revision");
    assert_eq!(&*ids(1, 11, &[1, 2, 3]), &[1, 2, 3], "a new revision");
    assert_eq!(&*ids(2, 11, &[7]), &[7], "another chat");
    assert_eq!(&*ids(1, 11, &[4]), &[4], "back to the first chat");
    assert_eq!(computed, 4);
}

/// Whether the player bar's Next is disabled, from the accessibility tree.
fn next_disabled(vcx: &mut VisualTestContext) -> bool {
    let label = tr(keys::Q_AV_NEXT_TRACK);
    vcx.update(|window, _| {
        let tree = window.a11y_tree_snapshot().expect("a tree");
        let mut next = tree
            .nodes
            .iter()
            .filter(|node| node.label.as_deref() == Some(label));
        let node = next.next().expect("the bar shows Next");
        assert!(next.next().is_none(), "one Next");
        node.disabled
    })
}

/// With the song paused and another chat open, a song arriving in the
/// song's chat turns Next on. That message redraws only the chat list,
/// and a paused song's progress tick no longer redraws the bar.
#[gpui_kit::test]
fn next_turns_on_for_a_song_arriving_off_screen_while_paused(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, _| {
        window.activate_window();
        window.set_a11y_forced(true);
    });
    app.update(vcx, |app, cx| app.toggle_active_playback(cx));
    assert!(app.read_with(vcx, |app, _| !app.playback.is_playing()));
    // One song left, and another chat open.
    apply(
        &app,
        vcx,
        r#"{"@type":"updateDeleteMessages","chat_id":11,"message_ids":[802],"is_permanent":true,"from_cache":false}"#,
    );
    let other = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|session| {
                session
                    .ordered_chats()
                    .into_iter()
                    .map(|chat| chat.id)
                    .find(|id| *id != CHAT)
            })
            .expect("the demo lists another chat")
    });
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.select_listed_chat(other, window, cx));
    });
    let _ = frames(vcx, 1);
    assert!(next_disabled(vcx), "one song: nothing to go to");

    apply_off_screen(&app, vcx, &new_track(803));
    let _ = frames(vcx, 1);
    assert!(!next_disabled(vcx), "the new song is next");
}
