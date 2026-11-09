//! The lock as a render state: while Quill is locked, every window draws
//! the lock screen or nothing (docs/decisions/lock-is-a-render-state.md).
//!
//! The kit's `Root` builds each window's surface: the application view and,
//! above it, every presentation plugin's overlay (the kit's dialog, sheet,
//! notification, tooltip and menu layers). [`LockLayer`] is a plugin
//! registered after the kit's, so its `decorate` runs last and gets the
//! whole surface. While locked it drops that surface unbuilt and returns
//! the lock screen alone. What it drops is never rendered, so no kit dialog,
//! sheet, popover, tooltip or deferred overlay is drawn or takes input,
//! whoever opened it and whenever.
//!
//! Which windows draw what is decided by the window's view type, here and
//! nowhere else. A window type not named below draws nothing while locked,
//! so a new kind of window is covered without anyone listing it.
#![deny(clippy::indexing_slicing)]

use gpui_base::{Root, RootPlugin};
use gpui_kit::component::{ActiveTheme, window_border};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::any::TypeId;

/// Whether Quill is locked, for code that has no `QuillApp` at hand (the
/// root plugin of every window). Written only by `QuillApp::set_locked`,
/// together with `PasscodeUi`'s own flag.
#[derive(Clone, Copy, Default)]
pub(crate) struct LockGate {
    locked: bool,
}

impl Global for LockGate {}

/// Whether the app is locked now.
pub(crate) fn app_locked(cx: &App) -> bool {
    cx.try_global::<LockGate>().is_some_and(|gate| gate.locked)
}

/// Record the lock state for every window and draw them all again.
pub(super) fn publish(locked: bool, cx: &mut App) {
    if app_locked(cx) == locked && cx.has_global::<LockGate>() {
        return;
    }
    cx.set_global(LockGate { locked });
    cx.refresh_windows();
}

/// What a window draws while locked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LockRole {
    /// Its own view, which draws only the lock screen while locked (the
    /// main window and the chat windows, each with its own field).
    LockScreen,
    /// Nothing: a plain themed surface.
    Blank,
    /// Everything, as unlocked. Only windows that show no chat: a call
    /// window stays usable while locked, as tdesktop leaves its call
    /// panel up (`Application::lockByPasscode` hides only the panel's
    /// layers; `closeAdditionalWindows` does not close it). A notification
    /// window shown while locked already hides its sender and text
    /// (`hide_sender`), as tdesktop's do.
    Unlocked,
}

/// The role of a window whose root holds a view of this type.
pub(crate) fn role_of(view: TypeId) -> LockRole {
    use crate::ui::calls::call_video::CallWindowRoot;
    use crate::ui::chat_window::ChatWindowView;
    use crate::ui::group_call_window::GroupCallRoot;
    use crate::ui::notify_windows::NotifyView;
    use crate::ui::screen_chooser::ScreenChooserView;
    use crate::ui::shell::QuillShell;
    if view == TypeId::of::<QuillShell>() || view == TypeId::of::<ChatWindowView>() {
        LockRole::LockScreen
    } else if view == TypeId::of::<CallWindowRoot>()
        || view == TypeId::of::<GroupCallRoot>()
        // Opened from a call window's Share Screen, which stays usable.
        || view == TypeId::of::<ScreenChooserView>()
        || view == TypeId::of::<NotifyView>()
    {
        LockRole::Unlocked
    } else {
        LockRole::Blank
    }
}

/// The root plugin. It holds nothing; its overlay is empty.
pub(crate) struct LockLayer;

impl LockLayer {
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self
    }
}

/// Register the plugin for every window opened from now on. Called while
/// the app's state is built, before the main window's root exists; a
/// second call replaces the factory and mounts nothing twice.
pub(crate) fn register(cx: &mut App) {
    Root::register_plugin::<LockLayer>(cx, LockLayer::new);
}

impl Render for LockLayer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

impl RootPlugin for LockLayer {
    fn decorate(
        &self,
        surface: AnyElement,
        root: &Root,
        _: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        if !app_locked(cx) {
            return surface;
        }
        let role = role_of(root.view().entity_type());
        if role == LockRole::Unlocked {
            return surface;
        }
        // The surface goes unbuilt: its views (the kit's layers among
        // them) do not render this frame.
        drop(surface);
        let theme = cx.theme();
        let screen = div()
            .id("lock-layer")
            .debug_selector(|| "lock-layer".to_string())
            .relative()
            .size_full()
            .font_family(theme.font_family.clone())
            .bg(theme.background)
            .text_color(theme.foreground)
            // The kit's Tab handling went with its root: the lock screen's
            // field and buttons still take Tab and Shift-Tab.
            .on_key_down(|event: &KeyDownEvent, window, cx| {
                let keystroke = &event.keystroke;
                if keystroke.key == "tab" && !keystroke.modifiers.control {
                    if keystroke.modifiers.shift {
                        window.focus_prev(cx);
                    } else {
                        window.focus_next(cx);
                    }
                    cx.stop_propagation();
                }
            })
            .when(role == LockRole::LockScreen, |this| {
                this.child(root.view().clone())
            });
        window_border().child(screen).into_any_element()
    }
}
