//! A chat window runs the per-window half of the frame upkeep with its own
//! state in place (`app_render::window_upkeep`), and keeps its motion,
//! dialogs, Escape, forward answers and forum peek to itself.
#![cfg(feature = "demo-capture")]

use super::tests::{Harness, OPEN, OTHER, OTHER_LAST, new_app, new_demo_app};
use crate::ui::app::QuillApp;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext, Context, TestAppContext, Window};
use quill::ids::ChatId;

fn in_window<R>(
    h: &Harness,
    handle: AnyWindowHandle,
    chat: Option<ChatId>,
    cx: &mut TestAppContext,
    f: impl FnOnce(&mut QuillApp, &mut Window, &mut Context<QuillApp>) -> R,
) -> R {
    let app = h.app.clone();
    let result = cx
        .update_window(handle, |_, window, cx| {
            app.update(cx, |app, cx| {
                app.in_window_scope(chat, cx, |app, cx| f(app, window, cx))
            })
        })
        .unwrap()
        .expect("the window's state");
    cx.run_until_parked();
    result
}

fn draw(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .unwrap();
    cx.run_until_parked();
}

/// Read `f` of `chat`'s window state (`None`: the main window's).
fn read<R>(
    h: &Harness,
    chat: Option<ChatId>,
    cx: &mut TestAppContext,
    f: impl FnOnce(&mut QuillApp, &mut Context<QuillApp>) -> R,
) -> R {
    h.app
        .update(cx, |app, cx| app.in_window_scope(chat, cx, f))
        .expect("the window's state")
}

#[gpui_kit::test]
fn a_link_clicked_in_a_chat_window_asks_there(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    in_window(&h, chat_window, Some(OTHER), cx, |app, _, cx| {
        let link = quill::text::LinkTarget::Url {
            url: "https://example.com/offer".into(),
            label: Some("a prize".into()),
        };
        app.queue_link(link, (OTHER.0, OTHER_LAST.0 as u64), cx);
    });
    draw(chat_window, cx);
    let (window_confirm, window_pending) = read(&h, Some(OTHER), cx, |app, _| {
        (
            app.message_ui.open_link_confirm.is_some(),
            app.message_ui.pending_link.is_some(),
        )
    });
    assert!(!window_pending, "the window's frame took the click");
    assert!(window_confirm, "\"Open this link?\" asks in the window");
    assert!(!read(&h, None, cx, |app, _| app
        .message_ui
        .open_link_confirm
        .is_some()));
}

#[gpui_kit::test]
fn a_topic_switch_in_a_forum_window_leaves_the_old_topics_text(cx: &mut TestAppContext) {
    use crate::ui::demos::composer::{FORUM_CHAT, RELEASES_TOPIC};
    let h = new_demo_app(cx, "ready-composer-bars");
    let chat_window = h.open_in_new_window(FORUM_CHAT, cx);
    let composer = h.window_composer(FORUM_CHAT, cx);
    in_window(&h, chat_window, Some(FORUM_CHAT), cx, |app, window, cx| {
        composer.update(cx, |field, cx| {
            field.set_value("about the chat", window, cx)
        });
        app.chat_ui
            .set_reply(Some(quill::composer::ComposerReplyTo::new(
                FORUM_CHAT,
                quill::ids::MessageId(20),
            )));
        app.select_topic_ui(RELEASES_TOPIC, cx);
    });
    draw(chat_window, cx);
    let (text, topic) = read(&h, Some(FORUM_CHAT), cx, |app, cx| {
        let topic = app.session().and_then(|session| session.open_topic);
        (app.composer.read(cx).value().to_string(), topic)
    });
    assert_eq!(topic, Some(RELEASES_TOPIC));
    assert_eq!(text, "", "the topic's own (empty) draft is in the field");
}

#[gpui_kit::test]
fn an_ai_answer_for_a_chat_window_lands_in_its_composer(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.app.update(cx, |app, _| {
        if let Some(session) = app.session_mut() {
            session.messages.ai_composer_text = Some((OTHER, "fixed text".into()));
        }
    });
    // The main window's frame leaves another window's answer alone.
    draw(h.main, cx);
    assert_eq!(h.main_composer_text(cx), "");
    draw(chat_window, cx);
    let composer = h.window_composer(OTHER, cx);
    assert_eq!(
        composer.read_with(cx, |field, _| field.value().to_string()),
        "fixed text"
    );
}

#[gpui_kit::test]
fn each_window_lifts_its_own_corner_buttons(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    let now = std::time::Instant::now();
    read(&h, Some(OTHER), cx, |app, _| {
        app.frame.surfaces.corner_lift.borrow_mut().go(
            1.,
            crate::ui::surface_reveal::CORNER,
            false,
            now,
        );
    });
    let main = read(&h, None, cx, |app, _| {
        app.frame.surfaces.corner_lift.borrow().target()
    });
    let window = read(&h, Some(OTHER), cx, |app, _| {
        app.frame.surfaces.corner_lift.borrow().target()
    });
    // One shared glide flipped between the two targets at every draw of
    // either window and kept both on a 60 fps tick.
    assert_eq!((main, window), (0., 1.));
}

#[gpui_kit::test]
fn a_main_dialog_stays_open_while_the_pointer_is_in_a_chat_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    read(&h, None, cx, |app, cx| {
        app.message_ui.open_link_confirm = Some(crate::ui::entity_links::OpenLinkConfirm {
            url: "https://example.com".into(),
            shown: "https://example.com".into(),
            suspicious: Vec::new(),
            from_poll_option: false,
        });
        app.note_window_input(None);
        cx.notify();
    });
    draw(h.main, cx);
    assert!(h.shell.read_with(cx, |shell, _| shell.dialog_open()));
    // The pointer moves into the chat window, then the main window draws.
    h.app.update(cx, |app, cx| {
        app.note_window_input(Some(OTHER));
        cx.notify();
    });
    draw(h.main, cx);
    assert!(
        h.shell.read_with(cx, |shell, _| shell.dialog_open()),
        "the main window's dialog read the main window's flag"
    );
}
