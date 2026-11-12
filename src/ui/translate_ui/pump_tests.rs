use crate::ui::app::QuillApp;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::{AppContext, TestAppContext, VisualTestContext, px, size};
use quill::ids::{ChatId, MessageId};
use quill::wake::UPDATES;

const CHANNEL: ChatId = ChatId(13);
const NEWEST_POST: MessageId = MessageId(208);

#[gpui_kit::test]
fn a_picked_language_is_asked_for_by_the_pass_the_pick_wakes(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chat-translate");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    let Some(app) = slot.borrow().clone() else {
        panic!("the window opened");
    };
    let shown_in = app.update_in(&mut vcx, |app, window, cx| {
        app.toggle_chat_translation(CHANNEL, cx);
        app.open_translate_chooser("en", window, cx);
        let session = app.session().expect("the demo session");
        session.chat_translated_to(CHANNEL).map(str::to_string)
    });
    assert_eq!(shown_in.as_deref(), Some("en"));

    let before = UPDATES.pokes_on_this_thread();
    app.update(&mut vcx, |app, cx| app.choose_translate_language("ar", cx));
    assert!(
        UPDATES.pokes_on_this_thread() > before,
        "a resting poll loop runs a pass"
    );

    let asked = app.update(&mut vcx, |app, _| {
        app.pump_translation();
        app.translate.asked.clone()
    });
    assert_eq!(
        asked.first(),
        Some(&(NEWEST_POST, "ar")),
        "asked: {asked:?}"
    );
}

fn new_app(cx: &mut TestAppContext) -> (gpui_kit::Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chat-translate");
    let handle = cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        let shell = cx.new(|_| QuillShell::new(view.clone()));
        *slot_in.borrow_mut() = Some(view);
        Root::new(shell, window, cx)
    });
    let vcx = VisualTestContext::from_window(handle.into(), cx);
    vcx.run_until_parked();
    let Some(app) = slot.borrow().clone() else {
        panic!("the window opened");
    };
    (app, vcx)
}

/// Two windows showing different chats each keep their offered language:
/// the cache holds one entry per chat, not one slot they fight over.
#[gpui_kit::test]
fn the_offer_cache_keeps_an_entry_for_each_chat(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update(&mut vcx, |app, _| {
        let chats: Vec<ChatId> = app
            .session()
            .expect("the demo session")
            .histories
            .iter()
            .filter(|(_, history)| !history.messages.is_empty())
            .map(|(id, _)| ChatId(*id))
            .take(2)
            .collect();
        assert_eq!(chats.len(), 2, "the demo has two chats with messages");
        for chat in &chats {
            let _ = app.translate_offer(*chat, &[]);
        }
        let holds =
            |app: &QuillApp, chat: ChatId| app.translate.offer.borrow().contains_key(&chat.0);
        assert!(
            holds(app, chats[0]) && holds(app, chats[1]),
            "one entry each"
        );
        // Asking again for the first does not evict the second.
        let _ = app.translate_offer(chats[0], &[]);
        assert!(holds(app, chats[0]) && holds(app, chats[1]));
    });
}

/// A selection whose translation could not be sent says it failed; its box
/// does not sit on the loading placeholder for good.
#[gpui_kit::test]
fn a_refused_selection_translation_shows_failed_not_loading(cx: &mut TestAppContext) {
    use quill::state::Translation;
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_translate_selection(CHANNEL, "hola".into(), window, cx);
        assert_eq!(app.dialog_translation(), None, "nothing asked of the demo");
        if let Some(shown) = app
            .translate
            .dialog
            .as_mut()
            .and_then(|dialog| dialog.current_box_mut())
        {
            shown.ask = super::TranslateAsk::Refused;
        }
        assert!(matches!(
            app.dialog_translation(),
            Some(Translation::Failed { .. })
        ));
    });
}

/// The chooser opens over the box and goes back to it; a pick shows the box
/// again in the new language. A chooser with no box under it closes.
#[gpui_kit::test]
fn the_chooser_returns_to_the_box_it_opened_over(cx: &mut TestAppContext) {
    use super::TranslateStage;
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.open_translate_selection(CHANNEL, "hola".into(), window, cx);
        let Some(dialog) = app.translate.dialog.as_mut() else {
            panic!("the box opened");
        };
        dialog.open_chooser();
        assert!(matches!(
            dialog.stage,
            TranslateStage::Choosing { under: Some(_) }
        ));
        assert!(dialog.back_to_box());
        assert!(matches!(dialog.stage, TranslateStage::Box(_)));
        dialog.open_chooser();
        assert!(dialog.pick("ar"));
        assert!(matches!(dialog.stage, TranslateStage::Box(_)));
        assert_eq!(dialog.to, "ar");

        app.open_translate_chooser("en", window, cx);
        let Some(dialog) = app.translate.dialog.as_mut() else {
            panic!("the chooser opened");
        };
        assert!(!dialog.back_to_box(), "nothing under it");
        assert!(!dialog.pick("de"), "a pick ends a chooser with no box");
    });
}

/// A toast leaves when its own timer fires, and not before; a newer toast
/// replaces it (its timer goes with it).
#[gpui_kit::test]
fn a_toast_leaves_with_its_own_timer(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    app.update(&mut vcx, |app, cx| {
        app.show_translate_toast(
            CHANNEL,
            "one".into(),
            "Undo",
            super::ToastAction::ShowBar,
            cx,
        );
        assert!(app.translate.toast.toast().is_some());
    });
    vcx.executor().advance_clock(super::TOAST_DURATION / 2);
    vcx.run_until_parked();
    app.update(&mut vcx, |app, cx| {
        assert!(app.translate.toast.toast().is_some(), "not yet");
        app.show_translate_toast(
            CHANNEL,
            "two".into(),
            "Undo",
            super::ToastAction::ShowBar,
            cx,
        );
    });
    vcx.executor()
        .advance_clock(super::TOAST_DURATION / 2 + std::time::Duration::from_millis(1));
    vcx.run_until_parked();
    app.update(&mut vcx, |app, _| {
        assert!(
            app.translate.toast.toast().is_some(),
            "the replaced toast's timer did not clear the newer one"
        );
    });
    vcx.executor().advance_clock(super::TOAST_DURATION);
    vcx.run_until_parked();
    app.update(&mut vcx, |app, _| {
        assert!(app.translate.toast.toast().is_none())
    });
}

/// A translation that failed is asked for again when the chat is shown
/// again, not on every pass and not never.
#[gpui_kit::test]
fn a_failed_translation_is_asked_again_when_the_chat_is_reopened(cx: &mut TestAppContext) {
    use quill::state::Translation;
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.toggle_chat_translation(CHANNEL, cx);
        // The language is whatever the saved preferences give (another test
        // may have saved one), so the test reads it back.
        app.open_translate_chooser("en", window, cx);
    });
    let to = app.read_with(&vcx, |app, _| {
        app.session()
            .and_then(|s| s.chat_translated_to(CHANNEL))
            .unwrap_or("en")
            .to_string()
    });
    let failed = |app: &QuillApp| {
        matches!(
            app.session()
                .and_then(|s| s.message_translation(CHANNEL, NEWEST_POST, &to)),
            Some(Translation::Failed { .. })
        )
    };
    app.update(&mut vcx, |app, _| {
        app.pump_translation();
        if let Some(session) = app.session_mut() {
            session.messages.translate.messages.insert(
                (CHANNEL.0, NEWEST_POST.0, to.to_string()),
                Translation::failed("send"),
            );
        }
        app.translate.asked.clear();
        // A pass of the same shown chat leaves the failure alone.
        app.pump_translation();
        assert!(failed(app), "not asked again on every pass");
        assert!(app.translate.asked.is_empty());
        // Leaving for a chat that is not translated drops the scan, so the
        // chat shown again starts a new one, which asks once more.
        let other = app
            .session()
            .and_then(|s| s.chats.keys().copied().find(|id| *id != CHANNEL.0))
            .map(ChatId)
            .expect("the demo has another chat");
        if let Some(session) = app.session_mut() {
            session.open_chat(other);
        }
        app.pump_translation();
        assert!(failed(app), "a chat that is not shown is not asked for");
        assert!(app.translate.pump.is_empty(), "its scan left with it");
        if let Some(session) = app.session_mut() {
            session.open_chat(CHANNEL);
        }
        app.pump_translation();
        assert!(!failed(app), "the failure is dropped for another ask");
        assert!(
            app.translate.asked.contains(&(NEWEST_POST, to.as_str())),
            "asked: {:?}",
            app.translate.asked
        );
    });
}

/// Sweep r2-06b 14-042: the bar's toggle turned on again asks for what
/// failed, so a failure is not stuck until the chat is closed.
#[gpui_kit::test]
fn turning_translation_on_again_asks_for_failed_messages(cx: &mut TestAppContext) {
    use quill::state::Translation;
    let (app, mut vcx) = new_app(cx);
    app.update_in(&mut vcx, |app, window, cx| {
        app.toggle_chat_translation(CHANNEL, cx);
        // The language is whatever the saved preferences give (another test
        // may have saved one), so the test reads it back.
        app.open_translate_chooser("en", window, cx);
    });
    let to = app.read_with(&vcx, |app, _| {
        app.session()
            .and_then(|s| s.chat_translated_to(CHANNEL))
            .unwrap_or("en")
            .to_string()
    });
    app.update_in(&mut vcx, |app, _, cx| {
        app.pump_translation();
        if let Some(session) = app.session_mut() {
            session.messages.translate.messages.insert(
                (CHANNEL.0, NEWEST_POST.0, to.to_string()),
                Translation::failed("send"),
            );
        }
        app.pump_translation();
        app.translate.asked.clear();
        // Off, then on again.
        app.toggle_chat_translation(CHANNEL, cx);
        app.toggle_chat_translation(CHANNEL, cx);
        app.pump_translation();
        assert!(
            app.translate.asked.contains(&(NEWEST_POST, to.as_str())),
            "asked: {:?}",
            app.translate.asked
        );
    });
}

/// Sweep r4-12 23-011: a failure that ran out of attempts while offline is
/// asked for again after the reconnect, with the chat open all along.
#[gpui_kit::test]
fn a_failure_out_of_attempts_is_asked_again_after_a_reconnect(cx: &mut TestAppContext) {
    use quill::state::{TRANSLATE_PUMP_ATTEMPTS, Translation};
    let (app, mut vcx) = new_app(cx);
    // The chat shows in Arabic. Set in memory and on the session: picking
    // it through the chooser would save the prefs files the other tests of
    // this module share.
    app.update(&mut vcx, |app, _| {
        app.preferences.translate.prefs.translate_to = "ar".into();
        if let Some(session) = app.session_mut() {
            session.set_chat_translated_to(CHANNEL, Some("ar"));
        }
    });
    let to = "ar".to_string();
    app.update(&mut vcx, |app, _| {
        // Shown once (as the reopen tests above get it): the pass asks.
        let other = app
            .session()
            .and_then(|s| s.chats.keys().copied().find(|id| *id != CHANNEL.0))
            .map(ChatId)
            .expect("the demo has another chat");
        if let Some(session) = app.session_mut() {
            session.open_chat(other);
        }
        app.pump_translation();
        if let Some(session) = app.session_mut() {
            session.open_chat(CHANNEL);
        }
        app.pump_translation();
        assert!(
            app.translate.asked.contains(&(NEWEST_POST, to.as_str())),
            "first pass asked: {:?}",
            app.translate.asked
        );
        if let Some(session) = app.session_mut() {
            session.messages.translate.messages.insert(
                (CHANNEL.0, NEWEST_POST.0, to.to_string()),
                Translation::Failed {
                    reason: "offline".into(),
                    attempts: TRANSLATE_PUMP_ATTEMPTS,
                },
            );
        }
        app.translate.asked.clear();
        app.pump_translation();
        assert!(
            !app.translate.asked.iter().any(|(id, _)| *id == NEWEST_POST),
            "out of attempts: {:?}",
            app.translate.asked
        );
        if let Some(session) = app.session_mut() {
            session.translations_back_online();
        }
        app.pump_translation();
        assert!(
            app.translate.asked.contains(&(NEWEST_POST, to.as_str())),
            "asked: {:?}",
            app.translate.asked
        );
    });
}

/// Sweep r4-12 23-011 (live path): a message whose ask is out when a later
/// pass runs stays in the scan, so when the ask fails out of attempts and
/// the connection comes back, the pass asks again.
#[gpui_kit::test]
fn a_pending_ask_that_fails_is_asked_again_after_a_reconnect(cx: &mut TestAppContext) {
    use quill::ids::RequestId;
    use quill::state::{TRANSLATE_PUMP_ATTEMPTS, Translation};
    let (app, mut vcx) = new_app(cx);
    // The chat shows in Arabic (set on the session: nothing saved to the
    // prefs files the other tests share).
    app.update(&mut vcx, |app, _| {
        app.preferences.translate.prefs.translate_to = "ar".into();
        if let Some(session) = app.session_mut() {
            session.set_chat_translated_to(CHANNEL, Some("ar"));
        }
    });
    let to = "ar".to_string();
    app.update(&mut vcx, |app, _| {
        let key = (CHANNEL.0, NEWEST_POST.0, to.clone());
        let pending = |attempts| Translation::Pending {
            request: RequestId(9_001),
            attempts,
        };
        if let Some(session) = app.session_mut() {
            session
                .messages
                .translate
                .messages
                .insert(key.clone(), pending(0));
        }
        // A pass while the ask is out: it does not ask again.
        app.translate.asked.clear();
        app.pump_translation();
        assert!(!app.translate.asked.iter().any(|(id, _)| *id == NEWEST_POST));
        for round in 0..TRANSLATE_PUMP_ATTEMPTS {
            if let Some(session) = app.session_mut() {
                if round > 0 {
                    session
                        .messages
                        .translate
                        .messages
                        .insert(key.clone(), pending(round));
                }
                session.finish_translation(RequestId(9_001), Translation::failed("offline"));
            }
            app.pump_translation();
        }
        app.translate.asked.clear();
        app.pump_translation();
        assert!(!app.translate.asked.iter().any(|(id, _)| *id == NEWEST_POST));
        if let Some(session) = app.session_mut() {
            session.translations_back_online();
        }
        app.pump_translation();
        assert!(
            app.translate.asked.contains(&(NEWEST_POST, to.as_str())),
            "asked: {:?}",
            app.translate.asked
        );
    });
}

/// A Hide that TDLib refuses (the answer to its own request) ends the
/// "hidden, Undo" toast and says why; the same refusal after the toast went
/// still says so; and an unrelated change of the flag is not a refusal.
#[gpui_kit::test]
fn a_refused_hide_ends_its_undo_toast_with_an_error_note(cx: &mut TestAppContext) {
    use super::HideStage;
    use quill::ids::RequestId;
    use quill::state::Answer;
    // The Hide as the driver sends it: a request in flight, awaited.
    let hide_sent = |app: &mut QuillApp, cx: &mut gpui_kit::Context<QuillApp>| -> RequestId {
        app.show_translate_toast(
            CHANNEL,
            "hidden".into(),
            "Undo",
            super::ToastAction::ShowBar,
            cx,
        );
        let request = app
            .session_mut()
            .map(|session| {
                let request = session.request(
                    quill::state::RequestPurpose::ToggleChatIsTranslatable,
                    Some(CHANNEL),
                );
                session.acks.expect(request);
                request
            })
            .expect("a session");
        app.translate.hide = HideStage::Sent(request);
        request
    };
    let failed = |app: &mut QuillApp, request: RequestId| {
        if let Some(session) = app.session_mut() {
            // As the reducer answers it: out of flight, the answer settled.
            drop(session.requests.take(request));
            session.acks.settle(request, Answer::Failed("no".into()));
        }
    };
    let (app, mut vcx) = new_app(cx);
    app.update(&mut vcx, |app, cx| {
        let request = hide_sent(app, cx);
        // An unrelated `updateChatIsTranslatable true` is no refusal.
        if let Some(demo) = app.demo_session.as_mut() {
            demo.messages.translate.translatable_chats.insert(CHANNEL.0);
        }
        app.pump_translation();
        assert!(app.translate.toast.toast().is_some(), "the hide stands");
        assert!(matches!(app.translate.hide, HideStage::Sent(_)));
        // The answer is the refusal.
        failed(app, request);
        assert!(app.pump_translation(), "the pass reports the change");
        assert!(app.translate.toast.toast().is_none(), "the toast ended");
        assert!(matches!(app.translate.hide, HideStage::Idle));
        assert_eq!(
            app.connection.note().text(),
            quill::lang::tr(quill::lang::keys::Q_TRANSLATE_HIDE_FAILED)
        );
    });
    // A refusal that comes after the toast went still says so.
    app.update(&mut vcx, |app, cx| {
        app.connection.clear_note();
        let request = hide_sent(app, cx);
        app.translate.toast = super::ToastStage::None;
        failed(app, request);
        assert!(app.pump_translation());
        assert_eq!(
            app.connection.note().text(),
            quill::lang::tr(quill::lang::keys::Q_TRANSLATE_HIDE_FAILED)
        );
    });
    // An Undo forgets the wait: a later answer says nothing.
    app.update(&mut vcx, |app, cx| {
        app.connection.clear_note();
        let request = hide_sent(app, cx);
        app.forget_hide();
        failed(app, request);
        assert!(!app.pump_translation());
        assert_ne!(
            app.connection.note().text(),
            quill::lang::tr(quill::lang::keys::Q_TRANSLATE_HIDE_FAILED)
        );
    });
}
