//! The forward panel follows tdesktop's share box: Enter toggles the first
//! match and Send exists only while a chat is ticked (needs the
//! `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::composer::ForwardDraft;
use quill::ids::{ChatId, MessageId};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, Entity<QuillShell>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-privacy")),
            )
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some((view, shell.clone()));
        Root::new(shell, window, cx)
    });
    let (app, shell) = slot.borrow().clone().unwrap();
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    (app, shell, vcx)
}

#[gpui_kit::test]
fn enter_toggles_the_first_match_and_does_not_send(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        // A message the history holds: a draft of a missing one is empty.
        let (first, message) = {
            let session = app.demo_session.as_ref().expect("demo session");
            let (chat, history) = session
                .histories
                .iter()
                .find(|(_, history)| !history.messages.is_empty())
                .expect("a chat with messages");
            let id = history.messages.keys().next().copied().expect("a message");
            (ChatId(*chat), MessageId(id))
        };
        app.share.forward =
            ForwardDraft::from_message(first, quill::ids::TopicId::None, message, false).map_or(
                crate::ui::forward_stage::ForwardStage::None,
                crate::ui::forward_stage::ForwardStage::Selecting,
            );
        app.open_forward_picker(window, cx);
        assert_eq!(app.share.tick_count(), 0);
        app.activate_first_forward_destination(cx);
        assert_eq!(app.share.tick_count(), 1);
        // A second Enter unticks it again (selectActive toggles); it never sends.
        app.activate_first_forward_destination(cx);
        assert_eq!(app.share.tick_count(), 0);
        assert!(app.share.forward.picker_open());
        // Ctrl+Enter with nothing ticked does nothing.
        app.submit_share_from_search(window, cx);
        assert!(app.share.forward.picker_open());
    });
}

/// What was typed in the share box goes with the box, on whichever way out,
/// and the next box starts empty with nothing ticked.
#[gpui_kit::test]
fn the_share_boxs_text_and_ticks_go_with_the_box(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        let (chat, message) = {
            let session = app.demo_session.as_ref().expect("demo session");
            let (chat, history) = session
                .histories
                .iter()
                .find(|(_, history)| !history.messages.is_empty())
                .expect("a chat with messages");
            let id = history.messages.keys().next().copied().expect("a message");
            (ChatId(*chat), MessageId(id))
        };
        app.share.forward =
            ForwardDraft::from_message(chat, quill::ids::TopicId::None, message, false).map_or(
                crate::ui::forward_stage::ForwardStage::None,
                crate::ui::forward_stage::ForwardStage::Selecting,
            );
        app.open_forward_picker(window, cx);
        app.share.toggle_tick(chat);
        app.share
            .comment_input
            .update(cx, |input, cx| input.set_value("look", window, cx));
        app.share
            .search_input
            .update(cx, |input, cx| input.set_value("ann", window, cx));
        app.close_forward_picker(window, cx);
    });
    vcx.run_until_parked();
    vcx.simulate_resize(size(px(1101.), px(800.)));
    vcx.run_until_parked();
    app.update_in(vcx, |app, window, cx| {
        assert!(app.share.comment_input.read(cx).value().is_empty());
        assert!(app.share.search_input.read(cx).value().is_empty());
        app.open_forward_picker(window, cx);
        assert_eq!(app.share.tick_count(), 0, "a new box starts with no ticks");
    });
}

/// The Shift+click anchor is the selection's: a new selection starts with
/// none, not with the row an earlier one ended on.
#[gpui_kit::test]
fn a_new_selection_starts_without_the_old_ones_anchor(cx: &mut TestAppContext) {
    let (app, _shell, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    app.update_in(vcx, |app, window, cx| {
        let (chat, ids) = {
            let session = app.demo_session.as_ref().expect("demo session");
            let (chat, history) = session
                .histories
                .iter()
                .find(|(_, history)| history.messages.len() >= 2)
                .expect("a chat with two messages");
            let ids: Vec<MessageId> = history.messages.keys().map(|id| MessageId(*id)).collect();
            (ChatId(*chat), ids)
        };
        let (first, second) = (ids[0], ids[1]);
        app.toggle_forward_select(chat, first, false, cx);
        app.set_selection_anchor(first);
        assert_eq!(app.selection_anchor(), Some(first));
        app.end_selection(window, cx);
        assert_eq!(app.selection_anchor(), None, "the selection took it along");
        app.toggle_forward_select(chat, second, false, cx);
        assert_eq!(app.selection_anchor(), None, "a new selection has none yet");
    });
}
