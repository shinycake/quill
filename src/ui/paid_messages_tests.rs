//! Paid messages on a demo `QuillApp`: the gate, the confirmation before a
//! paid send, the Messages price and Remove Fee. No Stars ever move: the
//! demo session has no sender.

use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::chat_bottom_bar::BottomBar;
use quill::ids::ChatId;
use quill::paid_messages::{CanSend, SendGate};
use quill::privacy::{NewChatPrivacy, NewChatPrivacyState};
use quill::state::UserFullInfoData;
use quill::telegram::envelope::{ChannelMemberStatus, SupergroupFullAdmin};

fn new_app(cx: &mut TestAppContext, demo: &str) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named(demo);
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, vcx)
}

/// Chat 11 is a private chat with user 11 that charges `stars`.
fn charge_in_chat(app: &mut QuillApp, stars: i64) {
    let session = app.demo_session.as_mut().expect("demo session");
    session.open_chat(ChatId(11));
    session
        .payments
        .paid_messages
        .record_answer(11, CanSend::HasPaidMessages { stars }, 1);
}

#[gpui_kit::test]
fn a_paid_chat_asks_once_then_sends_with_the_price(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        charge_in_chat(app, 25);
        assert!(app.paid_send_waits(ChatId(11), "hello", cx));
    });
    app.read_with(vcx, |app, _| {
        let confirm = app.paid_messages.confirm.as_ref().expect("asks first");
        assert_eq!(confirm.per_message, 25);
        assert_eq!(confirm.total(), 25);
        assert_eq!(confirm.text, "hello");
    });
    // "Don't ask me again" for this price makes the next send go through.
    app.update_in(vcx, |app, _, _| {
        let session = app.demo_session.as_mut().unwrap();
        session.payments.paid_messages.trust.trust(11, 25);
        app.paid_messages.confirm = None;
    });
    app.update_in(vcx, |app, _, cx| {
        assert!(!app.paid_send_waits(ChatId(11), "again", cx));
    });
    // A lower price is covered, a higher one asks again.
    app.update_in(vcx, |app, _, cx| {
        charge_in_chat(app, 10);
        assert!(!app.paid_send_waits(ChatId(11), "x", cx));
        charge_in_chat(app, 30);
        assert!(app.paid_send_waits(ChatId(11), "x", cx));
    });
}

#[gpui_kit::test]
fn free_chats_never_wait(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().unwrap();
        session.open_chat(ChatId(11));
        assert!(!app.paid_send_waits(ChatId(11), "hi", cx));
        assert!(app.paid_messages.confirm.is_none());
    });
}

#[gpui_kit::test]
fn restricted_user_replaces_the_composer_with_the_premium_bar(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, _| {
        let session = app.demo_session.as_mut().unwrap();
        session.open_chat(ChatId(11));
        session.payments.premium_option = Some(false);
        session
            .payments
            .paid_messages
            .record_answer(11, CanSend::RestrictsNewChats, 1);
        assert_eq!(session.send_gate(ChatId(11)), SendGate::PremiumRequired);
        assert_eq!(app.bottom_action(), Some(BottomBar::PremiumRequired));
    });
    // Premium lifts the gate.
    app.update_in(vcx, |app, _, _| {
        let session = app.demo_session.as_mut().unwrap();
        session.payments.premium_option = Some(true);
        assert_eq!(app.bottom_action(), None);
    });
}

#[gpui_kit::test]
fn charging_starts_only_with_save(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-privacy");
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().unwrap();
        session.settings.privacy_data.new_chat = Some(NewChatPrivacyState::Ready(NewChatPrivacy {
            allow_from_unknown: true,
            incoming_paid_message_star_count: 0,
        }));
        session.payments.premium_option = Some(true);
        session.payments.paid_messages.can_enable = Some(true);
        app.open_charge_draft(cx);
        let draft = app.paid_messages.charge.as_mut().expect("slider opens");
        assert_eq!(draft.price, 10, "starts on the tdesktop default");
        draft.price = 40;
    });
    app.read_with(vcx, |app, _| {
        let data = &app.demo_session.as_ref().unwrap().settings.privacy_data;
        assert!(matches!(
            data.new_chat,
            Some(NewChatPrivacyState::Ready(s)) if s.incoming_paid_message_star_count == 0
        ));
    });
    app.update_in(vcx, |app, _, cx| app.save_charge_price(cx));
    app.read_with(vcx, |app, _| {
        let data = &app.demo_session.as_ref().unwrap().settings.privacy_data;
        assert!(matches!(
            data.new_chat,
            Some(NewChatPrivacyState::Ready(s))
                if s.incoming_paid_message_star_count == 40 && s.allow_from_unknown
        ));
        assert!(app.paid_messages.charge.is_none());
    });
    // Everybody ends the charge.
    app.update_in(vcx, |app, _, cx| {
        app.pick_messages_policy(quill::paid_messages::MessagesPolicy::Everybody, cx);
    });
    app.read_with(vcx, |app, _| {
        let data = &app.demo_session.as_ref().unwrap().settings.privacy_data;
        assert!(matches!(
            data.new_chat,
            Some(NewChatPrivacyState::Ready(s)) if s.incoming_paid_message_star_count == 0
        ));
    });
}

#[gpui_kit::test]
fn charging_needs_premium(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-privacy");
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().unwrap();
        session.settings.privacy_data.new_chat = Some(NewChatPrivacyState::Ready(NewChatPrivacy {
            allow_from_unknown: true,
            incoming_paid_message_star_count: 0,
        }));
        session.payments.premium_option = Some(false);
        app.open_charge_draft(cx);
        assert!(app.paid_messages.charge.is_none());
        assert!(app.connection.status_note.contains("Premium"));
    });
}

#[gpui_kit::test]
fn remove_fee_clears_the_bar_and_the_price(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().unwrap();
        session.open_chat(ChatId(11));
        let mut info = UserFullInfoData::default();
        info.extras.incoming_paid_message_star_count = 5;
        session.users_state.user_full_infos.insert(11, info);
        assert_eq!(session.user_pays_me(ChatId(11)), 5);
        app.open_remove_fee(11, cx);
        assert!(app.paid_messages.remove_fee.is_some());
        app.confirm_remove_fee(cx);
    });
    app.read_with(vcx, |app, _| {
        let session = app.demo_session.as_ref().unwrap();
        assert_eq!(session.user_pays_me(ChatId(11)), 0);
        assert!(app.paid_messages.remove_fee.is_none());
    });
}

#[gpui_kit::test]
fn only_the_owner_of_a_channel_sees_direct_messages(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx, "ready-chats");
    let vcx = &mut vcx;
    app.update_in(vcx, |app, _, cx| {
        let session = app.demo_session.as_mut().unwrap();
        let supergroup = session.chat_supergroup(ChatId(13)).expect("demo channel");
        session.groups.supergroup_full_infos.insert(
            supergroup,
            quill::state::SupergroupFullInfoData {
                admin: SupergroupFullAdmin {
                    can_enable_paid_messages: true,
                    ..SupergroupFullAdmin::default()
                },
                ..quill::state::SupergroupFullInfoData::default()
            },
        );
        // Not the owner: no box opens from the row's action.
        assert!(!session.chat_is_owner(ChatId(13)));
        session.chats.get_mut(&13).unwrap().my_member_status = Some(ChannelMemberStatus::Creator);
        app.open_direct_messages(ChatId(13), cx);
        let boxed = app.paid_messages.direct.as_ref().expect("opens");
        assert!(!boxed.enabled);
        assert_eq!(boxed.price.steps.first(), Some(&0));
    });
}
