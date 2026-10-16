//! Live story viewer tests: the send button's rules, the pinned strip and
//! the effects clock, and UI flows over the screenshot demo's live story
//! (comments over the picture, the Stars button with its toast and Undo,
//! the comments toggle, the Stars box for a comment, comments disabled,
//! deleting a comment, and the viewer closing when the stream ends).
//! Nothing reaches Telegram: the app has no connection and joins nothing.
use super::live_state::{
    EFFECT_DURATION, SendAction, StarsBoxMode, effect_progress, pin_label, pinned_comments,
    send_action,
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
use quill::story_viewer::{LiveStoryCall, StoryViewerItem, StoryViewerKind};
use quill::telegram::envelope::{MessageSender, ParsedGroupCallMessage};
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
    frames(&mut vcx);
    (app, vcx)
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
            .and_then(|s| s.calls.active_group_call.as_mut())
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
            .and_then(|s| s.calls.active_group_call.as_ref())
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
            .and_then(|s| s.calls.active_group_call.as_mut())
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
    let focused = app.update_in(vcx, |app, window, cx| {
        app.story_text_input_open(window, cx)
    });
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
        assert!(app.connection.status_note.contains("Needed"));
    });
}

#[gpui_kit::test]
fn another_call_is_left_only_after_the_confirm(cx: &mut TestAppContext) {
    use quill::calls::group_call_create::LeaveText;
    let (app, mut vcx) = open_live(cx, "ready-live-story");
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        app.stories.live.leave_other = Some(LeaveText::EndCall { to_channel: true });
        cx.notify();
    });
    frames(vcx);
    assert!(find(vcx, "live-story-leave-other").is_some());
    app.update(vcx, |app, cx| app.leave_other_call_for_live_story(cx));
    frames(vcx);
    assert!(find(vcx, "live-story-leave-other").is_none());
    app.read_with(vcx, |app, _| {
        assert!(app.stories.live.leave_other.is_none());
        assert!(app.stories.live.retry_join, "the join is tried again");
    });
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
