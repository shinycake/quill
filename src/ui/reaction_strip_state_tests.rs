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
use gpui_kit::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, point, px, size};
use quill::ids::{ChatId, MessageId};
use quill::state::MessageReactionsState;

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
            crate::ui::demos::seed_reaction_options(session, CHAT, MESSAGE);
        }
        cx.notify();
    });
    frames(vcx);
    assert!(has(vcx, "reaction-strip"));
    assert!(!has(vcx, "reaction-strip-loading"));
}
