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
