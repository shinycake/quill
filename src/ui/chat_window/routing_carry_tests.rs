//! What an open hands over reaches the window that shows the chat
//! (`ChatOpenCarry`): the forward bar, dropped files, composer text and a
//! link's `/start` prefill.

use super::super::tests::{Harness, OPEN, OTHER, OTHER_LAST, new_app};
use super::ChatOpen;
use crate::ui::app::QuillApp;
use gpui_kit::{AnyWindowHandle, AppContext, Context, TestAppContext, Window};
use quill::composer::ForwardDraft;
use quill::ids::ChatId;

fn in_window(
    h: &Harness,
    handle: AnyWindowHandle,
    chat: Option<ChatId>,
    cx: &mut TestAppContext,
    f: impl FnOnce(&mut QuillApp, &mut Window, &mut Context<QuillApp>),
) {
    let app = h.app.clone();
    cx.update_window(handle, |_, window, cx| {
        app.update(cx, |app, cx| {
            app.in_window_scope(chat, cx, |app, cx| f(app, window, cx));
        });
    })
    .unwrap();
    cx.run_until_parked();
    h.settle(cx);
}

/// `(forward bar chat, forward source)` of the window showing `chat`.
fn forward_bar(
    h: &Harness,
    chat: Option<ChatId>,
    cx: &mut TestAppContext,
) -> (Option<ChatId>, Option<ChatId>) {
    h.app
        .update(cx, |app, cx| {
            app.in_window_scope(chat, cx, |app, _| {
                (
                    app.share.forward_bar_dest,
                    app.share.pending_forward.as_ref().map(|d| d.from_chat_id),
                )
            })
        })
        .unwrap_or((None, None))
}

fn draft() -> ForwardDraft {
    ForwardDraft::from_message(OTHER, OTHER_LAST, false).expect("a forward")
}

#[gpui_kit::test]
fn a_forward_for_a_chat_with_its_own_window_waits_in_that_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    in_window(&h, h.main, None, cx, |app, window, cx| {
        app.select_chat_request(ChatOpen::new(OTHER).forwarding(draft()), window, cx);
    });
    assert_eq!(forward_bar(&h, Some(OTHER), cx), (Some(OTHER), Some(OTHER)));
    assert_eq!(forward_bar(&h, None, cx), (None, None), "main has no bar");
    assert_eq!(h.open_chat(cx), Some(OPEN), "the main window kept its chat");
}

#[gpui_kit::test]
fn continue_in_chat_moves_the_forward_and_comment_to_the_destination(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    // The Forward box, in the chat window, with the main window's chat
    // ticked and a comment typed.
    in_window(&h, chat_window, Some(OTHER), cx, |app, window, cx| {
        app.share.pending_forward = Some(draft());
        app.share.selection.toggle(OPEN);
        app.share
            .comment_input
            .update(cx, |input, cx| input.set_value("look", window, cx));
        app.share_continue_in_chat(window, cx);
    });
    assert_eq!(forward_bar(&h, None, cx), (Some(OPEN), Some(OTHER)));
    assert_eq!(h.open_chat(cx), Some(OPEN));
    assert_eq!(h.main_composer_text(cx), "look");
    assert_eq!(
        forward_bar(&h, Some(OTHER), cx),
        (None, None),
        "the box's window keeps no forward"
    );
    let window_text = h
        .window_composer(OTHER, cx)
        .read_with(cx, |input, _| input.value().to_string());
    assert_eq!(window_text, "", "the comment left the box's window");
}

#[gpui_kit::test]
fn composer_text_for_a_chat_with_its_own_window_lands_there(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    in_window(&h, h.main, None, cx, |app, window, cx| {
        app.select_chat_request(ChatOpen::new(OTHER).compose("@bot cats".into()), window, cx);
    });
    let window_text = h
        .window_composer(OTHER, cx)
        .read_with(cx, |input, _| input.value().to_string());
    assert_eq!(window_text, "@bot cats");
    assert_eq!(h.main_composer_text(cx), "");
}

#[gpui_kit::test]
fn a_start_link_for_a_chat_with_its_own_window_prefills_that_window(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    in_window(&h, h.main, None, cx, |app, window, cx| {
        let open = crate::ui::links_state::PendingOpen {
            chat_id: OTHER,
            action: quill::state::DeepLinkAction::OpenUsername {
                domain: "somebot".into(),
                start_param: Some("ref".into()),
                post: None,
                story_id: None,
            },
            from_agent: false,
        };
        app.open_deep_link_chat(open, window, cx);
    });
    let window_text = h
        .window_composer(OTHER, cx)
        .read_with(cx, |input, _| input.value().to_string());
    assert_eq!(window_text, "/start ref");
    assert_eq!(h.main_composer_text(cx), "");
    assert_eq!(h.open_chat(cx), Some(OPEN), "the main window kept its chat");
}

#[gpui_kit::test]
fn files_dropped_for_a_chat_with_its_own_window_attach_there(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let _chat_window = h.open_in_new_window(OTHER, cx);
    let file = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    in_window(&h, h.main, None, cx, |app, window, cx| {
        app.select_chat_request(
            ChatOpen::new(OTHER).with_files(vec![file], false),
            window,
            cx,
        );
    });
    let counts = h.app.update(cx, |app, cx| {
        let window = app.in_window_scope(Some(OTHER), cx, |app, _| app.pending_attachments().len());
        let main = app.in_window_scope(None, cx, |app, _| app.pending_attachments().len());
        (window, main)
    });
    assert_eq!(counts, (Some(1), Some(0)));
}
