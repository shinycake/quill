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
        if let Some(dialog) = app.translate.dialog.as_mut() {
            dialog.ask = super::TranslateAsk::Refused;
        }
        assert!(matches!(
            app.dialog_translation(),
            Some(Translation::Failed(_))
        ));
    });
}
