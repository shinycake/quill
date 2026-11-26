#![cfg(test)]
#![cfg(feature = "demo-capture")]
//! The window's shortcuts keep working after the element that had the
//! keyboard goes away, in both layouts (`keyboard_home.rs`). GPUI runs a
//! key's action along the focused element's path; a focus left on Settings
//! after it closed cut every root shortcut off, ⌘, included.

use crate::ui::app::QuillApp;
use crate::ui::appearance::own_appearance_prefs;
use crate::ui::keybindings::bind_keys;
use crate::ui::screenshot_demo::ScreenshotDemo;
use crate::ui::shell::QuillShell;
use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    Action, AppContext, Entity, Focusable, Modifiers, TestAppContext, VisualTestContext, point, px,
    size,
};
use quill::layout_style::LayoutStyle;

/// ⌘, on macOS, Ctrl+, elsewhere.
const SETTINGS_CHORD: &str = if cfg!(target_os = "macos") {
    "cmd-,"
} else {
    "ctrl-,"
};

fn frames(vcx: &mut VisualTestContext) {
    vcx.run_until_parked();
    for _ in 0..3 {
        vcx.update(|window, cx| window.render_frame(cx));
    }
}

/// The main window over the chat-list fixture, focused as `main.rs` does.
fn new_app(cx: &mut TestAppContext, layout: LayoutStyle) -> (Entity<QuillApp>, VisualTestContext) {
    quill::settings::use_isolated_app_root(quill::scratch::process_dir("quill-app-shortcuts"));
    cx.update(gpui_kit::init);
    cx.update(bind_keys);
    crate::ui::interface_zoom::set_initial_zoom(1.);
    let slot = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let demo = ScreenshotDemo::named("ready-chats");
    let handle = cx.open_window(size(px(1200.), px(800.)), move |window, cx| {
        let view = cx.new(|cx| QuillApp::new_with_demo(window, cx, None, Some(demo)));
        *slot_in.borrow_mut() = Some(view.clone());
        let focus = view.focus_handle(cx);
        window.focus(&focus, cx);
        let shell = cx.new(|_| QuillShell::new(view));
        Root::new(shell, window, cx)
    });
    let app: Entity<QuillApp> = slot.borrow().clone().expect("app");
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    app.update(&mut vcx, |app, cx| {
        app.set_appearance(cx, |appearance| appearance.layout = layout);
    });
    frames(&mut vcx);
    (app, vcx)
}

/// The app-wide shortcuts: Quill's bindings with no key context.
fn app_wide_actions(vcx: &mut VisualTestContext) -> Vec<Box<dyn Action>> {
    vcx.update(|_, cx| {
        let keymap = cx.key_bindings();
        let keymap = keymap.borrow();
        let mut actions: Vec<Box<dyn Action>> = Vec::new();
        for binding in keymap.bindings() {
            let name = binding.action().name();
            if binding.predicate().is_none()
                && name.starts_with("quill_ui::")
                && !actions.iter().any(|action| action.name() == name)
            {
                actions.push(binding.action().boxed_clone());
            }
        }
        actions
    })
}

/// The app-wide actions the focused element's path reaches.
fn reachable(vcx: &mut VisualTestContext, actions: &[Box<dyn Action>]) -> Vec<&'static str> {
    vcx.update(|window, cx| {
        actions
            .iter()
            .filter(|action| window.is_action_available(action.as_ref(), cx))
            .map(|action| action.name())
            .collect()
    })
}

fn settings_open(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> bool {
    app.read_with(vcx, |app, _| app.settings.open())
}

fn first_chat(app: &Entity<QuillApp>, vcx: &mut VisualTestContext) -> quill::ids::ChatId {
    app.update_in(vcx, |app, _, _| {
        let session = app.demo_session.as_ref().expect("demo session");
        quill::ids::ChatId(*session.chats.keys().min().expect("a chat"))
    })
}

/// ⌘, opens Settings, a click lands inside it (its page takes the
/// keyboard), the back arrow closes it: every shortcut still reaches its
/// action, and ⌘, opens Settings again. Returns the shortcuts reached.
fn settings_closed_after_a_click(
    layout: LayoutStyle,
    cx: &mut TestAppContext,
) -> Vec<&'static str> {
    let (app, mut vcx) = new_app(cx, layout);
    let vcx = &mut vcx;
    let actions = app_wide_actions(vcx);
    let before = reachable(vcx, &actions);
    assert!(before.contains(&"quill_ui::OpenSettings"), "{before:?}");

    vcx.simulate_keystrokes(SETTINGS_CHORD);
    frames(vcx);
    assert!(settings_open(&app, vcx));
    let page = vcx.debug_bounds("settings-window").expect("settings drawn");
    vcx.simulate_click(
        point(page.right() - px(40.), page.bottom() - px(20.)),
        Modifiers::default(),
    );
    frames(vcx);
    app.update(vcx, |app, _| app.close_settings());
    frames(vcx);

    assert_eq!(reachable(vcx, &actions), before);
    vcx.simulate_keystrokes(SETTINGS_CHORD);
    frames(vcx);
    assert!(settings_open(&app, vcx), "⌘, after Settings closed");
    before
}

#[gpui_kit::test]
fn shortcuts_survive_settings_closing_in_classic(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-shortcuts-classic");
    settings_closed_after_a_click(LayoutStyle::Classic, cx);
}

/// Every app-wide shortcut a window reaches in Classic it reaches in
/// Floating too, through the same Settings round trip.
#[gpui_kit::test]
fn floating_reaches_the_shortcuts_classic_does(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-shortcuts-floating");
    let classic = settings_closed_after_a_click(LayoutStyle::Classic, cx);
    let floating = settings_closed_after_a_click(LayoutStyle::Floating, cx);
    assert_eq!(floating, classic);
}

/// The composer has the keyboard and Settings opens from a menu, covering
/// it: Escape closes Settings, and the composer has the keyboard again.
#[gpui_kit::test]
fn settings_over_the_composer_takes_the_keyboard(cx: &mut TestAppContext) {
    let _prefs = own_appearance_prefs("quill-shortcuts-composer");
    let (app, mut vcx) = new_app(cx, LayoutStyle::Floating);
    let vcx = &mut vcx;
    let chat = first_chat(&app, vcx);
    app.update_in(vcx, |app, window, cx| {
        app.select_chat_with(chat, false, window, cx);
        app.composer.update(cx, |input, cx| input.focus(window, cx));
    });
    frames(vcx);
    app.update_in(vcx, |app, window, cx| {
        app.navigate(
            crate::ui::navigation::NavigationAction::Settings,
            window,
            cx,
        );
    });
    frames(vcx);

    vcx.simulate_keystrokes("escape");
    frames(vcx);
    assert!(!settings_open(&app, vcx), "Escape closes Settings");
    let composer_focused = vcx.update(|window, cx| {
        app.read(cx)
            .composer
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    });
    assert!(composer_focused);
}
