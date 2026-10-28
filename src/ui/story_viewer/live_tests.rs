//! Live story viewer tests: the send button's rules, the pinned strip and
//! the effects clock, and UI flows over the screenshot demo's live story
//! (comments over the picture, the Stars button with its toast and Undo,
//! the comments toggle, the Stars box for a comment, comments disabled,
//! deleting a comment, the viewer closing when the stream ends, the story
//! playing beside a 1:1 call, and the header drawn by the live
//! controller). Nothing reaches Telegram: the app
//! has no connection and joins nothing.
use super::live_state::{
    EFFECT_DURATION, FlyingStars, SendAction, StarsBoxMode, effect_progress, pin_label,
    pinned_comments, send_action,
};
use super::live_story::live_item;
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Bounds, Entity, Modifiers, MouseButton, Pixels, TestAppContext, VisualTestContext,
    px, size,
};
use quill::calls::engine::StreamState;
use quill::ids::{ChatId, FileId};
use quill::live_story::fallback_levels;
use quill::state::LiveBursts;
use quill::story_viewer::{LiveStoryCall, StoryViewerItem, StoryViewerKind};
use quill::telegram::envelope::{
    MessageSender, ParsedGroupCallMessage, StickerFormat, StickerItem,
};
use std::time::{Duration, Instant};

fn message(id: i32, date: i32, stars: i64) -> ParsedGroupCallMessage {
    ParsedGroupCallMessage {
        message_id: id,
        sender_id: MessageSender::User { user_id: 9 },
        date,
        text: "hi".to_string(),
        entities: Vec::new(),
        paid_message_star_count: stars,
        is_from_owner: false,
        can_be_deleted: true,
    }
}

#[test]
fn the_send_button_asks_for_stars_when_the_comment_needs_them() {
    let levels = fallback_levels();
    assert_eq!(send_action(&levels, "", 0, 0, false), SendAction::EditPrice);
    assert_eq!(send_action(&levels, "hi", 0, 0, false), SendAction::Send);
    // The call's price for any comment.
    assert_eq!(
        send_action(&levels, "hi", 0, 5, false),
        SendAction::NeedsStars(5)
    );
    // 31 characters are past the free level's 30.
    let long = "a".repeat(31);
    assert_eq!(
        send_action(&levels, &long, 0, 0, false),
        SendAction::NeedsStars(10)
    );
    assert_eq!(send_action(&levels, &long, 10, 0, false), SendAction::Send);
    // Your own live story: always free.
    assert_eq!(send_action(&levels, &long, 0, 5, true), SendAction::Send);
}

#[test]
fn pinned_comments_run_out_and_the_most_stars_lead() {
    let levels = fallback_levels();
    let now = 1_000;
    // 50 Stars pin for 120 s, 250 for 600 s, a free one never.
    let messages = [
        message(1, 800, 50),
        message(2, 950, 250),
        message(3, 990, 0),
    ];
    let pinned = pinned_comments(&messages, &levels, now);
    let ids: Vec<i32> = pinned.iter().map(|p| p.message_id).collect();
    assert_eq!(ids, vec![2]);
    let pinned = pinned_comments(&messages, &levels, 900);
    let ids: Vec<i32> = pinned.iter().map(|p| p.message_id).collect();
    assert_eq!(ids, vec![2, 1]);
    assert!(pinned.iter().all(|p| (0.0..=1.0).contains(&p.gone)));
}

#[test]
fn effects_last_two_seconds_and_pin_times_shorten() {
    let start = Instant::now();
    assert_eq!(effect_progress(start, start), Some(0.0));
    assert!(effect_progress(start, start + EFFECT_DURATION).is_none());
    let half = effect_progress(start, start + Duration::from_millis(1000)).unwrap_or(0.0);
    assert!((half - 0.5).abs() < 0.01);
    assert_eq!(pin_label(30), "30s");
    assert_eq!(pin_label(120), "2m");
    assert_eq!(pin_label(3600), "1h");
}

#[test]
fn only_a_live_item_with_a_call_is_joined() {
    let mut item = StoryViewerItem {
        chat_id: ChatId(7),
        story_id: 6,
        kind: StoryViewerKind::Live,
        display_file_ids: Vec::new(),
        download_file_id: FileId(0),
        caption: String::new(),
        caption_entities: Vec::new(),
        duration_label: None,
        duration_secs: None,
        video_file_id: None,
        is_live: true,
        live_call: Some(LiveStoryCall {
            group_call_id: 1,
            is_rtmp_stream: true,
        }),
        areas: Vec::new(),
    };
    assert_eq!(live_item(Some(&item)), Some((ChatId(7), 6)));
    item.live_call = None;
    assert_eq!(live_item(Some(&item)), None);
    item.kind = StoryViewerKind::Photo;
    assert_eq!(live_item(None), None);
}

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..2 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// The main window on the demo's live story.
fn open_live(cx: &mut TestAppContext, kind: &'static str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx
            .new(|cx| QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named(kind))));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    hold_the_feed_clock(&app, &mut vcx);
    frames(&mut vcx);
    (app, vcx)
}

/// The demo's one frame was stamped with the real time it was made at, and
/// the player judges a stall by the real time since (`STALL_AFTER`, 1.5 s).
/// A loaded machine can let that pass between the setup and an assertion,
/// which turns the picture into the stalled overlay with its spinner. The
/// test executor's timers are simulated, so only this clock is real: read
/// it as the frame's own arrival time, and the picture plays until a test
/// moves the clock.
fn hold_the_feed_clock(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) {
    let arrived = app.read_with(vcx, |app, _| {
        let player = app.stories.live.player.as_ref()?;
        let (_, at) = player.live_sink()?.snapshot().latest?;
        Some((player.clone(), at))
    });
    if let Some((player, at)) = arrived {
        player.set_live_clock(at);
        app.update(vcx, |_, cx| cx.notify());
    }
}

fn find(vcx: &mut VisualTestContext, selector: &'static str) -> Option<Bounds<Pixels>> {
    vcx.debug_bounds(selector)
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let bounds = find(vcx, selector).unwrap_or_else(|| panic!("{selector} is shown"));
    vcx.simulate_click(bounds.center(), Modifiers::default());
    frames(vcx);
}

#[gpui_kit::test]
fn the_comments_sit_over_the_lower_half_of_the_picture(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    let frame = find(vcx, "live-story-frame").expect("the stream frame");
    assert_eq!(frame.size.width, px(360.));
    assert_eq!(frame.size.height, px(640.));
    let comments = find(vcx, "live-story-comments").expect("the comments");
    // tdesktop: 8 px in from the left, the frame's width less 16, at most
    // half its height, ending 8 px above its bottom.
    assert_eq!(comments.left(), frame.left() + px(8.));
    assert_eq!(comments.size.width, px(344.));
    assert!(comments.size.height <= px(320.));
    assert_eq!(comments.bottom(), frame.bottom() - px(8.));
    // The paid comment is pinned at the top; the composer sits below.
    let pinned = find(vcx, "live-story-pinned-strip").expect("the pinned strip");
    assert!(pinned.top() < frame.top() + px(20.));
    let composer = find(vcx, "live-story-composer").expect("the comment row");
    assert!(composer.top() >= frame.bottom());
}

#[gpui_kit::test]
fn a_star_tap_waits_under_the_toast_until_undo(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    assert!(find(vcx, "live-story-toast").is_none());
    click(vcx, "live-story-stars");
    let stars = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, _| {
            app.session()
                .and_then(|s| s.stories.live.as_ref())
                .and_then(|live| live.pending_reaction)
                .map(|p| p.stars)
        })
    };
    assert_eq!(stars(&app, vcx), Some(1));
    click(vcx, "live-story-stars");
    assert_eq!(stars(&app, vcx), Some(2), "taps add up");
    assert!(find(vcx, "live-story-toast").is_some());
    click(vcx, "live-story-undo");
    assert_eq!(stars(&app, vcx), None);
    assert!(find(vcx, "live-story-toast").is_none());
}

#[gpui_kit::test]
fn the_toggle_hides_and_shows_the_comments(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| app.toggle_live_comments(cx));
    frames(vcx);
    assert!(find(vcx, "live-story-comments").is_none());
    // A click on a pinned comment brings them back.
    let pinned = find(vcx, "live-story-pinned-strip").expect("the pinned strip");
    vcx.simulate_click(
        pinned.origin + gpui_kit::point(px(30.), px(10.)),
        Modifiers::default(),
    );
    frames(vcx);
    assert!(find(vcx, "live-story-comments").is_some());
}

#[gpui_kit::test]
fn the_stars_box_sets_the_next_comment_price(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    click(vcx, "live-story-edit-stars");
    let mode = app.read_with(vcx, |app, _| {
        app.stories
            .live
            .stars_box
            .as_ref()
            .map(|b| (b.mode, b.stars))
    });
    assert_eq!(mode, Some((StarsBoxMode::Comment, 10)));
    app.update_in(vcx, |app, window, cx| app.submit_live_stars_box(window, cx));
    frames(vcx);
    assert_eq!(
        app.read_with(vcx, |app, _| app.stories.live.chosen_stars),
        10
    );
    assert!(app.read_with(vcx, |app, _| app.stories.live.stars_box.is_none()));
    // Remove Stars from the send button's menu.
    let send = find(vcx, "live-story-send").expect("the send button");
    vcx.simulate_mouse_down(send.center(), MouseButton::Right, Modifiers::default());
    vcx.simulate_mouse_up(send.center(), MouseButton::Right, Modifiers::default());
    frames(vcx);
    // The kit's menu with Edit Stars and Remove Stars.
    click(vcx, "live-story-remove-stars");
    assert_eq!(
        app.read_with(vcx, |app, _| app.stories.live.chosen_stars),
        0
    );
}

#[gpui_kit::test]
fn comments_off_replaces_the_field(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    assert!(find(vcx, "live-story-comments-off").is_none());
    app.update(vcx, |app, cx| {
        if let Some(call) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.calls.story_stream.as_mut())
        {
            call.are_messages_allowed = false;
        }
        cx.notify();
    });
    frames(vcx);
    assert!(find(vcx, "live-story-comments-off").is_some());
    assert!(find(vcx, "live-story-stars").is_some(), "Stars still go");
}

#[gpui_kit::test]
fn a_comment_is_deleted_from_its_menu(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    let row = find(vcx, "live-story-comment-2").expect("a comment");
    vcx.simulate_mouse_down(row.center(), MouseButton::Right, Modifiers::default());
    vcx.simulate_mouse_up(row.center(), MouseButton::Right, Modifiers::default());
    frames(vcx);
    // The kit's menu: the date, Copy Text and Delete.
    click(vcx, "live-comment-delete");
    assert!(app.read_with(vcx, |app, _| app.stories.live.delete.is_some()));
    app.update(vcx, |app, cx| app.confirm_delete_live_comment(cx));
    frames(vcx);
    let left: Vec<i32> = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.calls.story_stream.as_ref())
            .map(|call| call.messages.iter().map(|m| m.message_id).collect())
            .unwrap_or_default()
    });
    assert!(!left.contains(&2));
    assert!(find(vcx, "live-story-comment-2").is_none());
}

#[gpui_kit::test]
fn an_ended_stream_closes_the_viewer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(call) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.calls.story_stream.as_mut())
        {
            call.stream = StreamState::Ended;
        }
        app.sync_live_story(cx);
    });
    frames(vcx);
    assert!(!app.read_with(vcx, |app, _| app.stories.viewer.is_open()));
    assert!(app.read_with(vcx, |app, _| app.stories.live.watching.is_none()));
    assert!(find(vcx, "live-story-frame").is_none());
}

#[gpui_kit::test]
fn space_types_in_the_comment_field_and_does_not_pause(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    let field = find(vcx, "live-story-field").expect("the comment field");
    vcx.simulate_click(field.center(), Modifiers::default());
    frames(vcx);
    let focused = app.update_in(vcx, |app, window, cx| app.story_text_input_open(window, cx));
    assert!(focused, "the field has the keyboard");
    vcx.simulate_keystrokes("a space b");
    frames(vcx);
    let value = app.read_with(vcx, |app, cx| {
        app.stories.live.input.read(cx).value().to_string()
    });
    assert_eq!(value, "a b");
    assert!(!app.read_with(vcx, |app, _| app.stories.pause.is_paused()));
    // The placeholder goes once there is text.
    assert!(find(vcx, "live-story-placeholder").is_none());
}

#[gpui_kit::test]
fn a_landscape_stream_shows_whole_and_a_portrait_one_fills(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_live(cx, "ready-live-story");
    assert!(find(&mut vcx, "live-story-backdrop").is_some());
    let (_app, mut vcx) = open_live(cx, "ready-live-story-portrait");
    assert!(find(&mut vcx, "live-story-frame").is_some());
    assert!(find(&mut vcx, "live-story-backdrop").is_none());
}

#[gpui_kit::test]
fn the_composer_is_one_field_between_the_toggle_and_the_stars(cx: &mut TestAppContext) {
    let (_app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    let row = find(vcx, "live-story-composer").expect("the comment row");
    let toggle = find(vcx, "live-story-comments-toggle").expect("the toggle");
    let field = find(vcx, "live-story-field").expect("the field");
    let stars = find(vcx, "live-story-stars").expect("the Stars button");
    let edit = find(vcx, "live-story-edit-stars").expect("Edit Stars");
    let send = find(vcx, "live-story-send").expect("Send");
    // tdesktop: 8 px between the toggle, the field and the Stars button;
    // Edit Stars and Send sit inside the field.
    assert_eq!(field.left(), toggle.right() + px(8.));
    assert_eq!(stars.left(), field.right() + px(8.));
    assert_eq!(stars.right(), row.right());
    assert!(edit.left() >= field.left() && edit.right() <= field.right());
    assert!(send.left() >= field.left() && send.right() <= field.right());
    // "Comment for 5 Stars" fits.
    let placeholder = find(vcx, "live-story-placeholder").expect("the placeholder");
    assert!(placeholder.right() <= edit.left());
}

#[gpui_kit::test]
fn stars_past_the_balance_open_the_stars_top_up(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.payments.hub.balance =
                Some(quill::premium_hub::StarAmount { stars: 3, nanos: 0 });
        }
        app.open_live_stars_box(StarsBoxMode::Reaction, 0, false, cx);
    });
    frames(vcx);
    app.update_in(vcx, |app, window, cx| app.submit_live_stars_box(window, cx));
    frames(vcx);
    app.read_with(vcx, |app, _| {
        let session = app.session().expect("the demo session");
        assert!(session.payments.hub.stars_open, "the Stars box opens");
        assert!(session.payments.star_packs.open, "with its packs");
        assert!(
            session
                .stories
                .live
                .as_ref()
                .is_some_and(|live| live.pending_reaction.is_none()),
            "nothing waits to be sent"
        );
        assert!(app.connection.status_note().contains("Needed"));
    });
    // Back from the top-up, the box opens on the same amount.
    let chosen = app.read_with(vcx, |app, _| {
        app.stories.live.stars_before_top_up.map(|(_, stars)| stars)
    });
    assert!(chosen.is_some_and(|stars| stars > 3));
    app.update(vcx, |app, cx| {
        app.open_live_stars_box(StarsBoxMode::Reaction, 0, false, cx);
    });
    app.read_with(vcx, |app, _| {
        assert_eq!(app.stories.live.stars_box.as_ref().map(|b| b.stars), chosen);
        assert!(app.stories.live.stars_before_top_up.is_none());
    });
}

#[gpui_kit::test]
fn a_story_that_ends_before_its_call_closes_the_viewer(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.stories.live.seen_call = false;
        let watched = app.stories.live.watching;
        if let Some(session) = app.demo_session.as_mut() {
            session.calls.story_stream = None;
            if let Some((chat_id, story_id)) = watched
                && let Some(story) = session.stories.stories.get_mut(&(chat_id.0, story_id))
            {
                story.content = quill::telegram::envelope::StoryContentView::Unsupported;
            }
        }
        app.sync_live_story(cx);
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert!(!app.stories.viewer.is_open(), "closed as an ended stream");
        assert!(app.stories.live.watching.is_none());
    });
}

/// tdesktop plays a live story as a stream of its own beside the call you
/// are in: no card asks, and the call stays.
#[gpui_kit::test]
fn a_live_story_plays_beside_a_one_to_one_call(cx: &mut TestAppContext) {
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            app.demo_ui.seq.store(session.last_seq, Ordering::SeqCst);
            let sink: Arc<dyn quill::diagnostics::DiagnosticSink> = app.demo_ui.sink.clone();
            crate::ui::calls::call_video_demo::seed_video_call(
                session,
                false,
                &sink,
                &app.demo_ui.seq,
            );
        }
        app.sync_live_story(cx);
    });
    frames(vcx);
    app.read_with(vcx, |app, _| {
        let calls = &app.demo_session.as_ref().expect("the demo session").calls;
        assert!(calls.active_call.is_some(), "the call stays");
        assert!(calls.story_stream.is_some(), "the story has its own slot");
        assert!(
            calls.active_group_call.is_none(),
            "it is not the current call"
        );
        assert!(app.stories.viewer.is_open());
    });
    assert!(find(vcx, "live-story-frame").is_some(), "the picture plays");
}

#[gpui_kit::test]
fn the_in_call_demo_shows_the_story_beside_the_call(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story-in-call");
    let vcx = &mut vcx;
    app.read_with(vcx, |app, _| {
        let calls = &app.demo_session.as_ref().expect("the demo session").calls;
        assert!(calls.active_call.is_some());
        assert!(calls.story_stream.is_some());
        assert!(app.stories.viewer.is_open());
    });
    assert!(find(vcx, "live-story-frame").is_some());
}

#[gpui_kit::test]
fn the_toast_goes_once_the_stars_are_due(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story-stars");
    let vcx = &mut vcx;
    assert!(find(vcx, "live-story-toast").is_some());
    app.update(vcx, |app, cx| {
        if let Some(pending) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.stories.live.as_mut())
            .and_then(|live| live.pending_reaction.as_mut())
        {
            pending.commit_at_ms = 1;
        }
        cx.notify();
    });
    frames(vcx);
    assert!(find(vcx, "live-story-toast").is_none());
}

#[gpui_kit::test]
fn the_emoji_panel_closes_with_the_reply_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| app.toggle_live_emoji(cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.story_reply_panel_active()));
    app.update(vcx, |app, cx| app.toggle_live_emoji(cx));
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert!(!app.story_reply_panel_active());
        assert!(!app.stories.reply_open, "the reply row is not left open");
    });
}

/// The header is the live controller's: the LIVE pill and the viewers,
/// the sound toggle, and no scrubber or play button.
#[gpui_kit::test]
fn the_header_is_the_live_controller(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    let badge = find(vcx, "live-badge").expect("the LIVE badge");
    let pill = find(vcx, "live-pill").expect("the pill");
    assert_eq!(pill.size.height, px(11.), "groupCallMessageBadge height");
    // " 2,318 viewers" after the pill.
    assert!(
        badge.size.width > pill.size.width + px(40.),
        "the count follows the pill"
    );
    let row = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.calls.story_stream.as_ref())
            .map(|call| u64::from(call.id.unsigned_abs()))
            .expect("the story's call")
    });
    let present = |vcx: &mut VisualTestContext, id: &'static str| {
        vcx.update(|window, cx| {
            window.render_frame(cx);
            window.try_find((id, row)).is_some()
        })
    };
    assert!(present(vcx, "live-mute"), "the sound toggle");
    assert!(!present(vcx, "viewer-scrubber"));
    assert!(!present(vcx, "media-viewer-play"));
    // A playing story draws no live overlay.
    assert!(
        find(vcx, "live-opening").is_none(),
        "no placeholder over it"
    );
    assert!(find(vcx, "live-spinner").is_none());
    assert!(find(vcx, "live-stalled").is_none());
}

#[gpui_kit::test]
fn a_story_stream_that_failed_offers_retry(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    assert!(find(vcx, "live-failed").is_none());
    app.update(vcx, |app, cx| {
        if let Some(call) = app
            .demo_session
            .as_mut()
            .and_then(|s| s.calls.story_stream.as_mut())
        {
            call.transport_error = Some("another call needs it".into());
        }
        cx.notify();
    });
    frames(vcx);
    assert!(find(vcx, "live-failed").is_some(), "Failed, with Retry");
}

#[gpui_kit::test]
fn an_account_switch_leaves_the_live_story(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story-in-call");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| app.retire_live_to_background(cx));
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert!(!app.stories.viewer.is_open());
        assert!(app.stories.live.watching.is_none());
        let calls = &app.demo_session.as_ref().expect("the demo session").calls;
        assert!(calls.story_stream.is_none(), "the story slot is left");
        assert!(calls.active_call.is_some(), "the call is not the story's");
    });
}

#[gpui_kit::test]
fn an_account_switch_closes_a_story_pip(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| app.open_story_pip(window, cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.live_pip_story().is_some()));
    app.update(vcx, |app, cx| app.retire_live_to_background(cx));
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert!(app.group_call.live_pip.is_none());
        assert!(app.stories.live.watching.is_none());
    });
}

#[gpui_kit::test]
fn a_stream_in_pip_leaves_the_header_of_another_story_alone(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    assert!(find(vcx, "live-badge").is_some());
    app.update_in(vcx, |app, window, cx| app.open_story_pip(window, cx));
    frames(vcx);
    app.update_in(vcx, |app, window, cx| app.demo_story_photo(window, cx));
    frames(vcx);
    app.read_with(vcx, |app, _| {
        assert!(app.stories.live.watching.is_some(), "the PiP plays on");
        assert_eq!(
            app.stories.viewer.current().map(|item| item.story_id),
            super::story_demo::STORIES.first().copied()
        );
        assert_eq!(app.live_story_viewers(), None);
    });
    assert!(find(vcx, "live-badge").is_none(), "no LIVE badge");
    assert!(find(vcx, "story-header-age").is_some(), "the story's age");
    // The reply panel feeds the photo story's reply field, not the hidden
    // live comment field.
    app.update(vcx, |app, cx| {
        app.stories.reply_open = true;
        app.toggle_story_reply_panel(crate::ui::media_panel::PanelTab::Emoji, cx);
    });
    frames(vcx);
    let routed = app.update_in(vcx, |app, window, cx| {
        app.route_story_reply_emoji("👍", window, cx)
    });
    assert!(routed, "the panel is the story reply's");
    app.read_with(vcx, |app, cx| {
        assert_eq!(app.stories.reply_input.read(cx).value().to_string(), "👍");
        assert!(app.stories.live.input.read(cx).value().is_empty());
    });
}

/// tdesktop `setupStarsEffectsCanvas`.
#[gpui_kit::test]
fn star_effects_rise_over_the_stars_button_and_go(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story-stars");
    let vcx = &mut vcx;
    let layer = find(vcx, "live-story-effects").expect("the effects in flight");
    let stars = find(vcx, "live-story-stars").expect("the Stars button");
    assert_eq!(layer.size.width, px(128.));
    assert_eq!(layer.size.height, px(256.));
    assert_eq!(layer.bottom(), stars.bottom());
    assert_eq!(layer.center().x, stars.center().x);
    assert!(find(vcx, "live-story-effect-badge").is_some());
    app.update(vcx, |app, cx| {
        let live = &mut app.stories.live;
        live.demo_clock = live.demo_clock.map(|now| now + EFFECT_DURATION);
        cx.notify();
    });
    frames(vcx);
    assert!(find(vcx, "live-story-effects").is_none());
    assert!(find(vcx, "live-story-effect-badge").is_none());
}

/// The burst's gold frames come from the sticker player's workers, and an
/// effect plays them only when they were ready before it started (tdesktop
/// `ReactionFlyAnimation` resolves its effect once).
#[gpui_kit::test]
fn a_tinted_burst_is_made_off_thread_and_plays_for_effects_after_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_live(cx, "ready-live-story-stars");
    let vcx = &mut vcx;
    let file = FileId(9001);
    let effect = |started| FlyingStars {
        sender: MessageSender::User { user_id: 801 },
        stars: 5,
        started,
        shift: 0.5,
        seed: 0,
    };
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.stories.live_bursts = LiveBursts::Loaded(vec![burst_sticker(file)]);
        }
        let live = &mut app.stories.live;
        let now = Instant::now();
        live.demo_clock = Some(now);
        live.flying.clear();
        live.push_flying(effect(now));
        cx.notify();
    });
    frames(vcx);
    assert!(find(vcx, "live-story-effect-badge").is_some());
    assert!(find(vcx, "live-story-effect-burst").is_none());

    // Three white frames in place of the file.
    app.update(vcx, |app, cx| {
        app.playback.stickers.set_fixture(
            file.0,
            std::sync::Arc::new(|edge| {
                let len = (edge as usize).pow(2) * 4;
                quill::sticker_playback::player::DecodedFrames {
                    frames: vec![vec![255; len]; 3],
                    fps: 30.,
                }
            }),
        );
        cx.notify();
    });
    for _ in 0..3 {
        frames(vcx);
        app.read_with(vcx, |app, _| app.playback.stickers.settle());
    }
    frames(vcx);
    let first_pixel = app.read_with(vcx, |app, _| {
        let image = app.playback.stickers.any_frame(file.0)?;
        let bytes = image.as_bytes(0)?;
        bytes.get(..4).map(<[u8]>::to_vec)
    });
    assert_eq!(first_pixel, Some(vec![0, 184, 255, 255]), "tinted gold");
    assert!(
        find(vcx, "live-story-effect-burst").is_none(),
        "an effect that started before the burst was ready flies alone"
    );

    app.update(vcx, |app, cx| {
        let live = &mut app.stories.live;
        let now = Instant::now() + Duration::from_millis(1);
        live.demo_clock = Some(now);
        live.push_flying(effect(now));
        cx.notify();
    });
    frames(vcx);
    assert!(find(vcx, "live-story-effect-burst").is_some());
}

fn burst_sticker(file: FileId) -> StickerItem {
    StickerItem {
        custom_emoji_id: None,
        id: i64::from(file.0),
        set_id: 0,
        emoji: String::new(),
        width: 512,
        height: 512,
        format: StickerFormat::Tgs,
        file_id: file,
        thumb_file_id: None,
        thumb_width: 0,
        thumb_height: 0,
        requires_premium: false,
        premium_animation: None,
    }
}
