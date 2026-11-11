//! Gift boxes act for the gift and the recipient they were opened for:
//! picking another gift drops the first one's Upgrade confirmation, and
//! "Buy a collectible by name" from the Send Gift box buys for that
//! box's recipient, not the chat open behind it.

use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::{TestAppContext, WindowHandle};
use quill::gift_manage::{GiftFlow, GiftRecipient};
use quill::ids::ChatId;
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
        .update(cx, |app, _, cx| {
            if let Some(manage) = app.gift_manage_mut() {
                manage.flow = Some(GiftFlow::Upgrade {
                    id: "first".into(),
                    keep_details: true,
                });
            }
            app.select_gift(Some("first".into()), cx);
            let flow = |app: &mut QuillApp| app.gift_manage_mut().and_then(|m| m.flow.clone());
            assert!(flow(app).is_some(), "the same gift keeps its box");
            app.select_gift(Some("second".into()), cx);
            assert_eq!(flow(app), None);
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
                send.open = true;
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
            assert!(app.payments.marketplace_open);
            assert_eq!(app.payments.marketplace_chat, Some(ChatId(501)));
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
                    quote: None,
                    price: None,
                    loading: true,
                    sending: false,
                    completed: false,
                    note: None,
                });
            }
            app.payments.marketplace_chat = Some(ChatId(501));
            app.open_marketplace_for(Some(ChatId(502)), window, cx);
            assert!(!app.payments.marketplace_open, "B's box does not open");
            assert_eq!(app.payments.marketplace_chat, Some(ChatId(501)));
            app.open_marketplace_for(Some(ChatId(501)), window, cx);
            assert!(app.payments.marketplace_open, "A's own box reopens");
        })
        .unwrap();
}
