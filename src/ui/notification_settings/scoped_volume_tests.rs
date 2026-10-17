//! The per-scope and per-chat notification volume sliders against a demo
//! window (needs the `demo-capture` feature for gpui-kit's test support).
use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::component::slider::{SliderEvent, SliderValue};
use gpui_kit::{AppContext, Entity, TestAppContext, VisualTestContext, px, size};
use quill::ids::{AccountKey, ChatId};
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

/// The demo's account, open chat and that chat's scope.
fn facts(app: &QuillApp) -> (String, ChatId, NotificationSettingsScope) {
    let Some(session) = app.session() else {
        panic!("demo session missing");
    };
    let Some(chat_id) = session.open_chat else {
        panic!("demo has no open chat");
    };
    let Some(chat) = session.chats.get(&chat_id.0) else {
        panic!("open chat missing");
    };
    (
        session.account.0.clone(),
        chat_id,
        quill::state::scope_for_chat_kind(&chat.kind),
    )
}

fn override_for(app: &QuillApp, chat_id: ChatId) -> Option<u8> {
    let Some(session) = app.session() else {
        panic!("demo session missing");
    };
    session.notification_volume_override(chat_id)
}

#[gpui_kit::test]
fn volume_sliders_save_on_release_and_are_keyed_by_account_and_chat(cx: &mut TestAppContext) {
    quill::notify_volume::set_persistence(false);
    let (app, mut vcx) = new_app(cx);
    let (account, chat_id, scope) = app.update_in(&mut vcx, |app, _, cx| {
        let (account, chat_id, scope) = facts(app);
        quill::notify_volume::update_memory(&account, |volumes| *volumes = Default::default());
        app.ensure_scope_volume_sliders(cx);
        app.ensure_chat_volume_slider(chat_id, cx);
        assert_eq!(override_for(app, chat_id), None);
        assert_eq!(app.account_volumes().chat_slider_percent(chat_id), 100);
        (account, chat_id, scope)
    });
    let before = quill::notify_volume::persist_requests();

    app.update_in(&mut vcx, |app, _, cx| {
        let Some(slider) = app
            .notify
            .scope_volume_sliders
            .get(&(account.clone(), scope))
            .cloned()
        else {
            panic!("scope slider missing");
        };
        for value in [90.0, 60.0, 41.0] {
            slider.update(cx, |_, cx| {
                cx.emit(SliderEvent::Change(SliderValue::Single(value)));
            });
        }
    });
    app.update_in(&mut vcx, |app, _, _| {
        assert_eq!(app.account_volumes().scope_percent(scope), 41);
        assert_eq!(override_for(app, chat_id), Some(41));
    });
    assert_eq!(
        quill::notify_volume::persist_requests(),
        before,
        "a drag must not touch the disk"
    );

    app.update_in(&mut vcx, |app, _, cx| {
        let Some(slider) = app
            .notify
            .scope_volume_sliders
            .get(&(account.clone(), scope))
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
    });
    assert_eq!(quill::notify_volume::persist_requests(), before + 1);

    // The chat's own slider wins over the scope's.
    app.update_in(&mut vcx, |app, _, cx| {
        let Some(slider) = app
            .notify
            .chat_volume_sliders
            .get(&(account.clone(), chat_id))
            .cloned()
        else {
            panic!("chat slider missing");
        };
        slider.update(cx, |_, cx| {
            cx.emit(SliderEvent::Change(SliderValue::Single(25.0)));
        });
    });
    app.update_in(&mut vcx, |app, _, _| {
        assert_eq!(override_for(app, chat_id), Some(25));
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
    });

    // Two chats (two windows) and two accounts keep separate sliders.
    let keys = app.update_in(&mut vcx, |app, _, cx| {
        let (account, chat_a, scope) = facts(app);
        let chat_b = ChatId(chat_a.0 + 1);
        quill::notify_volume::update_memory(&account, |volumes| *volumes = Default::default());
        quill::notify_volume::update_memory("volume-test-other", |volumes| {
            volumes.set_scope(scope, 30);
            volumes.set_chat(chat_a, 20);
        });

        // Two chats (two windows) each keep their own slider, and asking
        // again keeps the same entity, so a drag is never replaced.
        app.ensure_chat_volume_slider(chat_a, cx);
        app.ensure_chat_volume_slider(chat_b, cx);
        let Some(slider_a) = app
            .notify
            .chat_volume_sliders
            .get(&(account.clone(), chat_a))
            .cloned()
        else {
            panic!("chat A slider missing");
        };
        app.ensure_chat_volume_slider(chat_b, cx);
        app.ensure_chat_volume_slider(chat_a, cx);
        let Some(again) = app
            .notify
            .chat_volume_sliders
            .get(&(account.clone(), chat_a))
        else {
            panic!("chat A slider lost");
        };
        assert_eq!(again.entity_id(), slider_a.entity_id());
        assert_eq!(app.notify.chat_volume_sliders.len(), 2);
        slider_a.update(cx, |_, cx| {
            cx.emit(SliderEvent::Change(SliderValue::Single(55.0)));
        });
        (account, chat_a, chat_b, scope)
    });
    let (account, chat_a, chat_b, scope) = keys;
    app.update_in(&mut vcx, |app, _, cx| {
        assert_eq!(app.account_volumes().chat_percent(chat_a), 55);
        assert_eq!(app.account_volumes().chat_percent(chat_b), 0);

        // Switch to another account: it gets sliders seeded from its own
        // values, and the first account's sliders stay as they were.
        app.ensure_scope_volume_sliders(cx);
        let Some(session) = app.demo_session.as_mut() else {
            panic!("demo session missing");
        };
        session.account = AccountKey("volume-test-other".to_string());
        app.ensure_scope_volume_sliders(cx);
        app.ensure_chat_volume_slider(chat_a, cx);
        let other = "volume-test-other".to_string();
        let value_of = |app: &QuillApp, key: (String, NotificationSettingsScope)| {
            app.notify
                .scope_volume_sliders
                .get(&key)
                .map(|slider| slider.read(cx).value().end())
        };
        assert_eq!(value_of(app, (other.clone(), scope)), Some(30.0));
        assert_eq!(value_of(app, (account.clone(), scope)), Some(100.0));
        let chat_key = (other.clone(), chat_a);
        let Some(other_chat) = app.notify.chat_volume_sliders.get(&chat_key) else {
            panic!("other account's chat slider missing");
        };
        assert_eq!(other_chat.read(cx).value().end(), 20.0);
        assert_eq!(app.notify.chat_volume_sliders.len(), 3);
        assert_eq!(app.account_volumes().scope_slider_percent(scope), 30);

        quill::notify_volume::update_memory(&account, |volumes| *volumes = Default::default());
        quill::notify_volume::update_memory(&other, |volumes| *volumes = Default::default());
    });
}
