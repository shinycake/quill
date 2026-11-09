//! A send that is not the composer's message (a shared story, a contact,
//! a die, a location) carries its destination's own "send silently"
//! choice, and none of the open chat's pending schedule or effect. Over
//! the `ready-composer-bars` demo; nothing is sent.
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::composer::ComposerScheduling;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(700.)), move |window, cx| {
        let demo = ScreenshotDemo::named("ready-composer-bars");
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("the app");
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    (app, vcx)
}

#[gpui_kit::test]
fn a_share_does_not_take_the_open_chats_composer_choices(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update(&mut vcx, |app, _| {
        let Some(open) = app.open_chat_id() else {
            panic!("the demo opens a chat");
        };
        let Some(other) = app
            .session()
            .and_then(|session| session.chats.keys().copied().find(|id| *id != open.0))
            .map(quill::ids::ChatId)
        else {
            panic!("the demo has another chat");
        };
        // The open chat X: silent, with a schedule and an effect pending.
        if !app.composer_effective_silent() {
            app.toggle_composer_silent();
        }
        app.composer_ui.send_effect_once = 77;
        app.update_pending_send(|send| send.scheduling = ComposerScheduling::SendWhenOnline);
        let composer = app.composer_send_options();
        assert!(composer.disable_notification);
        assert_eq!(composer.effect_id, 77);
        assert_eq!(composer.scheduling, ComposerScheduling::SendWhenOnline);

        // A share to chat Y: Y's own choice, nothing of X's.
        let share = app.send_options_to(other);
        assert!(!share.disable_notification, "Y was never set silent");
        assert_eq!(share.effect_id, 0);
        assert_eq!(share.scheduling, ComposerScheduling::None);

        // A contact or die to X itself keeps X's silent choice only.
        let same = app.send_options_to(open);
        assert!(same.disable_notification);
        assert_eq!(same.effect_id, 0);
        assert_eq!(same.scheduling, ComposerScheduling::None);
        // The composer's next message still has its schedule and effect.
        assert_eq!(app.composer_send_options().effect_id, 77);
    });
}
