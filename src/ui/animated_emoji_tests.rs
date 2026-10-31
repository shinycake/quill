//! UI tests on the `ready-animated-emoji` demo: a lone animated emoji is a
//! sticker in its skin tone that plays once, rests, and plays again on a
//! tap; leaving the chat forgets what played; custom emoji get their sizes;
//! their clips close once no window shows them, and only then.

use super::{message_emoji_media, play_head};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Entity, RenderImage, TestAppContext, VisualTestContext, px, size,
};
use quill::animated_emoji::EmojiMedia;
use quill::ids::{ChatId, MessageId};
use quill::state::HistoryMessage;
use quill::sticker_playback::FitzType;
use quill::sticker_playback::player::DecodedFrames;
use quill::telegram::envelope::MessageContent;
use std::sync::Arc;
use std::time::Duration;

const CHAT: i64 = 11;
/// The demo's first message (`demos/animated_emoji.rs`).
const FIRST_MESSAGE: i64 = 9_301;
const HAND_FILE: i32 = 7_301;
const CUSTOM_FILE: i32 = 7_302;

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

fn open_demo(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-animated-emoji");
    let handle = cx.open_window(size(px(1000.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        // A one-second clip: long enough to tap during a replay.
        for file in [HAND_FILE, CUSTOM_FILE] {
            view.read(cx).playback.stickers.set_fixture(
                file,
                Arc::new(|edge| DecodedFrames {
                    frames: (0..30u8)
                        .map(|i| vec![i; (edge as usize).pow(2) * 4])
                        .collect(),
                    fps: 30.0,
                }),
            );
        }
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    settle(&app, &mut vcx);
    (app, vcx)
}

/// Let the player draw what it was asked for, then draw the window again.
fn settle(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    for _ in 0..3 {
        frames(vcx);
        app.read_with(vcx, |app, _| app.playback.stickers.settle());
        app.update(vcx, |_, cx| cx.notify());
    }
    frames(vcx);
}

fn message(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, id: i64) -> HistoryMessage {
    app.read_with(vcx, |app, _| {
        app.demo_session
            .as_ref()
            .and_then(|s| s.histories.get(&CHAT))
            .and_then(|history| history.messages.get(&id))
            .cloned()
            .expect("the demo message")
    })
}

#[gpui_kit::test]
fn a_lone_emoji_is_a_sticker_in_its_skin_tone(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    for (id, fitz) in [
        (FIRST_MESSAGE, FitzType::None),
        (FIRST_MESSAGE + 1, FitzType::Type4),
    ] {
        let message = message(&app, &mut vcx, id);
        let MessageContent::Text(text) = &message.content else {
            panic!("a text message");
        };
        assert_eq!(text.animated.as_ref().map(|a| a.fitz), Some(fitz));
        let media = app.read_with(&vcx, |app, _| message_emoji_media(&message, app.session()));
        assert_eq!(media, Some(EmojiMedia::Sticker));
        let view = app.update(&mut vcx, |app, cx| app.lone_emoji_view(&message, cx));
        assert!(view.is_some(), "the sticker plays");
    }
}

#[gpui_kit::test]
fn a_lone_emoji_plays_once_and_a_tap_plays_it_again(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let key = (CHAT, FIRST_MESSAGE);
    let message = message(&app, &mut vcx, FIRST_MESSAGE);
    // Shown: its first play started, and the next build notes it.
    let _ = app.update(&mut vcx, |app, cx| app.lone_emoji_view(&message, cx));
    app.read_with(&vcx, |app, _| {
        assert_eq!(
            app.playback
                .stickers
                .head_known(play_head(key, 0), HAND_FILE),
            Some(true),
            "the play head started when the row painted"
        );
        assert!(app.playback.emoji_plays.borrow().has_played(key));
    });
    // Drawn for a second (30 frames at 30 fps), it played through.
    let done = |app: &QuillApp| {
        app.playback
            .stickers
            .head_progress(play_head(key, 0), HAND_FILE)
            .is_some_and(|(pos, total)| pos >= total)
    };
    for _ in 0..60 {
        if app.read_with(&vcx, |app, _| done(app)) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
        app.update(&mut vcx, |_, cx| cx.notify());
        frames(&mut vcx);
    }
    assert!(app.read_with(&vcx, |app, _| done(app)), "it played through");
    // A tap plays it again, on a new head.
    app.update(&mut vcx, |app, _| {
        app.tap_animated_emoji(ChatId(CHAT), MessageId(FIRST_MESSAGE));
    });
    app.read_with(&vcx, |app, _| {
        assert_eq!(app.playback.emoji_plays.borrow().generation(key), 1);
    });
    settle(&app, &mut vcx);
    app.read_with(&vcx, |app, _| {
        assert_eq!(
            app.playback
                .stickers
                .head_known(play_head(key, 1), HAND_FILE),
            Some(true),
            "the replay started"
        );
    });
    // A second tap while it plays does not start it over; once it played
    // through (a slow machine can get there during `settle`), it does.
    let replaying = app.read_with(&vcx, |app, _| {
        app.playback
            .stickers
            .head_progress(play_head(key, 1), HAND_FILE)
            .is_some_and(|(pos, total)| pos < total)
    });
    app.update(&mut vcx, |app, _| {
        app.tap_animated_emoji(ChatId(CHAT), MessageId(FIRST_MESSAGE));
    });
    app.read_with(&vcx, |app, _| {
        let expected = if replaying { 1 } else { 2 };
        assert_eq!(app.playback.emoji_plays.borrow().generation(key), expected);
    });
    // Leaving the chat forgets it: it plays again next time.
    let other = other_chat(&app, &vcx);
    open_in_main(&app, &mut vcx, other);
    app.read_with(&vcx, |app, _| {
        let plays = app.playback.emoji_plays.borrow();
        assert!(!plays.has_played(key));
        assert_eq!(plays.generation(key), 0);
    });
}

#[gpui_kit::test]
fn custom_emoji_messages_take_their_sizes(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    for (id, side, count) in [(FIRST_MESSAGE + 2, 78, 2), (FIRST_MESSAGE + 3, 58, 3)] {
        let message = message(&app, &mut vcx, id);
        let media = app
            .read_with(&vcx, |app, _| message_emoji_media(&message, app.session()))
            .expect("drawn big");
        assert_eq!(media.side(), side);
        let views = app
            .update(&mut vcx, |app, cx| app.big_custom_emoji_views(&message, cx))
            .expect("big custom emoji");
        assert_eq!(views.len(), count);
    }
}

/// Let `seconds` pass a second at a time, drawing nothing: the player's
/// clock and the executor's move together, and timers due fire.
fn idle(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, seconds: u64) {
    for _ in 0..seconds {
        app.read_with(vcx, |app, _| {
            app.playback.stickers.skip(Duration::from_secs(1));
        });
        vcx.executor().advance_clock(Duration::from_secs(1));
        vcx.run_until_parked();
    }
}

fn open_clips(app: &Entity<QuillApp>, vcx: &VisualTestContext) -> usize {
    app.read_with(vcx, |app, _| app.playback.stickers.engine().stats().clips)
}

fn hand_clips(app: &Entity<QuillApp>, vcx: &VisualTestContext) -> usize {
    app.read_with(vcx, |app, _| app.playback.stickers.clips_of(HAND_FILE))
}

/// Every clip open at `first` (`Player::first_frames`) still is, with the
/// frames it had: none closed and opened again.
fn assert_still_open(
    app: &Entity<QuillApp>,
    vcx: &VisualTestContext,
    first: &[Option<std::time::Instant>],
) {
    let now = app.read_with(vcx, |app, _| app.playback.stickers.first_frames());
    let gone: Vec<_> = first.iter().filter(|at| !now.contains(at)).collect();
    assert!(gone.is_empty(), "closed and opened again: {gone:?}");
}

/// One frame of the main window.
fn main_frame(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    app.update(vcx, |_, cx| cx.notify());
    vcx.update(|window, cx| window.render_frame(cx));
}

/// An image with `image`'s id, to ask the atlas about it once `image`
/// itself is gone.
fn atlas_probe(image: &RenderImage) -> RenderImage {
    let mut probe = RenderImage::new(smallvec::SmallVec::from_buf([image::Frame::new(
        image::RgbaImage::new(1, 1),
    )]));
    probe.id = image.id;
    probe
}

/// The emoji at rest on the conversation's animation layer, with nothing
/// drawn for longer than a clip may wait: on screen, they stay open, and
/// through the next frames. The closer looks once a burst of frames, not
/// at every frame.
#[gpui_kit::test]
fn emoji_at_rest_on_the_layer_stay_open_while_nothing_draws(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    // Played through: the lone emoji rest.
    app.read_with(&vcx, |app, _| {
        app.playback.stickers.skip(Duration::from_secs(2));
    });
    settle(&app, &mut vcx);
    let open = open_clips(&app, &vcx);
    assert!(open >= 3, "the emoji play: {open} clips");
    let first = app.read_with(&vcx, |app, _| app.playback.stickers.first_frames());
    idle(&app, &mut vcx, 10);
    assert_eq!(open_clips(&app, &vcx), open, "on screen: open");
    let looks = app.read_with(&vcx, |app, _| app.playback.stickers.looks());
    for _ in 0..5 {
        main_frame(&app, &mut vcx);
        vcx.run_until_parked();
    }
    assert_still_open(&app, &vcx, &first);
    let looked = app.read_with(&vcx, |app, _| app.playback.stickers.looks()) - looks;
    assert!(looked <= 1, "the closer looked {looked} times in 5 frames");
}

/// The messages go while nothing else changes: the next frame shows none
/// of their emoji, and no frame follows. Their clips close once due, and
/// their frames leave the atlas and memory then: they used to wait for a
/// frame to sweep them.
#[gpui_kit::test]
fn emoji_no_window_shows_close_and_free_their_frames_with_nothing_drawn(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    // The lone emoji show only in the messages (the chat list may show the
    // custom ones too).
    let frame = app
        .read_with(&vcx, |app, _| app.playback.stickers.any_frame(HAND_FILE))
        .expect("a lone emoji's frame");
    let probe = atlas_probe(&frame);
    let freed = Arc::downgrade(&frame);
    drop(frame);
    assert!(vcx.update(|window, _| window.has_image_atlas_entry(&probe)));
    let ids: Vec<String> = (FIRST_MESSAGE..FIRST_MESSAGE + 4)
        .map(|id| id.to_string())
        .collect();
    receive(
        &app,
        &mut vcx,
        &format!(
            r#"{{"@type":"updateDeleteMessages","chat_id":{CHAT},"message_ids":[{}],"is_permanent":true,"from_cache":false}}"#,
            ids.join(",")
        ),
    );
    main_frame(&app, &mut vcx);
    let drawn = crate::ui::image_budget::frame();
    assert!(hand_clips(&app, &vcx) > 0);
    idle(&app, &mut vcx, 8);
    assert_eq!(crate::ui::image_budget::frame(), drawn, "nothing drew");
    assert_eq!(hand_clips(&app, &vcx), 0, "closed with nothing drawn");
    assert_eq!(freed.strong_count(), 0, "its frame left memory");
    assert!(
        !vcx.update(|window, _| window.has_image_atlas_entry(&probe)),
        "and the atlas"
    );
}

/// The demo chat moved to its own window (the main window leaves it),
/// drawn there until its emoji play and rest.
/// The demo session receives `json`.
fn receive(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, json: &str) {
    app.update(vcx, |app, _| {
        let sink: Arc<dyn quill::diagnostics::DiagnosticSink> = app.demo_ui.sink.clone();
        let session = app.demo_session.as_mut().expect("the demo session");
        // `Session::apply` drops an envelope not newer than the last one.
        let seq = std::sync::atomic::AtomicU64::new(session.last_seq);
        let owned = quill::telegram::client::copy_and_parse(json, &seq, &sink);
        session.apply(owned.expect("the update parses"));
    });
}

fn chat_in_window(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> AnyWindowHandle {
    let main = vcx.update(|window, _| window.window_handle());
    // Read: a window opens an unread chat at its read boundary, which the
    // demo's history of four messages does not reach.
    let last = FIRST_MESSAGE + 3;
    receive(
        app,
        vcx,
        &format!(
            r#"{{"@type":"updateChatReadInbox","chat_id":{CHAT},"last_read_inbox_message_id":{last},"unread_count":0}}"#
        ),
    );
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_chat_in_new_window(ChatId(CHAT), window, cx)
        });
    });
    vcx.run_until_parked();
    let chat_window = vcx
        .windows()
        .into_iter()
        .find(|handle| handle.window_id() != main.window_id())
        .expect("the chat window");
    let draw = |vcx: &mut VisualTestContext| {
        vcx.run_until_parked();
        vcx.update_window(chat_window, |_, window, cx| window.render_frame(cx))
            .expect("the chat window draws");
    };
    for step in 0..4 {
        draw(vcx);
        app.read_with(vcx, |app, _| {
            if step == 1 {
                app.playback.stickers.skip(Duration::from_secs(2));
            }
            app.playback.stickers.settle();
        });
    }
    draw(vcx);
    chat_window
}

/// A chat window's emoji at rest stay open while the main window draws on
/// without them: the main window's frames say nothing about that window.
#[gpui_kit::test]
fn a_chat_windows_emoji_at_rest_stay_open_while_the_main_window_draws(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let _chat_window = chat_in_window(&app, &mut vcx);
    let open = open_clips(&app, &vcx);
    let shown = app.read_with(&vcx, |app, _| {
        let session = app.demo_session.as_ref().expect("the demo");
        let history = session.histories.get(&CHAT).map(|h| h.messages.len());
        let anchor = session.chats.get(&CHAT).and_then(|c| c.unread_anchor());
        (history, anchor, app.playback.stickers.screen_stats())
    });
    assert!(
        open >= 3,
        "the emoji play in the chat window: {open} clips ({shown:?})"
    );
    let first = app.read_with(&vcx, |app, _| app.playback.stickers.first_frames());
    for _ in 0..3 {
        main_frame(&app, &mut vcx);
    }
    idle(&app, &mut vcx, 10);
    assert_still_open(&app, &vcx, &first);
}

/// A chat window's emoji close as it closes, and their frames leave
/// memory then, not once the main window draws (it may not: minimized or
/// covered).
#[gpui_kit::test]
fn a_chat_windows_emoji_close_with_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let chat_window = chat_in_window(&app, &mut vcx);
    assert!(hand_clips(&app, &vcx) > 0);
    let frame = app
        .read_with(&vcx, |app, _| app.playback.stickers.any_frame(HAND_FILE))
        .expect("a lone emoji's frame");
    let freed = Arc::downgrade(&frame);
    drop(frame);
    vcx.update_window(chat_window, |_, window, cx| {
        window.dispatch_action(Box::new(crate::ui::actions::CloseWindow), cx);
    })
    .expect("the chat window closes");
    vcx.run_until_parked();
    // What played there is forgotten: it plays from the start when the
    // chat shows again (tdesktop drops the window's played set).
    let key = (CHAT, FIRST_MESSAGE);
    app.read_with(&vcx, |app, _| {
        let head = app
            .playback
            .stickers
            .head_known(play_head(key, 0), HAND_FILE);
        assert_eq!(head, None, "its lone emoji's head");
        assert!(!app.playback.emoji_plays.borrow().has_played(key));
    });
    // Its clips close with it, not 5 s later, and their frames go.
    assert_eq!(hand_clips(&app, &vcx), 0, "closed with the window");
    assert_eq!(freed.strong_count(), 0, "their frames left memory");
}

/// A chat in the demo's list other than the demo chat.
fn other_chat(app: &Entity<QuillApp>, vcx: &VisualTestContext) -> ChatId {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|session| {
                session
                    .ordered_chats()
                    .into_iter()
                    .map(|chat| chat.id)
                    .find(|id| id.0 != CHAT)
            })
            .expect("the demo lists another chat")
    })
}

/// The main window opens `chat`.
fn open_in_main(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, chat: ChatId) {
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| app.select_listed_chat(chat, window, cx));
    });
}

/// Where the lone emoji's play is: its head (position and length) and
/// whether it counts as played.
fn lone_emoji_play(
    app: &Entity<QuillApp>,
    vcx: &VisualTestContext,
) -> (Option<(Duration, Duration)>, bool) {
    let key = (CHAT, FIRST_MESSAGE);
    app.read_with(vcx, |app, _| {
        let head = play_head(key, 0);
        (
            app.playback.stickers.head_progress(head, HAND_FILE),
            app.playback.emoji_plays.borrow().has_played(key),
        )
    })
}

/// The main window opens another chat while a chat window shows the demo
/// chat: the window's emoji go on as they were. Opening a chat used to
/// close every clip and forget every head and every played emoji, in every
/// window, so the window's emoji opened again (their stills, then the
/// first frame) and its lone emoji played again from the start.
#[gpui_kit::test]
fn the_main_window_opening_a_chat_leaves_a_chat_windows_emoji_playing(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let chat_window = chat_in_window(&app, &mut vcx);
    let other = other_chat(&app, &vcx);
    let (head, played) = lone_emoji_play(&app, &vcx);
    assert!(head.is_some() && played, "it plays in the window");
    // Still played, and its head where it was or further on (the window
    // may draw as the main window switches).
    let went_on = |app: &Entity<QuillApp>, vcx: &VisualTestContext, what: &str| {
        let (now, played) = lone_emoji_play(app, vcx);
        let pos = |head: Option<(Duration, Duration)>| head.map(|(pos, _)| pos);
        assert!(
            played && pos(now) >= pos(head),
            "{what}: {head:?} then {now:?}, played {played}"
        );
    };
    // The lone emoji show only in the window (the main window's chat list
    // may show the custom ones too).
    let hand_first = |app: &Entity<QuillApp>, vcx: &VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.playback.stickers.first_frames_of(Some(HAND_FILE))
        })
    };
    let first = hand_first(&app, &vcx);
    assert!(!first.is_empty() && first.iter().all(Option::is_some));
    open_in_main(&app, &mut vcx, other);
    assert_eq!(hand_first(&app, &vcx), first, "its clips stay open");
    went_on(&app, &vcx, "the window's lone emoji went on");
    // Both windows draw.
    main_frame(&app, &mut vcx);
    for _ in 0..2 {
        vcx.run_until_parked();
        vcx.update_window(chat_window, |_, window, cx| window.render_frame(cx))
            .expect("the chat window draws");
        app.read_with(&vcx, |app, _| app.playback.stickers.settle());
    }
    assert_eq!(hand_first(&app, &vcx), first, "none opened again");
    went_on(&app, &vcx, "not started over");
}

/// The chat the main window leaves forgets what played there, as in
/// tdesktop (its lone emoji play again when it opens again), and its clips
/// close.
#[gpui_kit::test]
fn the_chat_the_main_window_leaves_plays_again_and_closes_its_clips(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let other = other_chat(&app, &vcx);
    // The player's clock moves only as the test moves it: the lone emoji
    // plays 500 ms, a paint every 100 ms.
    app.read_with(&vcx, |app, _| app.playback.stickers.freeze());
    main_frame(&app, &mut vcx);
    for _ in 0..5 {
        app.read_with(&vcx, |app, _| {
            app.playback.stickers.skip(Duration::from_millis(100));
        });
        main_frame(&app, &mut vcx);
    }
    let (head, played) = lone_emoji_play(&app, &vcx);
    let pos = head.map(|(pos, _)| pos);
    assert!(
        pos >= Some(Duration::from_millis(500)) && played,
        "{head:?}"
    );
    assert!(hand_clips(&app, &vcx) > 0);
    open_in_main(&app, &mut vcx, other);
    assert_eq!(lone_emoji_play(&app, &vcx), (None, false), "forgotten");
    assert_eq!(hand_clips(&app, &vcx), 0, "its clips closed");
    // Back in the chat: its clip opens, and the lone emoji plays again
    // from its first frame.
    open_in_main(&app, &mut vcx, ChatId(CHAT));
    settle(&app, &mut vcx);
    let (head, played) = lone_emoji_play(&app, &vcx);
    assert_eq!(head.map(|(pos, _)| pos), Some(Duration::ZERO), "{head:?}");
    assert!(played);
    let frame = app
        .read_with(&vcx, |app, _| app.playback.stickers.any_frame(HAND_FILE))
        .expect("a lone emoji's frame");
    assert_eq!(frame.as_bytes(0).and_then(|b| b.first().copied()), Some(0));
}

/// The demo chat in its own window and in the main window too, drawn in
/// both: a comment thread shows in the main window while its discussion
/// group has a window of its own (`select_chat_with` with `thread_switch`).
fn chat_in_both_windows(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> AnyWindowHandle {
    let chat_window = chat_in_window(app, vcx);
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.select_chat_with(ChatId(CHAT), true, window, cx)
        });
    });
    settle(app, vcx);
    chat_window
}

fn close_window(vcx: &mut VisualTestContext, chat_window: AnyWindowHandle) {
    vcx.update_window(chat_window, |_, window, cx| {
        window.dispatch_action(Box::new(crate::ui::actions::CloseWindow), cx);
    })
    .expect("the chat window closes");
}

/// The chat window closes while the main window shows the same chat: the
/// lone emoji there keeps its played mark. Played marks are kept by chat,
/// and closing the window cleared them for the main window too, so its
/// lone emoji played again.
#[gpui_kit::test]
fn closing_a_chat_window_keeps_the_plays_of_a_chat_the_main_window_shows(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let chat_window = chat_in_both_windows(&app, &mut vcx);
    let (head, played) = lone_emoji_play(&app, &vcx);
    assert!(head.is_some() && played, "it played");
    close_window(&mut vcx, chat_window);
    let (now, played) = lone_emoji_play(&app, &vcx);
    assert!(now.is_some() && played, "still played: {now:?}");
}

/// The chat window closes first while the main window shows the same
/// emoji: the clips they share stay open, and the frame the main window
/// shows stays in its atlas.
#[gpui_kit::test]
fn closing_a_chat_window_keeps_the_clips_the_main_window_shares(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx);
    let chat_window = chat_in_both_windows(&app, &mut vcx);
    let first = app.read_with(&vcx, |app, _| {
        app.playback.stickers.first_frames_of(Some(HAND_FILE))
    });
    assert!(!first.is_empty() && first.iter().all(Option::is_some));
    main_frame(&app, &mut vcx);
    let frame = app
        .read_with(&vcx, |app, _| app.playback.stickers.any_frame(HAND_FILE))
        .expect("a lone emoji's frame");
    let probe = atlas_probe(&frame);
    drop(frame);
    assert!(vcx.update(|window, _| window.has_image_atlas_entry(&probe)));
    close_window(&mut vcx, chat_window);
    assert!(
        vcx.update(|window, _| window.has_image_atlas_entry(&probe)),
        "the main window's frame stays in its atlas"
    );
    for _ in 0..2 {
        main_frame(&app, &mut vcx);
        vcx.run_until_parked();
    }
    let now = app.read_with(&vcx, |app, _| {
        app.playback.stickers.first_frames_of(Some(HAND_FILE))
    });
    assert_eq!(now, first, "none closed and opened again");
}

/// The emoji status a chat-list row shows: a one-second clip of its own.
const STATUS_ID: i64 = 8_804;
const STATUS_FILE: i32 = 7_303;
/// The demo's custom emoji, which the demo chat's row shows in its preview.
const PREVIEW_IDS: [i64; 3] = [8_801, 8_802, 8_803];

/// A chat in the list other than the demo chat and `other`, which now has
/// an animated emoji status.
fn give_a_row_a_status(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    other: ChatId,
) -> ChatId {
    use quill::telegram::envelope::{ChatKind, StickerFormat, StickerItem};
    app.update(vcx, |app, _| {
        app.playback.stickers.set_fixture(
            STATUS_FILE,
            Arc::new(|edge| DecodedFrames {
                frames: (0..30u8)
                    .map(|i| vec![i; (edge as usize).pow(2) * 4])
                    .collect(),
                fps: 30.0,
            }),
        );
        let session = app.demo_session.as_mut().expect("the demo session");
        let (chat, kind) = session
            .ordered_chats()
            .into_iter()
            .map(|chat| (chat.id, chat.kind.clone()))
            .find(|(id, _)| id.0 != CHAT && *id != other)
            .expect("the demo lists a third chat");
        match kind {
            ChatKind::Private { user_id } => {
                let user = session.users.entry(user_id.0).or_default();
                user.id = user_id.0;
                user.is_premium = true;
                user.emoji_status_id = STATUS_ID;
            }
            ChatKind::Supergroup { .. } => {
                let look = session.chats_state.chat_looks.entry(chat.0).or_default();
                look.emoji_status_id = STATUS_ID;
            }
            kind => panic!("no emoji status on {kind:?}"),
        }
        session.stickers.emoji.custom_emoji_stickers.update(|list| {
            list.push(StickerItem {
                custom_emoji_id: Some(STATUS_ID),
                id: STATUS_ID,
                set_id: 0,
                emoji: "⭐".into(),
                width: 512,
                height: 512,
                format: StickerFormat::Tgs,
                file_id: quill::ids::FileId(STATUS_FILE),
                thumb_file_id: None,
                thumb_width: 0,
                thumb_height: 0,
                requires_premium: false,
                premium_animation: None,
            });
        });
        chat
    })
}

/// The chat list's animated emoji, looping off, played and at rest: a
/// row's emoji status (two loops, then its first frame) and the custom
/// emoji in the demo chat's preview (once, then its last frame). The main
/// window opens another chat, and they stay as they were: on their clips,
/// where they rested. Opening a chat closed every clip and forgot every
/// head the main window used, the chat list's too, so each started over
/// from its first frame, and a row built while its clip opened again
/// showed its fallback (the star, the plain emoji) meanwhile.
#[gpui_kit::test]
fn the_chat_lists_emoji_stay_at_rest_when_the_main_window_opens_a_chat(cx: &mut TestAppContext) {
    use crate::ui::status_emoji::{StatusSurface, status_head};
    use crate::ui::sticker_player::sidebar_emoji_head;
    let (app, mut vcx) = open_demo(cx);
    let other = other_chat(&app, &vcx);
    let status_chat = give_a_row_a_status(&app, &mut vcx, other);
    // The demo chat's row previews its last message, of custom emoji.
    let entities: Vec<String> = (0..)
        .zip(PREVIEW_IDS)
        .map(|(offset, id)| {
            format!(
                r#"{{"@type":"textEntity","offset":{offset},"length":1,"type":{{"@type":"textEntityTypeCustomEmoji","custom_emoji_id":"{id}"}}}}"#
            )
        })
        .collect();
    let last = FIRST_MESSAGE + 3;
    receive(
        &app,
        &mut vcx,
        &format!(
            r#"{{"@type":"updateChatLastMessage","chat_id":{CHAT},"last_message":{{"id":{last},"chat_id":{CHAT},"is_outgoing":false,"date":1790640000,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"⭐⭐⭐","entities":[{}]}}}}}},"positions":[{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"30","is_pinned":true}}]}}"#,
            entities.join(",")
        ),
    );
    app.update(&mut vcx, |app, _| {
        let session = app.demo_session.as_mut().expect("the demo session");
        session.settings.media_prefs.loop_animated_stickers = false;
        app.stop_sticker_playback();
        app.playback.stickers.freeze();
    });
    settle(&app, &mut vcx);
    // Two and a half seconds, a paint every 100 ms.
    for _ in 0..25 {
        app.read_with(&vcx, |app, _| {
            app.playback.stickers.skip(Duration::from_millis(100));
        });
        main_frame(&app, &mut vcx);
        app.read_with(&vcx, |app, _| app.playback.stickers.settle());
    }
    let heads: Vec<_> = PREVIEW_IDS
        .map(|id| (sidebar_emoji_head(id), CUSTOM_FILE))
        .into_iter()
        .chain([(
            status_head(StatusSurface::ListRow, status_chat.0, STATUS_ID),
            STATUS_FILE,
        )])
        .collect();
    let progress = |app: &Entity<QuillApp>, vcx: &VisualTestContext| {
        app.read_with(vcx, |app, _| {
            heads
                .iter()
                .map(|(head, file)| app.playback.stickers.head_progress(*head, *file))
                .collect::<Vec<_>>()
        })
    };
    let rested = progress(&app, &vcx);
    assert!(
        rested
            .iter()
            .all(|head| head.is_some_and(|(pos, total)| pos >= total)),
        "played and at rest: {rested:?}"
    );
    let clips = |app: &Entity<QuillApp>, vcx: &VisualTestContext, file| {
        app.read_with(vcx, |app, _| {
            app.playback.stickers.first_frames_of(Some(file))
        })
    };
    let status_clips = clips(&app, &vcx, STATUS_FILE);
    let custom_clips = clips(&app, &vcx, CUSTOM_FILE);
    assert!(!status_clips.is_empty() && status_clips.iter().all(Option::is_some));
    open_in_main(&app, &mut vcx, other);
    let stayed = |app: &Entity<QuillApp>, vcx: &VisualTestContext, what: &str| {
        assert_eq!(progress(app, vcx), rested, "{what}: where they rested");
        assert_eq!(
            clips(app, vcx, STATUS_FILE),
            status_clips,
            "{what}: the status's clip stays open"
        );
        // The demo chat's history shows the same custom emoji, bigger:
        // those clips close with the chat, the preview's stays.
        let custom = clips(app, vcx, CUSTOM_FILE);
        assert!(
            !custom.is_empty() && custom.iter().all(|at| custom_clips.contains(at)),
            "{what}: the preview's clip stays open: {custom_clips:?} then {custom:?}"
        );
    };
    stayed(&app, &vcx, "opened");
    // The rows have their frames at once: no fallback while a clip opens.
    let status_frame = app.read_with(&vcx, |app, _| app.playback.stickers.any_frame(STATUS_FILE));
    assert!(status_frame.is_some(), "the status has a frame to show");
    main_frame(&app, &mut vcx);
    stayed(&app, &vcx, "the next frame");
    settle(&app, &mut vcx);
    stayed(&app, &vcx, "settled");
}
