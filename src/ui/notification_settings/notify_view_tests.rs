//! The notification "Show sender name" / "Show message text" switches
//! against a demo window (needs the `demo-capture` feature for gpui-kit's
//! test support).
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::notify::NotifyView;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(window, cx, None, Some(ScreenshotDemo::named("ready-chats")))
        });
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let Some(app) = slot.borrow().clone() else {
        panic!("demo window did not build");
    };
    (app, VisualTestContext::from_window(handle.into(), cx))
}

#[gpui_kit::test]
fn previews_show_by_default_and_the_switches_follow_tdesktop(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, _, cx| {
        // Regression: previews used to be hidden with no way to show them.
        assert_eq!(app.notify_view(), NotifyView::ShowPreview);
        assert!(app.notify_view_section(cx).is_some());
        app.set_notify_view(app.notify_view().toggled(false, false), cx);
        assert_eq!(app.notify_view(), NotifyView::ShowName);
        app.set_notify_view(app.notify_view().toggled(true, false), cx);
        assert_eq!(app.notify_view(), NotifyView::ShowNothing);
        // With desktop notifications off the switches are hidden.
        app.set_desktop_notifications(false, cx);
        assert!(app.notify_view_section(cx).is_none());
    });
}
