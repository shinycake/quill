//! The per-scope and per-chat notification volume sliders against a demo
//! window (needs the `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::component::slider::{SliderEvent, SliderValue};
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::ChatId;
use quill::telegram::envelope::NotificationSettingsScope;

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-mute-archive")),
            )
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
fn scope_and_chat_sliders_save_and_resolve_like_tdesktop(cx: &mut TestAppContext) {
    quill::notify_volume::set_persistence(false);
    let (app, mut vcx) = new_app(cx);
    let (account, chat_id, scope) = app.update_in(&mut vcx, |app, _, cx| {
        let Some(session) = app.session() else {
            panic!("demo session missing");
        };
        let account = session.account.0.clone();
        let Some(chat_id) = session.open_chat else {
            panic!("demo has no open chat");
        };
        let Some(chat) = session.chats.get(&chat_id.0) else {
            panic!("open chat missing");
        };
        let scope = quill::state::scope_for_chat_kind(&chat.kind);
        quill::notify_volume::update(&account, |volumes| *volumes = Default::default());
        app.ensure_scope_volume_sliders(cx);
        app.ensure_chat_volume_slider(cx);
        (account, chat_id, scope)
    });

    // Nothing set: the app-wide volume applies and the sliders show 100%.
    app.update_in(&mut vcx, |app, _, _| {
        let Some(session) = app.session() else { return };
        assert_eq!(session.notification_volume_override(chat_id), None);
        assert_eq!(app.account_volumes().chat_slider_percent(chat_id), 100);
    });

    // Letting go of the scope slider saves that scope's percent.
    app.update_in(&mut vcx, |app, _, cx| {
        let Some((_, slider)) = app
            .notify
            .scope_volume_sliders
            .iter()
            .find(|(candidate, _)| *candidate == scope)
            .cloned()
        else {
            panic!("scope slider missing");
        };
        slider.update(cx, |_, cx| {
            cx.emit(SliderEvent::Release(SliderValue::Single(40.0)));
        });
    });
    app.update_in(&mut vcx, |app, _, _| {
        assert_eq!(app.account_volumes().scope_percent(scope), 40);
        let Some(session) = app.session() else { return };
        assert_eq!(session.notification_volume_override(chat_id), Some(40));
    });

    // The chat's own slider wins over the scope's.
    app.update_in(&mut vcx, |app, _, cx| {
        let Some((_, slider)) = app.notify.chat_volume_slider.clone() else {
            panic!("chat slider missing");
        };
        slider.update(cx, |_, cx| {
            cx.emit(SliderEvent::Change(SliderValue::Single(25.0)));
        });
    });
    app.update_in(&mut vcx, |app, _, _| {
        assert_eq!(app.account_volumes().chat_percent(chat_id), 25);
        let Some(session) = app.session() else { return };
        assert_eq!(session.notification_volume_override(chat_id), Some(25));
    });

    // Rows show while the sound is on and hide when it is off.
    app.update_in(&mut vcx, |app, _, cx| {
        assert!(app.scope_volume_row(scope, -1, cx).is_some());
        assert!(app.scope_volume_row(scope, 0, cx).is_none());
        assert!(app.chat_volume_row(chat_id, false, cx).is_some());
        assert!(app.chat_volume_row(chat_id, true, cx).is_none());
        assert!(
            app.chat_volume_row(ChatId(chat_id.0 + 1), false, cx)
                .is_none()
        );
        assert!(
            app.scope_volume_row(NotificationSettingsScope::ALL[0], -1, cx)
                .is_some()
        );
    });

    quill::notify_volume::update(&account, |volumes| *volumes = Default::default());
}
