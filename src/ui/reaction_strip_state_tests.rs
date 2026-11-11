//! The message menu's reaction strip follows the options' state: a
//! placeholder only while the request is in flight, the strip once they
//! arrive, nothing when they are unavailable (tdesktop skips the selector
//! when it has no reactions, `AttachSelectorToMenu`).
use crate::ui::app::QuillApp;
use crate::ui::keybindings::bind_keys;
use crate::ui::menu_states::MessageMenuState;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Entity, Focusable, Modifiers, TestAppContext, VisualTestContext, point, px, size,
};
use quill::connect::{ConnectDriver, RecordingSender, prepare_connect};
use quill::credentials::TelegramCredentials;
use quill::diagnostics::{DiagnosticSink, MemorySink};
use quill::ids::{AccountKey, ChatId, MessageId};
use quill::platform::MemorySecretStore;
use quill::state::{MessageReactionsState, Session};
use quill::telegram::client::copy_and_parse;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

const CHAT: ChatId = ChatId(11);
const MESSAGE: MessageId = MessageId(101);

fn new_app(cx: &mut TestAppContext) -> (Entity<QuillApp>, VisualTestContext) {
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let handle = cx.open_window(size(px(1100.), px(900.)), move |window, cx| {
        let view = cx.new(|cx| {
            QuillApp::new_with_demo(
                window,
                cx,
                None,
                Some(ScreenshotDemo::named("ready-reactions")),
            )
        });
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().unwrap();
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    frames(&mut vcx);
    (app, vcx)
}

fn frames(cx: &mut VisualTestContext) {
    for _ in 0..2 {
        cx.update(|window, cx| window.render_frame(cx));
    }
    cx.run_until_parked();
}

fn show_menu_with(
    app: &Entity<QuillApp>,
    cx: &mut VisualTestContext,
    state: MessageReactionsState,
) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_message_menu(
                MessageMenuState {
                    chat_id: CHAT,
                    message_id: MESSAGE,
                    position: point(px(420.), px(300.)),
                },
                window,
                cx,
            );
            if let Some(session) = app.demo_session.as_mut() {
                session.stickers.message_reaction_options = state;
            }
            cx.notify();
        });
    });
    frames(cx);
}

fn has(cx: &mut VisualTestContext, id: &'static str) -> bool {
    cx.update(|window, _| window.try_find(id).is_some())
}

#[gpui_kit::test]
fn strip_is_absent_when_the_options_are_unavailable(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    show_menu_with(
        &app,
        vcx,
        MessageReactionsState::Unavailable {
            chat_id: CHAT,
            message_id: MESSAGE,
        },
    );
    assert!(has(vcx, "menu-reply"));
    assert!(!has(vcx, "reaction-strip"));
    assert!(!has(vcx, "reaction-strip-loading"));
}

#[gpui_kit::test]
fn strip_is_absent_when_nothing_was_asked(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    show_menu_with(&app, vcx, MessageReactionsState::Idle);
    assert!(has(vcx, "menu-reply"));
    assert!(!has(vcx, "reaction-strip"));
    assert!(!has(vcx, "reaction-strip-loading"));
}

#[gpui_kit::test]
fn placeholder_shows_only_while_loading(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    show_menu_with(
        &app,
        vcx,
        MessageReactionsState::Loading {
            chat_id: CHAT,
            message_id: MESSAGE,
        },
    );
    assert!(has(vcx, "reaction-strip-loading"));
    assert!(!has(vcx, "reaction-strip"));
}

#[gpui_kit::test]
fn strip_shows_once_the_options_are_ready(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    show_menu_with(&app, vcx, MessageReactionsState::Idle);
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            crate::ui::demos::reaction_fixtures::seed_reaction_options(session, CHAT, MESSAGE);
        }
        cx.notify();
    });
    frames(vcx);
    assert!(has(vcx, "reaction-strip"));
    assert!(!has(vcx, "reaction-strip-loading"));
}

#[gpui_kit::test]
fn ready_options_of_another_message_draw_no_strip(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    show_menu_with(&app, vcx, MessageReactionsState::Idle);
    app.update(vcx, |app, cx| {
        if let Some(session) = app.demo_session.as_mut() {
            crate::ui::demos::reaction_fixtures::seed_reaction_options(
                session,
                CHAT,
                MessageId(999),
            );
        }
        cx.notify();
    });
    frames(vcx);
    assert!(has(vcx, "menu-reply"));
    assert!(!has(vcx, "reaction-strip"));
    assert!(!has(vcx, "reaction-strip-loading"));
}

#[gpui_kit::test]
fn opening_the_menu_shows_the_strip_and_closing_it_resets_the_options(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    vcx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_message_menu(
                MessageMenuState {
                    chat_id: CHAT,
                    message_id: MESSAGE,
                    position: point(px(420.), px(300.)),
                },
                window,
                cx,
            );
        });
    });
    frames(vcx);
    assert!(has(vcx, "reaction-strip"));
    app.update(vcx, |app, cx| {
        assert!(app.close_context_menus(cx));
    });
    frames(vcx);
    let state = app.read_with(vcx, |app, _| {
        app.demo_session
            .as_ref()
            .map(|s| s.stickers.message_reaction_options.clone())
    });
    assert_eq!(state, Some(MessageReactionsState::Idle));
    assert!(!has(vcx, "reaction-strip"));
}

#[gpui_kit::test]
fn clicking_away_resets_the_options_like_escape(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    show_menu_with(
        &app,
        vcx,
        MessageReactionsState::Loading {
            chat_id: CHAT,
            message_id: MESSAGE,
        },
    );
    assert!(has(vcx, "reaction-strip-loading"));
    vcx.simulate_click(point(px(20.), px(20.)), Modifiers::default());
    frames(vcx);
    let (menu_open, state) = app.read_with(vcx, |app, _| {
        (
            app.message_ui.menu.is_open(),
            app.demo_session
                .as_ref()
                .map(|s| s.stickers.message_reaction_options.clone()),
        )
    });
    assert!(!menu_open);
    assert_eq!(state, Some(MessageReactionsState::Idle));
    assert!(!has(vcx, "reaction-strip-loading"));
}

/// The reducer state a real driver reaches: ask, then ingest TDLib's reply.
fn driver_state(answer: bool) -> MessageReactionsState {
    let sink: Arc<dyn DiagnosticSink> = Arc::new(MemorySink::new());
    let recorder = Arc::new(RecordingSender::new());
    let credentials = TelegramCredentials {
        api_id: 99,
        api_hash: "unit-test-hash-not-for-network".into(),
    };
    let dir = quill::scratch::unique_dir("quill-reaction-strip");
    let store = MemorySecretStore::new();
    let prepared =
        prepare_connect(&dir, AccountKey::primary(), &store, &credentials).expect("prepare");
    let session = Session::new(AccountKey::primary(), sink.clone());
    let mut driver = ConnectDriver::new(session, recorder, credentials, prepared);
    let seq = AtomicU64::new(0);
    let ingest = |driver: &mut ConnectDriver<Arc<RecordingSender>>, json: &str| {
        if let Some(parsed) = copy_and_parse(json, &seq, &sink) {
            let _ = driver.ingest(parsed);
        }
    };
    ingest(
        &mut driver,
        r#"{"@type":"updateAuthorizationState","authorization_state":{"@type":"authorizationStateReady"}}"#,
    );
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateNewChat","chat":{{"id":{0},"title":"Alice","type":{{"@type":"chatTypePrivate","user_id":{0}}},"unread_count":0}}}}"#,
            CHAT.0
        ),
    );
    ingest(
        &mut driver,
        &format!(
            r#"{{"@type":"updateChatPosition","chat_id":{},"position":{{"@type":"chatPosition","list":{{"@type":"chatListMain"}},"order":"9","is_pinned":false}}}}"#,
            CHAT.0
        ),
    );
    let extra = driver.fetch_message_reactions(CHAT, MESSAGE).ok().flatten();
    if answer && let Some(extra) = extra {
        ingest(
            &mut driver,
            &format!(
                r#"{{"@type":"availableReactions","@extra":"{}","top_reactions":[{{"@type":"availableReaction","type":{{"@type":"reactionTypeEmoji","emoji":"👍"}},"needs_premium":false}}],"recent_reactions":[],"popular_reactions":[],"allow_custom_emoji":false,"are_tags":false,"unavailability_reason":null}}"#,
                extra.0
            ),
        );
    }
    driver.session.stickers.message_reaction_options.clone()
}

fn show_driver_state(cx: &mut TestAppContext, answer: bool) -> (bool, bool) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    let state = driver_state(answer);
    show_menu_with(&app, vcx, state);
    (
        has(vcx, "reaction-strip-loading"),
        has(vcx, "reaction-strip"),
    )
}

#[gpui_kit::test]
fn driver_request_in_flight_renders_the_skeleton(cx: &mut TestAppContext) {
    assert_eq!(show_driver_state(cx, false), (true, false));
}

#[gpui_kit::test]
fn driver_answer_renders_the_strip(cx: &mut TestAppContext) {
    assert_eq!(show_driver_state(cx, true), (false, true));
}

const OUTGOING: MessageId = MessageId(102);

fn make_secret(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        app.update(cx, |app, _| {
            if let Some(chat) = app
                .demo_session
                .as_mut()
                .and_then(|session| session.chats.get_mut(&CHAT.0))
            {
                chat.kind = quill::telegram::envelope::ChatKind::Secret {
                    secret_chat_id: 9,
                    user_id: quill::ids::UserId(CHAT.0),
                };
            }
        });
    });
}

fn open_menu(app: &Entity<QuillApp>, cx: &mut VisualTestContext, message_id: MessageId) {
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.open_message_menu(
                MessageMenuState {
                    chat_id: CHAT,
                    message_id,
                    position: point(px(420.), px(300.)),
                },
                window,
                cx,
            );
        });
    });
    frames(cx);
}

#[gpui_kit::test]
fn a_secret_chat_menu_opens_with_only_what_works_there(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    make_secret(&app, vcx);
    open_menu(&app, vcx, OUTGOING);
    for row in ["menu-reply", "menu-copy", "menu-delete", "menu-select"] {
        assert!(has(vcx, row), "{row} is offered");
    }
    for row in [
        "menu-forward",
        "menu-edit",
        "menu-toggle-pin",
        "reaction-strip",
        "reaction-strip-loading",
    ] {
        assert!(!has(vcx, row), "{row} is not offered");
    }
}

#[gpui_kit::test]
fn a_secret_chat_menu_has_no_report_row(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    make_secret(&app, vcx);
    open_menu(&app, vcx, MESSAGE);
    assert!(has(vcx, "menu-reply"));
    assert!(!has(vcx, "menu-report"));
}

#[gpui_kit::test]
fn a_secret_chat_selection_offers_no_forward(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    make_secret(&app, vcx);
    select_message(&app, vcx);
    assert!(has(vcx, "selection-delete"));
    assert!(has(vcx, "selection-copy"));
    assert!(!has(vcx, "selection-forward"));
    assert!(!has(vcx, "selection-report"));
}

fn select_message(app: &Entity<QuillApp>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        app.update(cx, |app, cx| {
            app.close_message_menu();
            app.toggle_forward_select(CHAT, MESSAGE, false, cx);
        });
    });
    frames(cx);
    cx.run_until_parked();
    frames(cx);
}

#[gpui_kit::test]
fn a_secret_chat_ignores_the_quick_reaction(cx: &mut TestAppContext) {
    let (app, mut vcx) = new_app(cx);
    let vcx = &mut vcx;
    make_secret(&app, vcx);
    let chips = |app: &QuillApp| {
        let session = app.demo_session.as_ref().expect("demo session");
        session.histories[&CHAT.0].messages[&OUTGOING.0]
            .interaction_info
            .clone()
    };
    vcx.update(|_, cx| {
        app.update(cx, |app, cx| {
            let before = chips(app);
            if let Some(session) = app.demo_session.as_mut() {
                session.stickers.default_reaction =
                    Some(quill::state::ReactionChoice::Emoji("👍".to_string()));
            }
            app.quick_react(CHAT, OUTGOING, cx);
            assert_eq!(chips(app), before);
            assert!(app.history.reaction_fly.is_none());
        });
    });
}
