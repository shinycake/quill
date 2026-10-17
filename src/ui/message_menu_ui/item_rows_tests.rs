//! UI tests for the message menu's item rows, over the `ready-item-menus`
//! demo (gpui-kit's test support needs the `demo-capture` feature).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::menu_states::MessageMenuState;
use crate::ui::message_menu_ui::item_rows::inject_menu_facts;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, point, px, size};
use quill::composer::ReplyItem;
use quill::ids::{ChatId, MessageId};

const GROUP: ChatId = ChatId(-2003);
const CHANNEL: ChatId = ChatId(-1002000000002);
const POLL: i64 = 20;
const CHECKLIST: i64 = 21;
const CONTACT: MessageId = MessageId(22);
const POST: MessageId = MessageId(30);

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
                Some(ScreenshotDemo::named("ready-item-menus")),
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
    frames(&mut vcx);
    (app, vcx)
}

fn frames(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

/// What TDLib would answer for the message the menu opens on.
fn facts(app: &Entity<QuillApp>, cx: &mut VisualTestContext, mode: &str) {
    app.update(cx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            inject_menu_facts(session, mode);
        }
        cx.notify();
    });
    frames(cx);
}

fn open_chat(app: &Entity<QuillApp>, cx: &mut VisualTestContext, chat: ChatId) {
    app.update(cx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.open_chat(chat);
        }
        cx.notify();
    });
    frames(cx);
}

fn menu_item(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> ReplyItem {
    app.read_with(cx, |app, _| app.message_ui.menu_item.clone())
}

fn menu_open(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> bool {
    app.read_with(cx, |app, _| app.message_ui.menu.is_some())
}

fn has(cx: &mut VisualTestContext, id: &'static str) -> bool {
    cx.update(|window, _| window.try_find(id).is_some())
}

fn click(cx: &mut VisualTestContext, id: &'static str) {
    cx.update(|window, cx| window.click(id, cx));
    frames(cx);
}

fn reply_item(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> Option<ReplyItem> {
    app.read_with(cx, |app, _| {
        app.composer_ui.reply().map(|reply| reply.item.clone())
    })
}

fn open_menu_on(
    app: &Entity<QuillApp>,
    cx: &mut VisualTestContext,
    chat: ChatId,
    message: MessageId,
) {
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

#[gpui_kit::test]
fn right_clicking_an_option_offers_its_rows_and_replies_to_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    facts(&app, vcx, "poll");
    vcx.update(|window, cx| window.right_click(("poll-option", (POLL * 64 + 1) as u64), cx));
    frames(vcx);
    assert!(menu_open(&app, vcx));
    assert_eq!(menu_item(&app, vcx), ReplyItem::PollOption("b".into()));
    assert!(has(vcx, "menu-reply-option"));
    assert!(has(vcx, "menu-copy-option"));
    assert!(
        has(vcx, "menu-delete-option"),
        "TDLib allowed deleting option b"
    );
    assert!(!has(vcx, "menu-reply-task"));
    click(vcx, "menu-reply-option");
    assert_eq!(
        reply_item(&app, vcx),
        Some(ReplyItem::PollOption("b".into()))
    );
    assert!(!menu_open(&app, vcx));
    // The reply goes out naming the option.
    let sent = app.read_with(vcx, |app, _| {
        app.composer_ui
            .reply()
            .and_then(|reply| reply.send_target(GROUP))
    });
    assert_eq!(
        sent.map(|reply| reply.item),
        Some(ReplyItem::PollOption("b".into()))
    );
}

#[gpui_kit::test]
fn an_option_that_cannot_be_deleted_has_no_delete_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    facts(&app, vcx, "poll");
    // The facts are for option b; option a was never asked about.
    vcx.update(|window, cx| window.right_click(("poll-option", (POLL * 64 + 0) as u64), cx));
    frames(vcx);
    assert_eq!(menu_item(&app, vcx), ReplyItem::PollOption("a".into()));
    assert!(has(vcx, "menu-copy-option"));
    assert!(!has(vcx, "menu-delete-option"));
}

#[gpui_kit::test]
fn copy_option_puts_the_option_text_on_the_clipboard(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    facts(&app, vcx, "poll");
    vcx.update(|window, cx| window.right_click(("poll-option", (POLL * 64 + 1) as u64), cx));
    frames(vcx);
    click(vcx, "menu-copy-option");
    let copied = vcx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("Beach"));
}

#[gpui_kit::test]
fn right_clicking_a_task_turns_reply_into_reply_to_task(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    facts(&app, vcx, "task");
    vcx.update(|window, cx| {
        window.right_click(("checklist-task", (CHECKLIST * 64 + 2) as u64), cx)
    });
    frames(vcx);
    assert_eq!(menu_item(&app, vcx), ReplyItem::Task(2));
    assert!(has(vcx, "menu-reply-task"));
    assert!(!has(vcx, "menu-reply"), "the plain Reply row is replaced");
    click(vcx, "menu-reply-task");
    assert_eq!(reply_item(&app, vcx), Some(ReplyItem::Task(2)));
}

#[gpui_kit::test]
fn the_reply_bar_shows_the_task_text(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    facts(&app, vcx, "task");
    vcx.update(|window, cx| {
        window.right_click(("checklist-task", (CHECKLIST * 64 + 3) as u64), cx)
    });
    frames(vcx);
    click(vcx, "menu-reply-task");
    let text = app.read_with(vcx, |app, _| {
        let reply = app.composer_ui.reply()?;
        let message = app
            .session()?
            .reply_target(reply.chat_id, reply.message_id)?
            .clone();
        quill::composer::reply_item::item_text(&message.content, &reply.item)
    });
    assert_eq!(text.as_deref(), Some("Snacks"));
}

#[gpui_kit::test]
fn a_menu_not_over_an_item_replies_to_the_message(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    facts(&app, vcx, "poll");
    open_menu_on(&app, vcx, GROUP, MessageId(POLL));
    assert_eq!(menu_item(&app, vcx), ReplyItem::Message);
    assert!(has(vcx, "menu-reply"));
    assert!(!has(vcx, "menu-reply-option"));
    assert!(!has(vcx, "menu-delete-option"));
}

#[gpui_kit::test]
fn a_contact_message_offers_copy_phone(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    facts(&app, vcx, "contact");
    open_menu_on(&app, vcx, GROUP, CONTACT);
    assert!(has(vcx, "menu-copy-phone"));
    click(vcx, "menu-copy-phone");
    let copied = vcx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied.as_deref(), Some("+15551234567"));
}

#[gpui_kit::test]
fn other_messages_have_no_copy_phone(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    facts(&app, vcx, "poll");
    open_menu_on(&app, vcx, GROUP, MessageId(POLL));
    assert!(!has(vcx, "menu-copy-phone"));
}

#[gpui_kit::test]
fn a_channel_post_names_its_author(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, CHANNEL);
    facts(&app, vcx, "author");
    open_menu_on(&app, vcx, CHANNEL, POST);
    assert!(has(vcx, "menu-author"));
    let label = vcx.update(|window, _| window.find("menu-author").label().map(str::to_string));
    assert_eq!(label.as_deref(), Some("Sent by Nora Vale"));
}

#[gpui_kit::test]
fn the_author_row_waits_for_the_answer_and_hides_on_failure(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, CHANNEL);
    facts(&app, vcx, "author");
    app.update(vcx, |app, _| {
        if let Some(audience) = app
            .demo_session
            .as_mut()
            .and_then(|session| session.messages.message_audience.as_mut())
        {
            audience.author = quill::state::Audience::Loading;
        }
    });
    open_menu_on(&app, vcx, CHANNEL, POST);
    assert!(has(vcx, "menu-author-loading"));
    assert!(!has(vcx, "menu-author"));
    app.update(vcx, |app, _| {
        if let Some(audience) = app
            .demo_session
            .as_mut()
            .and_then(|session| session.messages.message_audience.as_mut())
        {
            audience.author = quill::state::Audience::Failed;
        }
    });
    frames(vcx);
    assert!(!has(vcx, "menu-author-loading"));
    assert!(!has(vcx, "menu-author"));
}

#[gpui_kit::test]
fn a_group_post_without_the_flag_has_no_author_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    facts(&app, vcx, "poll");
    open_menu_on(&app, vcx, GROUP, MessageId(POLL));
    assert!(!has(vcx, "menu-author"));
    assert!(!has(vcx, "menu-author-loading"));
}
