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
fn an_edit_rebuilds_the_rows_with_the_new_text(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    let (built, _) = counts(&app, vcx);
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
    assert_eq!(counts(&app, vcx).0, built + 1);
}

#[gpui_kit::test]
fn a_reaction_rebuilds_the_rows_with_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    open_group(&app, vcx);
    let (built, _) = counts(&app, vcx);
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
    assert_eq!(counts(&app, vcx).0, built + 1);
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
