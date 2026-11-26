//! Every Quill window keeps the keyboard where its shortcuts reach.
//!
//! GPUI runs a key's action, and decides whether a menu item is enabled,
//! along the path from the focused element up to the window root. When the
//! focused element is no longer drawn (Settings closed after a click inside
//! it, the composer under a page that covers it), that path is the root
//! alone: the element that binds the window's actions is not on it, so ⌘,
//! ⌘L, ⌘W and the rest do nothing and the macOS menu greys their items.
//!
//! Each window draws a home, an empty element inside the one that binds its
//! actions. When GPUI reports the focus lost and the window's focus still
//! names the element that is gone, the keyboard moves to the
//! nearest focusable ancestor that is still drawn, else to the first of the
//! places the window names for what it shows (Settings, the composer, the
//! chat list) that is drawn where the actions reach, else home.

use super::actions::OpenSettings;
use gpui_kit::*;

pub(in crate::ui) struct KeyboardHome {
    handle: FocusHandle,
    /// The focus-lost hook of the native window the home is drawn in.
    /// `None` until that window's first render hands it over.
    watch: Option<Subscription>,
}

impl KeyboardHome {
    pub(in crate::ui) fn new(cx: &mut App) -> Self {
        Self {
            handle: cx.focus_handle(),
            watch: None,
        }
    }

    /// The home, drawn as a child of the element that binds the window's
    /// actions. It has no size, so a click never lands on it.
    pub(in crate::ui) fn anchor(&self) -> Div {
        div().track_focus(&self.handle)
    }

    /// Hook `window`'s focus-lost report, once per native window.
    /// `places` lists where the keyboard should go for what the owner
    /// shows, best first; one not drawn this frame is passed over.
    pub(in crate::ui) fn watch<T: 'static>(
        &mut self,
        window: &mut Window,
        cx: &mut Context<T>,
        places: fn(&T, &App) -> Vec<FocusHandle>,
    ) {
        if self.watch.is_some() {
            return;
        }
        let home = self.handle.clone();
        self.watch = Some(cx.on_focus_lost(window, move |owner, window, cx| {
            // A window blurred on purpose has nothing focused: only a focus
            // that names an element no longer drawn is moved.
            if window.focused(cx).is_none() {
                return;
            }
            let target = window
                .focus_lost_restore_target(cx)
                .or_else(|| {
                    places(owner, cx)
                        .into_iter()
                        .find(|place| reaches_actions(place, window))
                })
                .unwrap_or_else(|| home.clone());
            window.focus(&target, cx);
        }));
    }

    /// A new native window took this one's place: the next render hooks it.
    pub(in crate::ui) fn forget_window(&mut self) {
        self.watch = None;
    }
}

/// Whether `handle` is drawn this frame inside the element that binds the
/// window's actions: every Quill window root binds Settings.
pub(in crate::ui) fn reaches_actions(handle: &FocusHandle, window: &Window) -> bool {
    window.is_action_available_in(&OpenSettings, handle)
}
