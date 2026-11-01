use crate::ui::app::QuillApp;
use crate::ui::history_row::HistoryRow;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::OutboxReceipt;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

fn new_app(cx: &mut TestAppContext, demo: &str) -> (Entity<QuillApp>, VisualTestContext) {
    // Default settings, not the saved window state (sidebar width) of the
    // app installed on this machine.
    quill::settings::use_isolated_app_root(quill::scratch::process_dir("quill-rows-cache-ui"));
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(demo);
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn frames(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

fn apply(app: &Entity<QuillApp>, cx: &mut VisualTestContext, updates: &[&str]) {
    app.update(cx, |app, cx| {
        let session = app.demo_session.as_mut().expect("demo session");
        let seq = AtomicU64::new(session.last_seq + 1);
        let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
        for json in updates {
            let owned = quill::telegram::client::copy_and_parse(json, &seq, &sink)
                .unwrap_or_else(|| panic!("parses: {json}"));
            session.apply(owned);
        }
        cx.notify();
    });
    frames(cx);
}

const GROUP: i64 = 31;

fn open_group(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    apply(
        app,
        cx,
        &[
            r#"{"@type":"updateUser","user":{"id":21,"first_name":"Ann","last_name":"","type":{"@type":"userTypeRegular"}}}"#,
            r#"{"@type":"updateUser","user":{"id":22,"first_name":"Bob","last_name":"","type":{"@type":"userTypeRegular"}}}"#,
            r#"{"@type":"updateNewChat","chat":{"id":31,"title":"Team","type":{"@type":"chatTypeBasicGroup","basic_group_id":31},"unread_count":0,"last_read_inbox_message_id":203,"last_read_outbox_message_id":0}}"#,
            r#"{"@type":"updateChatPosition","chat_id":31,"position":{"@type":"chatPosition","list":{"@type":"chatListMain"},"order":"40","is_pinned":false}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":201,"chat_id":31,"sender_id":{"@type":"messageSenderUser","user_id":21},"is_outgoing":false,"date":1790631720,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Morning.","entities":[]}}}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":202,"chat_id":31,"sender_id":{"@type":"messageSenderUser","user_id":22},"is_outgoing":false,"date":1790631780,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hi all.","entities":[]}}}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":203,"chat_id":31,"is_outgoing":true,"date":1790631840,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"On my way.","entities":[]}}}}"#,
        ],
    );
    app.update(cx, |app, cx| {
        app.demo_session
            .as_mut()
            .expect("demo session")
            .open_chat(ChatId(GROUP));
        cx.notify();
    });
    frames(cx);
}

fn counts(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> (u64, u64) {
    app.read_with(cx, |app, _| {
        (
            app.history.row_stats.rebuilds,
            app.history.row_stats.rows_patched,
        )
    })
}

fn single(app: &QuillApp, id: i64) -> Option<&crate::ui::history_row::HistoryRowInputs> {
    app.history.rows.iter().find_map(|row| match row {
        HistoryRow::Single(inputs) if inputs.message.id == MessageId(id) => Some(inputs.as_ref()),
        _ => None,
    })
}

fn sender_name(app: &Entity<QuillApp>, cx: &mut VisualTestContext, id: i64) -> Option<String> {
    app.read_with(cx, |app, _| {
        single(app, id).and_then(|row| row.chrome.sender.as_ref().map(|s| s.name.clone()))
    })
}

#[gpui_kit::test]
fn redrawing_with_nothing_changed_reuses_the_rows(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    let before = counts(&app, vcx);
    for _ in 0..3 {
        app.update(vcx, |_, cx| cx.notify());
        frames(vcx);
    }
    assert_eq!(counts(&app, vcx), before);
}

#[gpui_kit::test]
fn a_sender_rename_patches_only_that_senders_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    assert_eq!(sender_name(&app, vcx, 201).as_deref(), Some("Ann"));
    let (built, patched) = counts(&app, vcx);
    apply(
        &app,
        vcx,
        &[
            r#"{"@type":"updateUser","user":{"id":21,"first_name":"Annie","last_name":"","type":{"@type":"userTypeRegular"}}}"#,
        ],
    );
    assert_eq!(sender_name(&app, vcx, 201).as_deref(), Some("Annie"));
    assert_eq!(sender_name(&app, vcx, 202).as_deref(), Some("Bob"));
    assert_eq!(
        counts(&app, vcx),
        (built, patched + 1),
        "one row patched, none rebuilt"
    );
}

#[gpui_kit::test]
fn a_read_receipt_patches_the_outgoing_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    let receipt = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| single(app, 203).map(|row| row.chrome.receipt))
    };
    assert_eq!(receipt(&app, vcx), Some(OutboxReceipt::Sent));
    let (built, patched) = counts(&app, vcx);
    apply(
        &app,
        vcx,
        &[r#"{"@type":"updateChatReadOutbox","chat_id":31,"last_read_outbox_message_id":203}"#],
    );
    assert_eq!(receipt(&app, vcx), Some(OutboxReceipt::Read));
    assert_eq!(counts(&app, vcx), (built, patched + 1));
}

#[gpui_kit::test]
fn an_edit_patches_only_its_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    let (built, patched) = counts(&app, vcx);
    apply(
        &app,
        vcx,
        &[
            r#"{"@type":"updateMessageContent","chat_id":31,"message_id":202,"new_content":{"@type":"messageText","text":{"@type":"formattedText","text":"Hi everyone.","entities":[]}}}"#,
        ],
    );
    let text = app.read_with(vcx, |app, _| {
        single(app, 202).map(|row| quill::state::effective_preview(&row.message))
    });
    assert_eq!(text.as_deref(), Some("Hi everyone."));
    assert_eq!(
        counts(&app, vcx),
        (built, patched + 1),
        "one row patched, none rebuilt"
    );
}

#[gpui_kit::test]
fn a_reaction_patches_only_its_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    let (built, patched) = counts(&app, vcx);
    apply(
        &app,
        vcx,
        &[
            r#"{"@type":"updateMessageInteractionInfo","chat_id":31,"message_id":201,"interaction_info":{"@type":"messageInteractionInfo","view_count":0,"forward_count":0,"reactions":{"@type":"messageReactions","reactions":[{"@type":"messageReaction","type":{"@type":"reactionTypeEmoji","emoji":"👍"},"total_count":1,"is_chosen":false,"recent_sender_ids":[]}],"are_tags":false}}}"#,
        ],
    );
    let reacted = app.read_with(vcx, |app, _| {
        single(app, 201).is_some_and(|row| {
            row.message
                .interaction_info
                .as_ref()
                .is_some_and(|info| !info.emoji_chips().is_empty())
        })
    });
    assert!(reacted);
    assert_eq!(counts(&app, vcx), (built, patched + 1));
}

#[gpui_kit::test]
fn another_chats_typing_and_status_touch_no_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    let before = counts(&app, vcx);
    apply(
        &app,
        vcx,
        &[
            r#"{"@type":"updateChatAction","chat_id":12,"topic_id":null,"sender_id":{"@type":"messageSenderUser","user_id":12},"action":{"@type":"chatActionTyping"}}"#,
            r#"{"@type":"updateUserStatus","user_id":22,"status":{"@type":"userStatusOnline","expires":1890000000}}"#,
            r#"{"@type":"updateChatReadOutbox","chat_id":12,"last_read_outbox_message_id":40}"#,
        ],
    );
    assert_eq!(counts(&app, vcx), before);
}

#[gpui_kit::test]
fn voice_playback_keeps_the_rows_and_follows_the_position(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-voice-playing");
    let vcx = &mut vcx;
    frames(vcx);
    let playing = app.read_with(vcx, |app, _| app.active_playback_id());
    let Some(playing) = playing else {
        panic!("the demo plays a voice note");
    };
    let before = counts(&app, vcx);
    let shown = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.history.rows.iter().find_map(|row| match row {
                HistoryRow::Single(inputs) if inputs.message.id == playing => app
                    .history_row_seek_bar(&inputs.message)
                    .map(|bar| bar.display_secs),
                _ => None,
            })
        })
    };
    for secs in [1.0, 2.5] {
        app.update(vcx, |app, cx| {
            app.playback.seek_preview_secs = Some(secs);
            cx.notify();
        });
        frames(vcx);
        assert_eq!(shown(&app, vcx), Some(secs));
    }
    assert_eq!(counts(&app, vcx), before, "playback rebuilds no rows");
}

#[gpui_kit::test]
fn the_seek_slider_follows_a_position_tick_that_notifies_only_slices(cx: &mut TestAppContext) {
    use gpui_kit::component::slider::SliderValue;
    let (app, mut vcx) = new_app(cx, "ready-voice-playing");
    let vcx = &mut vcx;
    frames(vcx);
    let value = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, cx| {
            app.playback
                .seek_slider
                .as_ref()
                .map(|slider| slider.read(cx).value())
        })
    };
    assert!(value(&app, vcx).is_some(), "the demo shows a seek slider");
    let target = app.update(vcx, |app, _| {
        let at = app.playback.active_position_secs().unwrap_or(0.0) + 3.0;
        let _ = app.playback.audio.seek(at, true);
        app.playback.active_position_secs()
    });
    let Some(target) = target else {
        panic!("the demo has a track position");
    };
    // Only the slices are notified and drawn: no parking, which would run
    // the playback timer (it also touches the app).
    let _ = crate::ui::frame_clock::take_slice_renders();
    app.update(vcx, |app, cx| app.notify_conversations(cx));
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let rendered = crate::ui::frame_clock::take_slice_renders();
    assert!(rendered.contains_key("conversation"), "{rendered:?}");
    assert!(!rendered.contains_key("(app notified)"), "{rendered:?}");
    // The demo's sound keeps the test clock, which stands still here: the
    // slider shows the seek target, not the value before the tick.
    let Some(SliderValue::Single(shown)) = value(&app, vcx) else {
        panic!("the seek slider holds one value");
    };
    assert!(
        shown >= target as f32,
        "slider {shown} behind {target} {rendered:?}"
    );
}

#[gpui_kit::test]
fn a_thread_reuses_its_rows_until_a_reply_arrives(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-thread");
    let vcx = &mut vcx;
    frames(vcx);
    let ids = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.history
                .rows
                .iter()
                .filter_map(HistoryRow::first_id)
                .map(|id| id.0)
                .collect::<Vec<_>>()
        })
    };
    let shown = ids(&app, vcx);
    assert!(shown.contains(&501) && shown.contains(&504), "{shown:?}");
    let before = counts(&app, vcx);
    for _ in 0..3 {
        app.update(vcx, |_, cx| cx.notify());
        frames(vcx);
    }
    assert_eq!(
        counts(&app, vcx),
        before,
        "an unchanged thread is not rebuilt"
    );
    apply(
        &app,
        vcx,
        &[
            r#"{"@type":"updateNewMessage","message":{"id":505,"chat_id":14,"sender_id":{"@type":"messageSenderUser","user_id":21},"is_outgoing":false,"date":1790632300,"topic_id":{"@type":"messageTopicThread","message_thread_id":501},"content":{"@type":"messageText","text":{"@type":"formattedText","text":"Got it.","entities":[]}}}}"#,
        ],
    );
    assert_eq!(ids(&app, vcx).last(), Some(&505));
    assert_eq!(counts(&app, vcx).0, before.0 + 1);
}

fn tick_renders(
    app: &Entity<QuillApp>,
    cx: &mut VisualTestContext,
    tick: impl Fn(&mut QuillApp, &mut gpui_kit::Context<QuillApp>),
) -> std::collections::BTreeMap<&'static str, u32> {
    let _ = crate::ui::frame_clock::take_slice_renders();
    app.update(cx, |app, cx| tick(app, cx));
    cx.run_until_parked();
    // A display frame: dirty views only (`render_frame` would refresh the
    // whole window, past every cache).
    cx.update(|window, cx| window.draw(cx).clear(cx));
    crate::ui::frame_clock::take_slice_renders()
}

#[gpui_kit::test]
fn timer_ticks_redraw_only_the_slice_that_shows_them(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-voice-playing");
    let vcx = &mut vcx;
    frames(vcx);
    let playback = tick_renders(&app, vcx, |app, cx| app.notify_conversations(cx));
    assert!(playback.contains_key("conversation"), "{playback:?}");
    assert!(!playback.contains_key("sidebar"), "{playback:?}");
    assert!(!playback.contains_key("(app notified)"), "{playback:?}");
    let slow_mode = tick_renders(&app, vcx, |app, cx| {
        app.notify_tick(crate::ui::app_slice::SliceKind::Composer, cx)
    });
    assert!(slow_mode.contains_key("composer"), "{slow_mode:?}");
    assert!(!slow_mode.contains_key("conversation"), "{slow_mode:?}");
    assert!(!slow_mode.contains_key("sidebar"), "{slow_mode:?}");
}

/// Where message `id`'s row and bubble were painted: (row height, bubble width).
fn painted(id: i64) -> (f32, f32) {
    let Some((row, Some(bubble))) = crate::ui::vanish::painted(id) else {
        panic!("message {id} is painted with a bubble");
    };
    (f32::from(row.size.height), f32::from(bubble.size.width))
}

fn photo_width(cx: &mut VisualTestContext, selector: &'static str) -> f32 {
    let Some(bounds) = cx.debug_bounds(selector) else {
        panic!("{selector} is painted");
    };
    f32::from(bounds.size.width)
}

fn resize(app: &Entity<QuillApp>, cx: &mut VisualTestContext, width: f32, height: f32) {
    cx.simulate_resize(size(px(width), px(height)));
    // The width is measured at paint and applied on the next frame.
    frames(cx);
    app.update(cx, |_, cx| cx.notify());
    frames(cx);
}

#[gpui_kit::test]
fn a_narrow_window_rewraps_the_bubbles_without_rebuilding_the_rows(cx: &mut TestAppContext) {
    use quill::bubble_layout::HistoryWidth;
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    apply(
        &app,
        vcx,
        &[
            r#"{"@type":"updateNewMessage","message":{"id":204,"chat_id":31,"sender_id":{"@type":"messageSenderUser","user_id":21},"is_outgoing":false,"date":1790631900,"content":{"@type":"messageText","text":{"@type":"formattedText","text":"The train is late again, so start without me and I will catch up with everyone at the station café once it finally pulls in.","entities":[]}}}}"#,
            r#"{"@type":"updateNewMessage","message":{"id":205,"chat_id":31,"sender_id":{"@type":"messageSenderUser","user_id":22},"is_outgoing":false,"date":1790631960,"content":{"@type":"messagePhoto","photo":{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{"@type":"photoSize","type":"x","photo":{"@type":"file","id":905,"size":10,"expected_size":10,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"r5","unique_id":"u5","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}},"width":1200,"height":800,"progressive_sizes":[]}]},"caption":{"@type":"formattedText","text":"","entities":[]},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}}"#,
            // Already read, so no read mark lands mid-test and rebuilds rows.
            r#"{"@type":"updateChatReadInbox","chat_id":31,"last_read_inbox_message_id":205,"unread_count":0}"#,
        ],
    );
    let width = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| app.history.width.get())
    };
    assert_eq!(width(&app, vcx), HistoryWidth::WIDE);
    let (wide_row, wide_bubble) = painted(204);
    let wide_photo = photo_width(vcx, "photo-205");
    let built = counts(&app, vcx).0;

    // Widening a wide history changes nothing (short of the wide layout,
    // which rebuilds the rows on purpose).
    resize(&app, vcx, 1150., 800.);
    let layout = app.read_with(vcx, |app, _| app.history.chat_layout.get());
    assert_eq!(layout, quill::chat_wide::ChatLayout::Normal);
    assert_eq!(width(&app, vcx), HistoryWidth::WIDE);
    assert_eq!(painted(204), (wide_row, wide_bubble));
    assert_eq!(counts(&app, vcx).0, built);

    resize(&app, vcx, 700., 800.);
    let narrow = width(&app, vcx);
    assert!(narrow.get() < HistoryWidth::WIDE.get(), "{narrow:?}");
    let (row, bubble) = painted(204);
    assert!(
        bubble < wide_bubble,
        "bubble {bubble} not under {wide_bubble}"
    );
    assert!(
        bubble <= narrow.bubble_room(true, false) as f32,
        "bubble {bubble} past its room in {narrow:?}"
    );
    assert!(row > wide_row, "row {row} not taller than {wide_row}");
    // The photo scales down to its room instead of running off the pane.
    let photo = photo_width(vcx, "photo-205");
    assert!(photo < wide_photo, "photo {photo} not under {wide_photo}");
    assert!(
        photo <= narrow.bubble_room(true, false) as f32,
        "photo {photo} past its room in {narrow:?}"
    );
    assert_eq!(counts(&app, vcx).0, built, "resizing rebuilds no rows");

    resize(&app, vcx, 1100., 800.);
    assert_eq!(width(&app, vcx), HistoryWidth::WIDE);
    assert_eq!(painted(204), (wide_row, wide_bubble));
    assert_eq!(photo_width(vcx, "photo-205"), wide_photo);
    assert_eq!(counts(&app, vcx).0, built);
}

#[gpui_kit::test]
fn an_unpainted_history_is_seeded_from_its_pane_until_the_probe_measures(cx: &mut TestAppContext) {
    use quill::bubble_layout::HistoryWidth;
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    let width = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| app.history.width.get())
    };
    app.read_with(vcx, |app, _| {
        // The harness already painted once; start as a fresh history.
        app.history.width_probed.set(false);
        app.seed_history_width(403.);
    });
    assert_eq!(width(&app, vcx), HistoryWidth::new(400.));
    app.read_with(vcx, |app, _| app.seed_history_width(1200.));
    assert_eq!(width(&app, vcx), HistoryWidth::WIDE);
    // Once the history measured itself, a seed no longer overrides it.
    app.read_with(vcx, |app, _| {
        app.history.width_probed.set(true);
        app.seed_history_width(380.);
    });
    assert_eq!(width(&app, vcx), HistoryWidth::WIDE);
}

#[gpui_kit::test]
fn a_change_of_sender_or_album_does_not_keep_the_place(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    let old = app.read_with(vcx, |app, _| {
        single(app, 202).map(|row| row.message.clone())
    });
    let Some(old) = old else {
        panic!("row 202 is a single row");
    };
    assert!(super::same_place(&old, &old), "an unchanged message stays");
    let mut other_sender = old.clone();
    other_sender.sender = Some(quill::telegram::envelope::MessageSender::User { user_id: 9999 });
    assert!(!super::same_place(&old, &other_sender), "sender moved");
    let mut other_album = old.clone();
    other_album.media_album_id = 77;
    assert!(!super::same_place(&old, &other_album), "album id moved");
}

#[gpui_kit::test]
fn an_edit_that_makes_a_message_album_media_builds_its_rows_again(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    let (built, patched) = counts(&app, vcx);
    let splices = app.read_with(vcx, |app, _| app.history.row_stats.splices);
    apply(
        &app,
        vcx,
        &[
            r#"{"@type":"updateMessageContent","chat_id":31,"message_id":202,"new_content":{"@type":"messagePhoto","photo":{"@type":"photo","has_stickers":false,"minithumbnail":null,"sizes":[{"@type":"photoSize","type":"x","photo":{"@type":"file","id":906,"size":10,"expected_size":10,"local":{"@type":"localFile","path":"","can_be_downloaded":true,"can_be_deleted":false,"is_downloading_active":false,"is_downloading_completed":false,"download_offset":0,"downloaded_prefix_size":0,"downloaded_size":0},"remote":{"@type":"remoteFile","id":"r6","unique_id":"u6","is_uploading_active":false,"is_uploading_completed":true,"uploaded_size":10}},"width":1200,"height":800,"progressive_sizes":[]}]},"caption":{"@type":"formattedText","text":"","entities":[]},"show_caption_above_media":false,"has_spoiler":false,"is_secret":false}}"#,
        ],
    );
    let (built_after, patched_after) = counts(&app, vcx);
    assert_eq!(patched_after, patched, "no row patched in place");
    assert_eq!(built_after, built, "the other rows kept");
    assert_eq!(
        app.read_with(vcx, |app, _| app.history.row_stats.splices),
        splices + 1,
        "the rows around it built again"
    );
}

#[path = "rows_cache_long_tests.rs"]
mod long;

#[path = "row_splice_tests.rs"]
mod splice;

#[path = "rows_vanish_tests.rs"]
mod vanish;

#[path = "paragraph_cache_scroll_tests.rs"]
mod paragraph_cache_scroll;

#[path = "file_album_tests.rs"]
mod file_album;
