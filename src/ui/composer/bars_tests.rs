//! UI tests for the reply and edit bars and the field placeholder, over the
//! `ready-composer-bars` demo (gpui-kit's test support needs the
//! `demo-capture` feature).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px, size};
use quill::composer::{BarAction, BarKind, ComposerReplyTo};
use quill::ids::{ChatId, MessageId};

const GROUP: ChatId = ChatId(-2002);
const PAID_USER: ChatId = ChatId(51);
const CHANNEL: ChatId = ChatId(-1002000000001);
const SUPERGROUP: ChatId = ChatId(-1002000000002);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-composer-bars")),
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

fn open_chat(app: &Entity<QuillApp>, cx: &mut VisualTestContext, chat: ChatId) {
    app.update(cx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.open_chat(chat);
        }
        cx.notify();
    });
    frames(cx);
}

fn placeholder(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> String {
    app.read_with(cx, |app, cx| {
        app.composer
            .read(cx)
            .presentation()
            .placeholder()
            .to_string()
    })
}

fn begin_edit(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            let edit = app
                .session()
                .and_then(|session| session.last_editable_message(GROUP));
            assert!(edit.is_some(), "the group has an own message to edit");
            if let Some(edit) = edit {
                app.begin_edit(edit, window, cx);
            }
        });
    });
    frames(cx);
}

fn editing(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> bool {
    app.read_with(cx, |app, _| app.composer_ui.pending_edit.is_some())
}

fn confirm_open(app: &Entity<QuillApp>, cx: &mut VisualTestContext) -> bool {
    app.read_with(cx, |app, _| app.composer_ui.cancel_edit_confirm)
}

fn set_field(app: &Entity<QuillApp>, cx: &mut VisualTestContext, text: &str) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.composer.update(cx, |input, cx| {
                input.set_value(text.to_string(), window, cx)
            });
        });
    });
    frames(cx);
}

fn escape(_app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.press("escape", cx));
    frames(cx);
}

fn click(cx: &mut VisualTestContext, id: &'static str) {
    cx.update(|window, cx| window.click(id, cx));
    frames(cx);
}

#[gpui_kit::test]
fn placeholder_follows_the_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    assert_eq!(placeholder(&app, vcx), "Write a message…");
    open_chat(&app, vcx, PAID_USER);
    assert_eq!(placeholder(&app, vcx), "Message for 25 Stars");
    open_chat(&app, vcx, CHANNEL);
    assert_eq!(placeholder(&app, vcx), "Broadcast a message…");
    app.update(vcx, |app, _| app.toggle_composer_silent());
    frames(vcx);
    assert_eq!(placeholder(&app, vcx), "Silent broadcast…");
    open_chat(&app, vcx, GROUP);
    assert_eq!(placeholder(&app, vcx), "Write a message…");
}

#[gpui_kit::test]
fn editing_changes_the_placeholder_and_cancelling_restores_it(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    assert_eq!(placeholder(&app, vcx), "New message…");
    escape(&app, vcx);
    assert!(!editing(&app, vcx));
    assert_eq!(placeholder(&app, vcx), "Write a message…");
}

#[gpui_kit::test]
fn escape_cancels_an_unchanged_edit_at_once(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    escape(&app, vcx);
    assert!(!confirm_open(&app, vcx));
    assert!(!editing(&app, vcx));
}

#[gpui_kit::test]
fn whitespace_only_is_not_a_change(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    let text = app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string());
    set_field(&app, vcx, &format!("{text}  \n"));
    escape(&app, vcx);
    assert!(!editing(&app, vcx));
}

#[gpui_kit::test]
fn escape_asks_before_dropping_changed_text(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    set_field(&app, vcx, "something else entirely");
    escape(&app, vcx);
    assert!(confirm_open(&app, vcx));
    assert!(editing(&app, vcx), "the edit stays until the user answers");
}

#[gpui_kit::test]
fn answering_no_keeps_the_edit_and_the_text(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    set_field(&app, vcx, "something else entirely");
    escape(&app, vcx);
    assert!(confirm_open(&app, vcx));
    click(vcx, "cancel-edit-no");
    assert!(!confirm_open(&app, vcx));
    assert!(editing(&app, vcx));
    let text = app.read_with(vcx, |app, cx| app.composer.read(cx).value().to_string());
    assert_eq!(text, "something else entirely");
}

#[gpui_kit::test]
fn answering_yes_drops_the_edit(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    set_field(&app, vcx, "something else entirely");
    escape(&app, vcx);
    click(vcx, "cancel-edit-yes");
    assert!(!editing(&app, vcx));
    assert!(!confirm_open(&app, vcx));
}

#[gpui_kit::test]
fn a_second_escape_does_not_reopen_or_cancel(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    set_field(&app, vcx, "something else entirely");
    escape(&app, vcx);
    assert!(confirm_open(&app, vcx));
    escape(&app, vcx);
    assert!(
        editing(&app, vcx),
        "Escape on the box does not drop the edit"
    );
    escape(&app, vcx);
    assert!(editing(&app, vcx));
}

#[gpui_kit::test]
fn the_x_button_cancels_without_asking(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    set_field(&app, vcx, "something else entirely");
    click(vcx, "cancel-edit");
    assert!(!editing(&app, vcx));
    assert!(!confirm_open(&app, vcx));
}

fn reply_to_ada(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    app.update(cx, |app, cx| {
        app.composer_ui
            .set_reply(Some(ComposerReplyTo::new(GROUP, MessageId(10))));
        cx.notify();
    });
    frames(cx);
}

fn jump_ready(app: &Entity<QuillApp>, cx: &mut VisualTestContext, id: MessageId) -> bool {
    app.read_with(cx, |app, _| {
        app.session().is_some_and(|session| {
            matches!(
                session.search.chat_search.jump,
                quill::state::ChatSearchJump::Ready { message_id } if message_id == id
            )
        })
    })
}

fn clear_jump(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    app.update(cx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.search.chat_search.jump = quill::state::ChatSearchJump::None;
        }
    });
}

#[gpui_kit::test]
fn clicking_the_reply_bar_opens_the_quote_picker(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    reply_to_ada(&app, vcx);
    let kind = app.read_with(vcx, |app, _| {
        app.composer_ui
            .reply()
            .map(|reply| app.reply_bar_kind(reply))
    });
    assert!(matches!(
        kind,
        Some(BarKind::Reply {
            has_text: true,
            same_chat: true,
            ..
        })
    ));
    vcx.update(|window, cx| window.click(("composer-reply-quote", 1usize), cx));
    frames(vcx);
    assert!(app.read_with(vcx, |app, _| app.share.reply_quote_open));
}

#[gpui_kit::test]
fn right_clicking_the_reply_bar_jumps_to_the_replied_message(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    reply_to_ada(&app, vcx);
    clear_jump(&app, vcx);
    vcx.update(|window, cx| window.right_click(("composer-reply-quote", 1usize), cx));
    frames(vcx);
    assert!(jump_ready(&app, vcx, MessageId(10)));
    assert!(
        !app.read_with(vcx, |app, _| app.share.reply_quote_open),
        "a right click does not open the picker"
    );
}

#[gpui_kit::test]
fn clicking_the_edit_bar_jumps_to_the_edited_message(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    begin_edit(&app, vcx);
    let edited = app.read_with(vcx, |app, _| {
        app.composer_ui
            .pending_edit
            .as_ref()
            .map(|edit| edit.message_id)
    });
    assert_eq!(edited, Some(MessageId(11)));
    clear_jump(&app, vcx);
    vcx.update(|window, cx| window.click(("composer-edit-header", 1usize), cx));
    frames(vcx);
    assert!(jump_ready(&app, vcx, MessageId(11)));
    assert!(editing(&app, vcx), "jumping leaves the edit open");
}

#[gpui_kit::test]
fn the_bar_handler_for_a_media_reply_offers_another_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    reply_to_ada(&app, vcx);
    let clicks = app.update(vcx, |app, cx| app.bar_clicks(BarKind::Edit, None, cx));
    vcx.update(|window, cx| (clicks.run)(BarAction::ReplyElsewhere, window, cx));
    assert!(app.read_with(vcx, |app, _| app.share.reply_elsewhere_open));
}

fn supergroup_status(app: &Entity<QuillApp>, cx: &mut VisualTestContext, status: &str) {
    app.update(cx, |app, _| {
        use std::sync::atomic::Ordering;
        let (sink, seq) = (app.demo_ui.sink.clone(), &app.demo_ui.seq);
        if let Some(session) = app.demo_session.as_mut() {
            seq.store(session.last_seq, Ordering::SeqCst);
            let json = format!(
                r#"{{"@type":"updateSupergroup","supergroup":{{"@type":"supergroup","id":2000000002,"status":{status}}}}}"#
            );
            let dyn_sink: std::sync::Arc<dyn quill::diagnostics::DiagnosticSink> = sink;
            if let Some(owned) = quill::telegram::client::copy_and_parse(&json, seq, &dyn_sink) {
                session.apply(owned);
            }
        }
    });
    frames(cx);
}

#[gpui_kit::test]
fn an_anonymous_admin_sends_anonymously_until_demoted_or_revealed(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, SUPERGROUP);
    assert_eq!(placeholder(&app, vcx), "Send anonymously…");
    // The owner turns "Remain anonymous" off.
    supergroup_status(
        &app,
        vcx,
        r#"{"@type":"chatMemberStatusAdministrator","can_be_edited":false,"rights":{"@type":"chatAdministratorRights","is_anonymous":false}}"#,
    );
    assert_eq!(placeholder(&app, vcx), "Write a message…");
    supergroup_status(
        &app,
        vcx,
        r#"{"@type":"chatMemberStatusAdministrator","can_be_edited":false,"rights":{"@type":"chatAdministratorRights","is_anonymous":true}}"#,
    );
    assert_eq!(placeholder(&app, vcx), "Send anonymously…");
    // The owner posts anonymously too; a demotion clears it at once.
    supergroup_status(
        &app,
        vcx,
        r#"{"@type":"chatMemberStatusCreator","is_anonymous":true,"is_member":true}"#,
    );
    assert_eq!(placeholder(&app, vcx), "Send anonymously…");
    supergroup_status(&app, vcx, r#"{"@type":"chatMemberStatusMember"}"#);
    assert_eq!(placeholder(&app, vcx), "Write a message…");
}

#[gpui_kit::test]
fn an_inline_bots_hint_follows_the_typed_name(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, PAID_USER);
    let hint = |app: &Entity<QuillApp>, vcx: &mut VisualTestContext| {
        app.read_with(vcx, |app, cx| app.inline_bot_hint_text(cx))
    };
    assert_eq!(hint(&app, vcx), None);
    app.update(vcx, |app, _| {
        if let Some(session) = app.demo_session.as_mut() {
            session.bots.inline_bot_resolve = Some(quill::state::InlineBotResolve::Resolved {
                username: "gifbot".into(),
                user_id: 61,
                is_inline: Some(true),
            });
        }
    });
    set_field(&app, vcx, "@gifbot ");
    assert_eq!(hint(&app, vcx).as_deref(), Some("Search GIFs"));
    set_field(&app, vcx, "@gifbot cats");
    assert_eq!(hint(&app, vcx), None);
    set_field(&app, vcx, "@gifbot");
    assert_eq!(hint(&app, vcx).as_deref(), Some("Search GIFs"));
    set_field(&app, vcx, "");
    assert_eq!(hint(&app, vcx), None);
}

const FORUM: ChatId = ChatId(-1002000000003);
const KEYBOARD_BOT: ChatId = ChatId(71);
const TOPIC_BOT: ChatId = ChatId(72);

fn select_topic(app: &Entity<QuillApp>, cx: &mut VisualTestContext, chat: ChatId, topic: i32) {
    app.update(cx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session.select_topic(chat, topic);
        }
        cx.notify();
    });
    frames(cx);
}

fn reply_to(app: &Entity<QuillApp>, cx: &mut VisualTestContext, chat: ChatId, message: i64) {
    app.update(cx, |app, cx| {
        app.composer_ui
            .set_reply(Some(ComposerReplyTo::new(chat, MessageId(message))));
        cx.notify();
    });
    frames(cx);
}

#[gpui_kit::test]
fn a_forum_field_names_the_topic_it_sends_to(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    // Without a topic open a message lands in General.
    open_chat(&app, vcx, FORUM);
    assert_eq!(placeholder(&app, vcx), "Message in General");
    // A reply is sent to the topic its message is in, which no topic history
    // has to be loaded for.
    reply_to(&app, vcx, FORUM, 20);
    assert_eq!(placeholder(&app, vcx), "Reply in Releases");
    // Cancelling the reply brings General back.
    app.update(vcx, |app, cx| {
        app.composer_ui.set_reply(None);
        cx.notify();
    });
    frames(vcx);
    assert_eq!(placeholder(&app, vcx), "Message in General");
    // A topic that was renamed is read again.
    reply_to(&app, vcx, FORUM, 20);
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            session
                .threads
                .forum_topics
                .update_topic(FORUM.0, 5, |topic| topic.name = "Changelog".into());
        }
        cx.notify();
    });
    frames(vcx);
    assert_eq!(placeholder(&app, vcx), "Reply in Changelog");
    // Inside an open topic the field is plain, as in tdesktop's topic view.
    select_topic(&app, vcx, FORUM, 5);
    assert_eq!(placeholder(&app, vcx), "Write a message…");
    open_chat(&app, vcx, GROUP);
    assert_eq!(placeholder(&app, vcx), "Write a message…");
}

#[gpui_kit::test]
fn a_bot_keyboards_placeholder_shows_while_the_keyboard_does(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, KEYBOARD_BOT);
    assert_eq!(placeholder(&app, vcx), "Choose a size");
    // Hiding the keyboard takes its text with it.
    click(vcx, "composer-keyboard-toggle");
    assert_eq!(placeholder(&app, vcx), "Write a message…");
    click(vcx, "composer-keyboard-toggle");
    assert_eq!(placeholder(&app, vcx), "Choose a size");
    open_chat(&app, vcx, GROUP);
    assert_eq!(placeholder(&app, vcx), "Write a message…");
}

#[gpui_kit::test]
fn a_bot_that_makes_its_own_topics_is_off_thread_until_one_opens(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    open_chat(&app, vcx, TOPIC_BOT);
    assert_eq!(placeholder(&app, vcx), "Off-thread message");
    select_topic(&app, vcx, TOPIC_BOT, 2);
    assert_eq!(placeholder(&app, vcx), "Write a message…");
    open_chat(&app, vcx, KEYBOARD_BOT);
    assert_eq!(placeholder(&app, vcx), "Choose a size");
}
