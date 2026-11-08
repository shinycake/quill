use super::super::direct_messages::{CHANNEL, DIRECT};
use super::{MAYA, PRIYA, SAM};
use crate::ui::app::QuillApp;
use crate::ui::direct_messages_tabs::{DirectAction, direct_menu_actions};
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::subsection_tabs::Tab;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::composer::ComposerReplyTo;
use quill::ids::{ChatId, MessageId, TopicId};
use quill::lang::{keys, tr};

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Viewer {
    Subscriber,
    AdminInAll,
    AdminInMayasChat,
}

fn open_demo(cx: &mut TestAppContext, viewer: Viewer) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-direct-messages");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    if viewer != Viewer::Subscriber {
        let all = viewer == Viewer::AdminInAll;
        vcx.update(|window, cx| app.update(cx, |app, cx| app.demo_direct_admin(all, window, cx)));
        frames(&mut vcx);
    }
    (app, vcx)
}

fn open_topic(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Option<i64> {
    app.read_with(vcx, |app, _| {
        app.session()?.open_direct_topic(ChatId(DIRECT))
    })
}

fn shown_ids(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> Vec<i64> {
    app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.histories.get(&DIRECT))
            .map(|h| h.messages.keys().copied().collect())
            .unwrap_or_default()
    })
}

fn pick_tab(app: &Entity<QuillApp>, vcx: &mut VisualTestContext, tab: Tab) {
    app.update(vcx, |app, cx| app.select_subsection_tab(tab, cx));
    frames(vcx);
}

#[gpui_kit::test]
fn all_asks_to_choose_a_message_until_a_reply_is_picked(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, Viewer::AdminInAll);
    let vcx = &mut vcx;
    assert_eq!(open_topic(&app, vcx), None);
    assert_eq!(
        shown_ids(&app, vcx).len(),
        7,
        "All shows every chat's messages"
    );
    let (restriction, composer, status) = app.read_with(vcx, |app, _| {
        (
            app.composer_restriction(),
            app.composer_available(app.pane_mode()),
            app.direct_admin_header().map(|h| h.status),
        )
    });
    assert_eq!(
        restriction.as_deref(),
        Some(tr(keys::MONOFORUM_CHOOSE_TO_REPLY))
    );
    assert!(!composer, "no composer until a message is chosen");
    assert_eq!(status.as_deref(), Some("3 chats"));

    app.update(vcx, |app, _| {
        app.chat_ui
            .set_reply(Some(ComposerReplyTo::new(ChatId(DIRECT), MessageId(2001))));
    });
    let (restriction, composer) = app.read_with(vcx, |app, _| {
        (
            app.composer_restriction(),
            app.composer_available(app.pane_mode()),
        )
    });
    assert_eq!(restriction, None, "replying lifts the lock");
    assert!(composer);
    let topic = app.read_with(vcx, |app, _| {
        app.session()?
            .direct_topic_of_message(ChatId(DIRECT), MessageId(2001))
    });
    assert_eq!(topic, Some(SAM), "the reply goes to Sam's chat");
}

#[gpui_kit::test]
fn a_tab_opens_one_persons_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, Viewer::AdminInAll);
    let vcx = &mut vcx;
    pick_tab(&app, vcx, Tab::Direct(MAYA));
    assert_eq!(open_topic(&app, vcx), Some(MAYA));
    assert_eq!(shown_ids(&app, vcx), vec![1001, 1002, 1003, 1004]);
    let (restriction, title, send_topic, unread) = app.read_with(vcx, |app, _| {
        let session = app.session();
        (
            app.composer_restriction(),
            app.direct_admin_header().and_then(|h| h.title),
            session.and_then(|s| s.send_topic(ChatId(DIRECT))),
            session.and_then(|s| s.direct_unread_count(ChatId(DIRECT))),
        )
    });
    assert_eq!(restriction, None, "a person's chat takes messages");
    assert_eq!(title.as_deref(), Some("Maya Chen"));
    assert_eq!(
        send_topic,
        Some(TopicId::DirectMessages {
            direct_messages_chat_topic_id: MAYA
        })
    );
    assert_eq!(unread, Some(2), "the jump-down counter is hers");
    assert!(
        vcx.update(|window, _| window.try_find("direct-topic-menu").is_some()),
        "paid messages give the open chat its menu"
    );

    pick_tab(&app, vcx, Tab::All);
    assert_eq!(open_topic(&app, vcx), None);
}

#[gpui_kit::test]
fn the_tab_menu_offers_read_ban_and_delete(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, Viewer::AdminInMayasChat);
    let vcx = &mut vcx;
    let actions = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext, topic: i64| {
        app.read_with(vcx, |app, _| {
            app.session()
                .map(|s| direct_menu_actions(s, ChatId(DIRECT), topic))
                .unwrap_or_default()
        })
    };
    let ban = DirectAction::Ban {
        channel: ChatId(CHANNEL),
        user_id: MAYA,
    };
    assert_eq!(
        actions(&app, vcx, MAYA),
        vec![DirectAction::MarkRead, ban, DirectAction::Delete]
    );
    assert_eq!(
        actions(&app, vcx, PRIYA).first(),
        Some(&DirectAction::MarkUnread),
        "a read chat can be marked unread"
    );
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.set_direct_member_banned(ChatId(CHANNEL), MAYA, true);
        }
    });
    assert_eq!(
        actions(&app, vcx, MAYA).get(1),
        Some(&DirectAction::Unban {
            channel: ChatId(CHANNEL),
            user_id: MAYA
        })
    );
}

#[gpui_kit::test]
fn the_channel_hides_its_direct_messages_button_once_they_are_off(cx: &mut TestAppContext) {
    let (app, mut vcx) = open_demo(cx, Viewer::Subscriber);
    let vcx = &mut vcx;
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.select_listed_chat(ChatId(CHANNEL), window, cx)
        });
    });
    frames(vcx);
    assert!(vcx.update(|window, _| window.try_find("channel-direct-messages").is_some()));
    app.update(vcx, |app, _| {
        app.demo_apply_jsons([
            r#"{"@type":"updateSupergroupFullInfo","supergroup_id":81,"supergroup_full_info":{"description":"","member_count":48210,"direct_messages_chat_id":0}}"#
                .to_string(),
        ]);
    });
    frames(vcx);
    assert!(
        vcx.update(|window, _| window.try_find("channel-direct-messages").is_none()),
        "the button follows the channel's direct messages chat"
    );
}
