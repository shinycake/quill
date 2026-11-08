//! A chat window's message menu takes that window's keyboard and gives it
//! back (round 12, area B, finding 2): the focus hand-off and the item
//! list are the window's own, not the main window's.

use super::tests::{Harness, OTHER, OTHER_LAST, new_app};
use crate::ui::app::QuillApp;
use crate::ui::menu_states::MessageMenuState;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Context, FocusHandle, Focusable, TestAppContext, point, px,
};

fn scoped<R>(
    h: &Harness,
    cx: &mut TestAppContext,
    f: impl FnOnce(&mut QuillApp, &mut Context<QuillApp>) -> R,
) -> R {
    h.app
        .update(cx, |app, cx| app.in_window_scope(Some(OTHER), cx, f))
        .expect("the chat window's state")
}

fn render(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
        .expect("the window");
    cx.run_until_parked();
}

fn focus_state(
    handle: AnyWindowHandle,
    menu: &FocusHandle,
    composer: &FocusHandle,
    cx: &mut TestAppContext,
) -> (bool, bool, bool) {
    cx.update_window(handle, |_, window, cx| {
        (
            menu.contains_focused(window, cx),
            menu.is_focused(window),
            composer.is_focused(window),
        )
    })
    .expect("the window")
}

#[gpui_kit::test]
fn a_chat_windows_message_menu_takes_and_gives_back_its_focus(cx: &mut TestAppContext) {
    let h = new_app(cx);
    let chat_window = h.open_in_new_window(OTHER, cx);
    h.settle(cx);
    let (menu, composer) = scoped(&h, cx, |app, cx| {
        (
            app.frame.menu.focus.clone(),
            app.composer.read(cx).focus_handle(cx),
        )
    });
    cx.update_window(chat_window, |_, window, cx| window.focus(&composer, cx))
        .expect("the chat window");
    render(chat_window, cx);
    scoped(&h, cx, |app, cx| {
        app.message_ui.menu = Some(MessageMenuState {
            chat_id: OTHER,
            message_id: OTHER_LAST,
            position: point(px(200.), px(200.)),
        });
        cx.notify();
    });
    render(chat_window, cx);
    let (inside, _, on_composer) = focus_state(chat_window, &menu, &composer, cx);
    assert!(inside, "the menu takes the window's focus");
    assert!(!on_composer, "so Enter no longer reaches the composer");

    // The main window draws in between; the chat window's rows stay its own.
    render(h.main, cx);
    render(chat_window, cx);
    cx.simulate_keystrokes(chat_window, "down");
    render(chat_window, cx);
    let (inside, on_frame, _) = focus_state(chat_window, &menu, &composer, cx);
    assert!(
        inside && !on_frame,
        "Down lands on one of the window's rows"
    );
    let rows = scoped(&h, cx, |app, _| app.frame.menu.items.enabled_count());
    assert!(rows > 0, "the window's own rows were claimed");
    render(chat_window, cx);
    let again = scoped(&h, cx, |app, _| app.frame.menu.items.enabled_count());
    assert_eq!(again, rows, "a frame claims each row once");

    scoped(&h, cx, |app, cx| {
        app.message_ui.menu = None;
        cx.notify();
    });
    render(chat_window, cx);
    let (inside, _, on_composer) = focus_state(chat_window, &menu, &composer, cx);
    assert!(!inside);
    assert!(
        on_composer,
        "closing the menu gives the composer its focus back"
    );
}
