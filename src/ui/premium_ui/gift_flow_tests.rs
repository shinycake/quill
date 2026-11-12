//! Gift boxes act for the gift and the recipient they were opened for:
//! picking another gift drops the first one's Upgrade confirmation, and
//! "Buy a collectible by name" from the Send Gift box buys for that
//! box's recipient, not the chat open behind it.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::{TestAppContext, WindowHandle};
use quill::gift_manage::GiftRecipient;
use quill::ids::ChatId;
use quill::premium_hub::gift_view::GiftForm;
use quill::telegram::envelope::MessageSender;

fn demo(cx: &mut TestAppContext) -> WindowHandle<QuillApp> {
    cx.update(gpui_kit::init);
    cx.add_window(|window, cx| {
        QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
    })
}

#[gpui_kit::test]
fn another_gift_drops_the_first_gifts_upgrade_box(cx: &mut TestAppContext) {
    let window = demo(cx);
    window
        .update(cx, |app, window, cx| {
            if let Some(hub) = app.hub_mut() {
                hub.received.open();
            }
            app.select_gift(Some("first".into()), cx);
            app.set_gift_form(Some(GiftForm::upgrade(true)), window, cx);
            let form =
                |app: &mut QuillApp| app.hub_mut().and_then(|h| h.received.view.form().cloned());
            assert!(form(app).is_some(), "the box opens");
            app.select_gift(Some("first".into()), cx);
            assert!(form(app).is_some(), "the same gift keeps its box");
            app.select_gift(Some("second".into()), cx);
            assert_eq!(form(app), None);
            app.set_gift_form(Some(GiftForm::resale()), window, cx);
            app.select_gift(None, cx);
            assert_eq!(form(app), None, "closing the panel closes the form");
        })
        .unwrap();
}

#[gpui_kit::test]
fn buy_by_name_from_send_gift_buys_for_its_recipient(cx: &mut TestAppContext) {
    let window = demo(cx);
    window
        .update(cx, |app, window, cx| {
            if let Some(session) = app.demo_session.as_mut() {
                session.open_chat(ChatId(11));
                let send = &mut session.payments.gifts_manage.send;
                send.stage = quill::gift_manage::SendStage::Idle;
                send.recipient = Some(GiftRecipient {
                    sender: MessageSender::User { user_id: 501 },
                    name: "Ada".into(),
                    is_self: false,
                    is_channel: false,
                });
            }
            let chat = app.send_gift_recipient_chat();
            assert_eq!(chat, Some(ChatId(501)));
            app.open_marketplace_for(chat, window, cx);
            assert!(app.payments.marketplace.is_open());
            assert_eq!(app.payments.marketplace.chat(), Some(ChatId(501)));
        })
        .unwrap();
}

/// Sweep 10-171: a purchase on its way stays the chat it started in; the
/// box opened from another chat does not take it over.
#[gpui_kit::test]
fn a_purchase_on_its_way_is_not_taken_over_by_another_chats_box(cx: &mut TestAppContext) {
    let window = demo(cx);
    window
        .update(cx, |app, window, cx| {
            if let Some(session) = app.demo_session.as_mut() {
                session.payments.marketplace_gift = Some(quill::marketplace::GiftPurchase {
                    chat_id: ChatId(501),
                    recipient: MessageSender::User { user_id: 501 },
                    recipient_name: "Ada".into(),
                    requested_name: "plush".into(),
                    asked: quill::ids::RequestId(1),
                    stage: quill::marketplace::GiftPurchaseStage::Loading(quill::ids::RequestId(1)),
                });
            }
            app.payments.marketplace.open_for(ChatId(501), false);
            app.payments.marketplace.close();
            app.open_marketplace_for(Some(ChatId(502)), window, cx);
            assert!(!app.payments.marketplace.is_open(), "B's box does not open");
            assert_eq!(app.payments.marketplace.chat(), Some(ChatId(501)));
            app.open_marketplace_for(Some(ChatId(501)), window, cx);
            assert!(app.payments.marketplace.is_open(), "A's own box reopens");
        })
        .unwrap();
}

/// A finished purchase of an earlier box does not show in a fresh one for
/// the same chat, and opening does not reset the session's copy by hand.
#[gpui_kit::test]
fn a_fresh_box_hides_the_purchase_of_an_earlier_one(cx: &mut TestAppContext) {
    let window = demo(cx);
    window
        .update(cx, |app, window, cx| {
            if let Some(session) = app.demo_session.as_mut() {
                session.payments.marketplace_gift = Some(quill::marketplace::GiftPurchase {
                    chat_id: ChatId(501),
                    recipient: MessageSender::User { user_id: 501 },
                    recipient_name: "Ada".into(),
                    requested_name: "plush".into(),
                    asked: quill::ids::RequestId(1),
                    stage: quill::marketplace::GiftPurchaseStage::Failed("old".into()),
                });
            }
            app.open_marketplace_for(Some(ChatId(501)), window, cx);
            let draft = app.payments.marketplace.draft().cloned();
            let gift = app.session().and_then(|s| s.payments.marketplace_gift.clone());
            assert!(gift.is_some(), "the session's copy is not reset by the box");
            assert!(draft.is_some_and(|d| gift.is_some_and(|g| !d.shows(&g))));
        })
        .unwrap();
}
