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
        app.share.pending_forward = ForwardDraft::from_message(first, message, false);
        app.open_forward_picker(window, cx);
        assert!(app.share.selection.is_empty());
        app.activate_first_forward_destination(cx);
        assert_eq!(app.share.selection.len(), 1);
        // A second Enter unticks it again (selectActive toggles); it never sends.
        app.activate_first_forward_destination(cx);
        assert!(app.share.selection.is_empty());
        assert!(app.share.forward_picker_open);
        // Ctrl+Enter with nothing ticked does nothing.
        app.submit_share_from_search(cx);
        assert!(app.share.forward_picker_open);
    });
}
