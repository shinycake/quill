//! Suggested posts in the demo's channel direct messages: the toggle and
//! the bar, the price box filling the bar, the decision buttons, Accept
//! and Decline (needs the `demo-capture` feature for gpui-kit's test
//! support). Nothing reaches Telegram: the app has no connection.
use super::{CHAT, COUNTER, OFFER};
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::suggested_posts_state::{SuggestTarget, TimeTarget};
use crate::ui::wall_clock::WallClock;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};
use quill::suggested_posts::{
    DirectMessagesRole, SuggestMode, SuggestTerms, SuggestedPostState, SuggestedPrice,
};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-suggested-posts")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().expect("the app view");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

fn state_of(
    app: &Entity<QuillApp>,
    vcx: &mut VisualTestContext,
    id: i64,
) -> Option<SuggestedPostState> {
    app.update_in(vcx, |app, _, _| {
        app.session()
            .and_then(|s| s.histories.get(&CHAT))
            .and_then(|h| h.messages.get(&id))
            .and_then(|m| m.extras.suggested_post)
            .map(|info| info.state)
    })
}

#[gpui_kit::test]
fn the_toggle_shows_for_a_member_only_while_the_bar_is_down(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        assert_eq!(
            app.suggest_role(ChatId(CHAT)),
            Some(DirectMessagesRole::Member)
        );
        // The demo starts with the bar up: no toggle, the bar shows.
        assert!(app.suggest_bar_panel(cx).is_some());
        assert!(app.suggest_post_toggle(cx).is_none());
        app.set_suggest_bar(ChatId(CHAT), None, cx);
        assert!(app.suggest_bar_panel(cx).is_none());
        assert!(app.suggest_post_toggle(cx).is_some());
        // An admin has no toggle.
        if let Some(session) = app.demo_session.as_mut() {
            session
                .groups
                .supergroup_direct_messages
                .insert(777, DirectMessagesRole::Admin);
        }
        assert!(app.suggest_post_toggle(cx).is_none());
    });
}

#[gpui_kit::test]
fn the_toggle_takes_the_reply_s_place(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        app.set_suggest_bar(ChatId(CHAT), None, cx);
        app.chat_ui
            .set_reply(Some(quill::composer::ComposerReplyTo::new(
                ChatId(CHAT),
                MessageId(COUNTER),
            )));
        assert!(app.suggest_post_toggle(cx).is_some());
        app.toggle_suggest_post(ChatId(CHAT), cx);
        assert!(app.chat_ui.reply().is_none());
        let bar = app.session().and_then(|s| s.suggest_bar(ChatId(CHAT)));
        assert_eq!(bar.map(|bar| bar.terms), Some(SuggestTerms::default()));
        // No box opens: the bar's line asks for the terms.
        assert!(app.suggested_posts.price().is_none());
        assert!(app.suggest_post_toggle(cx).is_none());
    });
}

#[gpui_kit::test]
fn the_price_box_fills_the_bar_and_rejects_a_too_small_price(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.open_suggest_price(
            SuggestTarget::Bar(ChatId(CHAT)),
            SuggestMode::New,
            SuggestTerms::default(),
            false,
            window,
            cx,
        );
        let input = app
            .suggested_posts
            .price()
            .map(|price| price.input.clone())
            .expect("the price box");
        input.update(cx, |input, cx| input.set_value("3", window, cx));
        assert!(!app.submit_suggest_price(cx));
        assert_eq!(
            app.suggested_posts
                .price()
                .and_then(|price| price.error.clone())
                .as_deref(),
            Some("Minimum price is 5 Stars.")
        );
        input.update(cx, |input, cx| input.set_value("40", window, cx));
        assert!(app.submit_suggest_price(cx));
        assert!(app.suggested_posts.price().is_none());
        let bar = app.session().and_then(|s| s.suggest_bar(ChatId(CHAT)));
        assert_eq!(
            bar.map(|bar| bar.terms.price),
            Some(Some(SuggestedPrice::Stars(40)))
        );
    });
    vcx.run_until_parked();
}

#[gpui_kit::test]
fn the_time_box_answers_the_price_box(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.open_suggest_price(
            SuggestTarget::Bar(ChatId(CHAT)),
            SuggestMode::New,
            SuggestTerms {
                price: None,
                send_date: 1,
            },
            false,
            window,
            cx,
        );
        app.open_suggest_time(TimeTarget::PriceBox, SuggestMode::New, 0, window, cx);
        assert!(app.suggested_posts.time().is_some());
        // "Anytime".
        app.submit_suggest_time(true, window, cx);
        assert!(app.suggested_posts.time().is_none());
        assert_eq!(
            app.suggested_posts.price().map(|price| price.send_date),
            Some(0)
        );
    });
}

#[gpui_kit::test]
fn only_the_counter_offer_has_decision_buttons(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.run_until_parked();
    app.update_in(vcx, |app, _, cx| {
        let message = |id: i64| {
            app.session()
                .and_then(|s| s.histories.get(&CHAT))
                .and_then(|h| h.messages.get(&id))
                .cloned()
                .expect("the demo message")
        };
        let counter = message(COUNTER);
        let offer = message(OFFER);
        assert!(
            crate::ui::history::suggested_post_block::suggestion_buttons(&counter, cx).is_some()
        );
        assert!(crate::ui::history::suggested_post_block::suggestion_buttons(&offer, cx).is_none());
        let session = app.session().expect("the demo session");
        assert!(
            crate::ui::history::suggested_post_block::suggested_post_block(session, &counter, cx)
                .is_some()
        );
    });
}

#[gpui_kit::test]
fn accepting_asks_for_the_terms_then_publishes(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.begin_suggest_accept(ChatId(CHAT), MessageId(COUNTER), window, cx);
        let text = app
            .suggested_posts
            .accept()
            .map(|accept| accept.text.clone())
            .expect("the Accept Terms box");
        assert!(text.starts_with("Do you want to publish this post to Quill Weekly?"));
        assert!(text.contains("You will pay 750 Stars for publishing"));
        app.confirm_suggest_accept(cx);
        assert!(app.suggested_posts.accept().is_none());
    });
    assert_eq!(
        state_of(&app, vcx, COUNTER),
        Some(SuggestedPostState::Approved)
    );
}

fn set_busy(app: &mut QuillApp, busy: bool) {
    let Some(session) = app.demo_session.as_mut() else {
        return;
    };
    use quill::state::{PaymentsPurpose, RequestPurpose};
    use quill::suggested_posts::SuggestedPostPurpose;
    let purpose = RequestPurpose::Payments(PaymentsPurpose::SuggestedPost(
        SuggestedPostPurpose::Approve {
            chat_id: CHAT,
            message_id: COUNTER,
        },
    ));
    if busy {
        let request = session.request(purpose, Some(ChatId(CHAT)));
        session.decision_sent(CHAT, COUNTER, request);
    } else {
        drop(session.requests.take_purpose(purpose));
        let out: Vec<_> = session
            .payments
            .suggested_posts
            .deciding
            .iter()
            .map(|decision| decision.request)
            .collect();
        for request in out {
            session.decision_settled(request);
        }
    }
}

#[gpui_kit::test]
fn a_publish_in_flight_refuses_a_second_terms_box_and_a_decided_post_closes_one(
    cx: &mut TestAppContext,
) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        // The first Publish is out: opening the box again opens nothing.
        set_busy(app, true);
        app.open_suggest_accept(ChatId(CHAT), MessageId(COUNTER), 0, window, cx);
        assert!(app.suggested_posts.accept().is_none());
        set_busy(app, false);
        app.open_suggest_accept(ChatId(CHAT), MessageId(COUNTER), 0, window, cx);
        app.drain_suggested_post_notes(cx);
        assert!(app.suggested_posts.accept().is_some(), "still pending");
        // The post is decided elsewhere.
        if let Some(session) = app.demo_session.as_mut() {
            session.edit_suggested_post(ChatId(CHAT), MessageId(COUNTER), |info| {
                if let Some(info) = info.as_mut() {
                    info.state = SuggestedPostState::Approved;
                }
            });
        }
        app.drain_suggested_post_notes(cx);
        assert!(
            app.suggested_posts.accept().is_none(),
            "the box of a published post closes"
        );
    });
}

/// Sweep r4-12: a Decline is not opened or confirmed over a Publish that is
/// still out; the post's own answer settles it.
#[gpui_kit::test]
fn a_publish_in_flight_refuses_a_decline_box_and_a_decline_confirm(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_suggest_decline(ChatId(CHAT), MessageId(COUNTER), window, cx);
        assert!(app.suggested_posts.decline().is_some(), "free: it opens");
        // The Publish goes out while the box is open: confirming sends
        // nothing and closes the box.
        set_busy(app, true);
        app.confirm_suggest_decline(cx);
        assert!(app.suggested_posts.decline().is_none());
        app.open_suggest_decline(ChatId(CHAT), MessageId(COUNTER), window, cx);
        assert!(app.suggested_posts.decline().is_none(), "busy: no box");
    });
    assert_eq!(
        state_of(&app, &mut vcx, COUNTER),
        Some(SuggestedPostState::Proposed),
        "not declined"
    );
}

/// The Decline box closes with nothing sent when the post is decided
/// elsewhere, as the Accept box does; a Decline confirmed after that sends
/// nothing either.
#[gpui_kit::test]
fn a_decided_post_closes_its_decline_box_and_declines_nothing(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_suggest_decline(ChatId(CHAT), MessageId(COUNTER), window, cx);
        app.drain_suggested_post_notes(cx);
        assert!(app.suggested_posts.decline().is_some(), "still pending");
        if let Some(session) = app.demo_session.as_mut() {
            session.edit_suggested_post(ChatId(CHAT), MessageId(COUNTER), |info| {
                if let Some(info) = info.as_mut() {
                    info.state = SuggestedPostState::Approved;
                }
            });
        }
        app.drain_suggested_post_notes(cx);
        assert!(
            app.suggested_posts.decline().is_none(),
            "the box of a decided post closes"
        );
        // A confirm that races the closer declines nothing.
        app.open_suggest_decline(ChatId(CHAT), MessageId(COUNTER), window, cx);
        app.confirm_suggest_decline(cx);
    });
    assert_eq!(
        state_of(&app, &mut vcx, COUNTER),
        Some(SuggestedPostState::Approved),
        "not turned into Declined"
    );
}

#[gpui_kit::test]
fn the_accept_terms_date_reads_the_pickers_clock(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        cx.set_global(WallClock::chicago_repeated_hour());
        // 01:20 CST, fifteen minutes ahead on the Chicago clock.
        let send_date = i32::try_from(WallClock::CHICAGO_FALL_BACK + 1200).unwrap_or(0);
        app.open_suggest_accept(ChatId(CHAT), MessageId(COUNTER), send_date, window, cx);
        let text = app
            .suggested_posts
            .accept()
            .map(|accept| accept.text.clone())
            .expect("the Accept Terms box");
        assert!(text.contains("Nov 1 at 01:20"), "{text}");
    });
}

#[gpui_kit::test]
fn the_history_block_time_reads_the_pickers_clock(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.set_global(WallClock::chicago_repeated_hour());
        // 01:20 CST, fifteen minutes ahead on the Chicago clock.
        let send_date = i32::try_from(WallClock::CHICAGO_FALL_BACK + 1200).unwrap_or(0);
        assert_eq!(
            crate::ui::history::suggested_post_block::block_time(send_date, cx),
            "today at 01:20"
        );
    });
}

#[gpui_kit::test]
fn declining_takes_a_reason(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        app.open_suggest_decline(ChatId(CHAT), MessageId(COUNTER), window, cx);
        let decline = app.suggested_posts.decline().expect("the Decline box");
        assert_eq!(
            decline.text,
            "Do you want to decline publishing this post to Quill Weekly?"
        );
        let reason = decline.reason.clone();
        reason.update(cx, |input, cx| input.set_value("Not this week", window, cx));
        app.confirm_suggest_decline(cx);
        assert!(app.suggested_posts.decline().is_none());
    });
    assert_eq!(
        state_of(&app, vcx, COUNTER),
        Some(SuggestedPostState::Declined)
    );
}

#[gpui_kit::test]
fn add_offer_is_offered_for_plain_messages_only(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, _| {
        // Both demo messages are suggestions already.
        assert!(!app.can_add_offer(ChatId(CHAT), MessageId(OFFER)));
        if let Some(session) = app.demo_session.as_mut() {
            session.edit_suggested_post(ChatId(CHAT), MessageId(OFFER), |info| {
                *info = None;
            });
        }
        assert!(app.can_add_offer(ChatId(CHAT), MessageId(OFFER)));
    });
}
