//! UI tests for the message menu's origin rows, over the
//! `ready-origin-menus` demo (gpui-kit's test support needs the
//! `demo-capture` feature).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::menu_states::MessageMenuState;
use crate::ui::message_menu_ui::origin_rows::{FORWARD_CHAT, GIFT_CHAT, REPLIES_CHAT, TIMES_CHAT};
use crate::ui::profile_cover::{ShowOrPremium, ShowOrPremiumBox};
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, point, px, size};
use quill::ids::{ChatId, MessageId};
use quill::lang::{keys, tr};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-origin-menus")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.update(|_, cx| cx.set_reduce_motion(true));
    frames(&mut vcx);
    (app, vcx)
}

fn frames(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

fn has(cx: &mut VisualTestContext, id: &'static str) -> bool {
    cx.update(|window, _| window.try_find(id).is_some())
}

fn click(cx: &mut VisualTestContext, id: &'static str) {
    cx.update(|window, cx| window.click(id, cx));
    frames(cx);
}

fn open_menu_on(
    app: &Entity<QuillApp>,
    cx: &mut VisualTestContext,
    chat: ChatId,
    message: MessageId,
) {
    app.update(cx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.open_chat(chat);
        }
        cx.notify();
    });
    frames(cx);
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_message_menu(
                MessageMenuState {
                    chat_id: chat,
                    message_id: message,
                    position: point(px(400.), px(300.)),
                },
                window,
                cx,
            );
        });
    });
    frames(cx);
}

fn replies_block_open(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> bool {
    app.read_with(cx, |app, _| {
        app.session()
            .is_some_and(|s| s.users_state.replies_block.is_some())
    })
}

#[gpui_kit::test]
fn a_forward_shows_when_it_was_first_sent(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_menu_on(&app, vcx, FORWARD_CHAT, MessageId(40));
    assert!(has(vcx, "menu-original-date"));
}

#[gpui_kit::test]
fn an_edited_message_shows_only_the_edited_date(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_menu_on(&app, vcx, TIMES_CHAT, MessageId(71));
    assert!(has(vcx, "menu-sent"));
    assert!(!has(vcx, "menu-edited"));
    open_menu_on(&app, vcx, TIMES_CHAT, MessageId(72));
    assert!(has(vcx, "menu-edited"));
    assert!(!has(vcx, "menu-sent"));
}

#[gpui_kit::test]
fn an_edited_forward_shows_only_its_original_date(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update(vcx, |app, cx| {
        if let Some(message) = app
            .demo_session
            .as_mut()
            .and_then(|session| session.histories.get_mut(&FORWARD_CHAT.0))
            .and_then(|history| history.messages.get_mut(&40))
        {
            message.extras.edit_date = message.date + 600;
        }
        cx.notify();
    });
    open_menu_on(&app, vcx, FORWARD_CHAT, MessageId(40));
    assert!(has(vcx, "menu-original-date"));
    assert!(!has(vcx, "menu-edited"));
    assert!(!has(vcx, "menu-sent"));
}

#[gpui_kit::test]
fn only_a_gift_you_sent_offers_another(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_menu_on(&app, vcx, GIFT_CHAT, MessageId(50));
    assert!(has(vcx, "menu-send-another-gift"));
    assert!(!has(vcx, "menu-original-date"));
    click(vcx, "menu-send-another-gift");
    assert!(app.read_with(vcx, |app, _| app.message_ui.menu.is_none()));

    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut()
            && let Some(message) = session
                .histories
                .get_mut(&GIFT_CHAT.0)
                .and_then(|h| h.messages.get_mut(&50))
        {
            message.is_outgoing = false;
        }
    });
    open_menu_on(&app, vcx, GIFT_CHAT, MessageId(50));
    assert!(!has(vcx, "menu-send-another-gift"));
}

#[gpui_kit::test]
fn blocking_a_reply_sender_asks_first(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_menu_on(&app, vcx, REPLIES_CHAT, MessageId(60));
    assert!(has(vcx, "menu-block-sender"));
    assert!(!has(vcx, "menu-report"), "tdesktop hides Report here");
    assert!(!replies_block_open(&app, vcx));

    click(vcx, "menu-block-sender");
    assert!(replies_block_open(&app, vcx));
    assert!(has(vcx, "replies-block-report"));
    assert!(has(vcx, "replies-block-delete"));
    assert!(has(vcx, "replies-block-delete-all"));

    click(vcx, "replies-block-delete-all");
    let delete_all = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|s| s.users_state.replies_block.as_ref().map(|b| b.delete_all))
    });
    assert_eq!(delete_all, Some(false));

    click(vcx, "replies-block-cancel");
    assert!(!replies_block_open(&app, vcx));

    open_menu_on(&app, vcx, REPLIES_CHAT, MessageId(60));
    click(vcx, "menu-block-sender");
    click(vcx, "replies-block-submit");
    assert!(!replies_block_open(&app, vcx));
    assert!(!has(vcx, "replies-block-submit"));
}

#[gpui_kit::test]
fn other_chats_offer_no_block(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_menu_on(&app, vcx, FORWARD_CHAT, MessageId(40));
    assert!(!has(vcx, "menu-block-sender"));
}

#[gpui_kit::test]
fn a_private_chat_lists_the_date_above_the_read_time(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_menu_on(&app, vcx, TIMES_CHAT, MessageId(71));
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            let mut audience = quill::state::MessageAudience::new(TIMES_CHAT, MessageId(71));
            audience.read_date = quill::state::Audience::Ready(
                quill::telegram::envelope::MessageReadDate::Read(1_773_504_420),
            );
            session.messages.message_audience = Some(audience);
        }
        cx.notify();
    });
    frames(vcx);
    let (date, read) = vcx.update(|window, _| {
        (
            window.find("menu-sent").bounds().origin.y,
            window.find("menu-read-date").bounds().origin.y,
        )
    });
    assert!(date < read, "the sent line sits above the read time");
    assert!(!has(vcx, "menu-read-show"));
}

fn hide_read_time(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    app.update(cx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            let mut audience = quill::state::MessageAudience::new(TIMES_CHAT, MessageId(71));
            audience.read_date = quill::state::Audience::Ready(
                quill::telegram::envelope::MessageReadDate::MyPrivacyRestricted,
            );
            session.messages.message_audience = Some(audience);
            session.settings.read_date_show = Some(false);
        }
        cx.notify();
    });
    frames(cx);
}

fn read_time_box(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Option<ShowOrPremiumBox> {
    app.read_with(cx, |app, _| app.profile_cover.show_or_premium)
}

#[gpui_kit::test]
fn your_hidden_read_time_offers_to_show_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_menu_on(&app, vcx, TIMES_CHAT, MessageId(71));
    hide_read_time(&app, vcx);
    click(vcx, "menu-read-show");
    assert!(app.read_with(vcx, |app, _| app.message_ui.menu.is_none()));
    let shown = read_time_box(&app, vcx);
    assert_eq!(shown.map(|open| open.kind), Some(ShowOrPremium::ReadTime));
    click(vcx, "show-or-premium-show");
    assert_eq!(read_time_box(&app, vcx), None);
    let shown = app.read_with(vcx, |app, _| {
        app.session()
            .and_then(|session| session.settings.read_date_show)
    });
    assert_eq!(
        shown,
        Some(true),
        "Show My Read Time shows read dates again"
    );
    let note = app.read_with(vcx, |app, _| app.connection.note().text().to_string());
    assert_eq!(note, tr(keys::READTIME_SHOWN_TOAST));
}

#[gpui_kit::test]
fn the_read_time_box_subscribes_with_its_ref(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_menu_on(&app, vcx, TIMES_CHAT, MessageId(71));
    hide_read_time(&app, vcx);
    click(vcx, "menu-read-show");
    click(vcx, "show-or-premium-premium");
    assert_eq!(read_time_box(&app, vcx), None);
    let premium = app.read_with(vcx, |app, _| {
        app.session().map(|session| {
            (
                session.payments.hub.premium_open,
                session.payments.hub.premium_ref.clone(),
            )
        })
    });
    assert_eq!(premium, Some((true, "revtime_hidden".to_string())));
}

#[gpui_kit::test]
fn their_hidden_read_time_has_nothing_to_show(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_menu_on(&app, vcx, TIMES_CHAT, MessageId(71));
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            let mut audience = quill::state::MessageAudience::new(TIMES_CHAT, MessageId(71));
            audience.read_date = quill::state::Audience::Ready(
                quill::telegram::envelope::MessageReadDate::UserPrivacyRestricted,
            );
            session.messages.message_audience = Some(audience);
        }
        cx.notify();
    });
    frames(vcx);
    assert!(has(vcx, "menu-read-date"));
    assert!(!has(vcx, "menu-read-show"));
}
