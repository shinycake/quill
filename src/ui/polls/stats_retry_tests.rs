//! The poll stats Retry asks for the graph again and leaves the dialog
//! alone: the option the user picked stays picked.

use crate::ui::app::QuillApp;
use crate::ui::dialogs::{PollView, PollVotersDialog};
use crate::ui::screenshot_demo::ScreenshotDemo;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1000.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| crate::ui::shell::QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app = slot.borrow().clone().unwrap();
    (app, VisualTestContext::from_window(handle.into(), cx))
}

#[gpui_kit::test]
fn retrying_the_stats_keeps_the_dialog_and_its_option(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update(&mut vcx, |app, cx| {
        let (chat, message) = (ChatId(7), MessageId(108));
        app.message_ui.poll_voters_dialog = Some(PollVotersDialog {
            chat_id: chat,
            message_id: message,
            view: PollView::Stats { option: Some(1) },
        });
        app.retry_poll_stats(chat, message, cx);
        let dialog = app.message_ui.poll_voters_dialog.as_ref();
        assert_eq!(
            dialog.map(|dialog| dialog.view),
            Some(PollView::Stats { option: Some(1) }),
            "the chosen option stays"
        );
    });
}
