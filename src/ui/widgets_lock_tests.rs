//! Message widgets leave their windows when Quill locks: a locked window
//! draws no history pass, so nothing else would say they went.
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use crate::ui::widgets_state::{Showing, WidgetEntry};
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};
use quill::message_widget::WidgetKey;
use quill::message_widget::storage::Stored;

#[gpui_kit::test]
fn locking_takes_every_widget_off_screen(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("window built");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    let key = WidgetKey {
        chat_id: ChatId(11),
        message_id: MessageId(5),
    };
    app.update(&mut vcx, |app, cx| {
        let mut entry = WidgetEntry::new(Stored::default(), 120.);
        entry.presence.show(
            None,
            Showing {
                width: 300.,
                view: None,
            },
        );
        app.widgets.entries.insert(key, entry);
        assert!(app.widgets.entries[&key].presence.is_shown());
        app.account.passcode.enabled = true;
        app.lock_by_passcode(cx);
        assert!(
            !app.widgets.entries[&key].presence.is_shown(),
            "the locked window shows no widget"
        );
    });
}
