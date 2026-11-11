//! A channel's ads are part of its history, placed as Telegram Desktop
//! places them: the reader scrolls up from one freely (#1427), scrolling
//! past the end adds the next, and ads with `messages_between` go between
//! posts two screens apart.

use crate::ui::app::QuillApp;
use crate::ui::history::demo_chat::frames;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Modifiers, ScrollDelta, ScrollEvent, TestAppContext, TouchPhase,
    VisualTestContext, px, size,
};
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{ChatId, MessageId};
use quill::state::RequestPurpose;
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

/// The demo channel open with 40 more posts, so its history scrolls.
fn open_channel(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(crate::ui::keybindings::bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    app.update(&mut vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
            let seq = AtomicU64::new(session.last_seq);
            session.open_chat(ChatId(13));
            for i in 0..40 {
                let json = format!(
                    r#"{{"@type":"updateNewMessage","message":{{"id":{},"chat_id":13,"sender_id":{{"@type":"messageSenderChat","chat_id":13}},"is_outgoing":false,"is_channel_post":true,"date":{},"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Post {i} with enough words to make a row","entities":[]}}}}}}}}"#,
                    100 + i,
                    1_690_000_000 + i * 600
                );
                if let Some(owned) = copy_and_parse(&json, &seq, &sink) {
                    session.apply(owned);
                }
            }
        }
        cx.notify();
    });
    for _ in 0..5 {
        frames(&mut vcx);
    }
    (app, vcx)
}

/// A new ad list for the channel (two text ads, ids 9101 and 9102), as a
/// refetch delivers it; `unread_after` opens the channel with unread posts
/// after that one.
fn new_ads(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    between: i32,
    unread_after: Option<i64>,
) {
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            if let Some(history) = session.histories.get_mut(&13) {
                history.unread_anchor = unread_after.map(MessageId);
            }
            let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
            let seq = AtomicU64::new(session.last_seq);
            let ad = |id: i64| {
                format!(
                    r#"{{"@type":"sponsoredMessage","message_id":{id},"is_recommended":false,"can_be_reported":true,"content":{{"@type":"messageText","text":{{"@type":"formattedText","text":"Ad {id}","entities":[]}}}},"sponsor":{{"@type":"advertisementSponsor","url":"https://example.com","photo":{{"@type":"photo","has_stickers":false,"sizes":[]}},"info":"Example"}},"title":"Ad","button_text":"Open","accent_color_id":0,"background_custom_emoji_id":"0","additional_info":""}}"#
                )
            };
            let extra = session.request(RequestPurpose::GetChatSponsoredMessages, Some(ChatId(13)));
            let json = format!(
                r#"{{"@type":"sponsoredMessages","@extra":"{}","messages_between":{between},"messages":[{},{}]}}"#,
                extra.0,
                ad(9101),
                ad(9102)
            );
            if let Some(owned) = copy_and_parse(&json, &seq, &sink) {
                session.apply(owned);
            }
        }
        cx.notify();
    });
    for _ in 0..5 {
        frames(vcx);
    }
}

fn placed(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<(i64, i64)> {
    app.read_with(vcx, |app, _| {
        app.history
            .sponsored
            .spots
            .iter()
            .map(|spot| (spot.after.0, spot.ad_id))
            .collect()
    })
}

fn wheel(vcx: &mut VisualTestContext, at: gpui_kit::Point<gpui_kit::Pixels>, dy: f32) {
    vcx.update(|window, cx| window.render_frame(cx));
    vcx.run_until_parked();
    for (phase, dy) in [
        (TouchPhase::Started, 0.),
        (TouchPhase::Moved, dy),
        (TouchPhase::Ended, 0.),
    ] {
        vcx.simulate_event(ScrollEvent {
            position: at,
            delta: ScrollDelta::Pixels(gpui_kit::point(px(0.), px(dy))),
            modifiers: Modifiers::default(),
            touch_phase: phase,
            source: gpui_kit::ScrollSource::Pointer,
        });
    }
}

fn last_post(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> i64 {
    app.read_with(vcx, |app, _| {
        app.history
            .rows
            .last()
            .and_then(|row| row.last_id())
            .map_or(0, |id| id.0)
    })
}

fn scrolled_up(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, cx| {
        app.history.scroller.read(cx).is_scrolled_up()
    })
}

/// Scrolling up a little from the ad stays where the reader put it; before
/// #1427 the ad left the layout, the list grew into its space, landed at
/// the end again and the ad pulled the view back down.
#[gpui_kit::test]
fn scrolling_up_from_the_ad_stays_up(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_channel(cx);
    let vcx = &mut vcx;
    new_ads(&app, vcx, 0, None);
    assert!(scroll_past_end(&app, vcx));
    let last = last_post(&app, vcx);
    assert_eq!(
        placed(&app, vcx),
        vec![(last, 9101)],
        "the channel ends with its ad"
    );
    let footer = vcx.debug_bounds("sponsored-ad-9101").expect("ad painted");
    let viewport = app
        .read_with(vcx, |app, _| app.history.viewport.get())
        .expect("history painted");
    assert!(
        footer.bottom() <= viewport.bottom() + px(0.5),
        "the bottom of the history shows the whole ad"
    );
    assert!(!scrolled_up(&app, vcx));

    for step in 1..=6 {
        wheel(vcx, viewport.center(), 30.);
        for _ in 0..4 {
            frames(vcx);
        }
        assert!(
            scrolled_up(&app, vcx),
            "wheel step {step} up from the ad was pulled back to the bottom"
        );
    }
}

/// The reader scrolls 30px toward newer messages (the history's wheel
/// listener and PageDown pass the distance on).
fn scroll_past_end(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    let added = app.update(vcx, |app, cx| app.sponsored_bottom_request(px(30.), cx));
    for _ in 0..4 {
        frames(vcx);
    }
    added
}

/// tdesktop `setBottomContentRequest`: no ad at first; scrolling past the
/// last post adds one, and the list runs into it.
#[gpui_kit::test]
fn scrolling_past_the_end_adds_the_next_ad(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_channel(cx);
    let vcx = &mut vcx;
    new_ads(&app, vcx, 0, None);
    assert!(placed(&app, vcx).is_empty(), "no ad before the reader asks");
    let viewport = app
        .read_with(vcx, |app, _| app.history.viewport.get())
        .expect("history painted");
    let last = last_post(&app, vcx);
    assert!(scroll_past_end(&app, vcx));
    assert_eq!(placed(&app, vcx), vec![(last, 9101)]);
    let ad = vcx.debug_bounds("sponsored-ad-9101").expect("ad painted");
    assert!(
        ad.bottom() <= viewport.bottom() + px(0.5),
        "the list followed into the new ad"
    );
    assert!(!scrolled_up(&app, vcx));
    // Past the end again: the second ad follows the first.
    assert!(scroll_past_end(&app, vcx));
    assert_eq!(placed(&app, vcx), vec![(last, 9101), (last, 9102)]);
    assert!(!scroll_past_end(&app, vcx), "every ad is shown");
}

/// Scrolled up from the end, a wheel step that stays inside the history
/// adds nothing.
#[gpui_kit::test]
fn a_scroll_inside_the_history_adds_no_ad(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_channel(cx);
    let vcx = &mut vcx;
    new_ads(&app, vcx, 0, None);
    let viewport = app
        .read_with(vcx, |app, _| app.history.viewport.get())
        .expect("history painted");
    wheel(vcx, viewport.center(), 200.);
    for _ in 0..4 {
        frames(vcx);
    }
    assert!(scrolled_up(&app, vcx));
    assert!(!scroll_past_end(&app, vcx));
    assert!(placed(&app, vcx).is_empty());
}

/// tdesktop `inject`: with `messages_between`, the first ad goes after at
/// least that many posts and two screens of rows past the first unread
/// post; the second finds too few rows after it and waits.
#[gpui_kit::test]
fn ads_go_between_posts_two_screens_apart(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_channel(cx);
    let vcx = &mut vcx;
    new_ads(&app, vcx, 3, Some(110));
    let spots = placed(&app, vcx);
    let [(after, 9101)] = spots.as_slice() else {
        panic!("one ad between posts, got {spots:?}");
    };
    let (rows_after, viewport) = app.read_with(vcx, |app, cx| {
        let rows = &app.history.rows;
        let first_unread = rows
            .iter()
            .position(|row| row.contains(MessageId(111)))
            .expect("first unread row");
        let anchor = rows
            .iter()
            .position(|row| row.contains(MessageId(*after)))
            .expect("anchor row");
        let heights = app
            .history
            .scroller
            .read(cx)
            .estimated_row_heights(first_unread + 1..anchor + 1)
            .expect("rows measured");
        let height: f32 = heights.iter().map(|h| f32::from(*h)).sum();
        let viewport = app.history.viewport.get().expect("painted").size.height;
        (height, f32::from(viewport))
    });
    assert!(*after - 111 >= 3, "at least three posts apart: {after}");
    assert!(
        rows_after >= viewport * 2.,
        "two screens apart: {rows_after} < 2 x {viewport}"
    );
    // Not one row earlier: the ad sits at the first row that is enough.
    let last = last_post(&app, vcx);
    assert!(*after < last, "room was left at the end: {after}");
}
