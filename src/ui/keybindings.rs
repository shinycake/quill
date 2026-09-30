use super::actions::{
    CancelSearch, ChatSearchNewer, ChatSearchOlder, CloseWindow, FocusComposer, FocusSidebar,
    FormatBold, FormatItalic, FormatUnderline, LoadOlder, MinimizeWindow, OpenChatSearch, OpenHelp,
    OpenSearch, QuitApp, ToggleFullscreen, ToggleTheme, ViewerNext, ViewerPrev, ViewerZoomIn,
    ViewerZoomOut, ViewerZoomReset, ZoomWindow,
};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::settings::CustomKeybinding;

/// Parity slice (platform-custom-keybindings): a user-rebindable action.
pub struct RebindableAction {
    /// Stable id used in persisted prefs (e.g. "focus-composer").
    pub id: &'static str,
    /// Display label in the shortcuts settings section.
    pub label: &'static str,
    /// Default keystrokes (all platforms).
    pub defaults: &'static [&'static str],
}

/// Parity slice (platform-custom-keybindings): the actions users may rebind.
/// Window-chrome and app-lifecycle bindings (quit, close, minimize, …) stay
/// fixed — rebinding those risks stranding the user.
pub const REBINDABLE_ACTIONS: &[RebindableAction] = &[
    RebindableAction {
        id: "focus-sidebar",
        label: "Focus chat list",
        defaults: &["cmd-1", "ctrl-1"],
    },
    RebindableAction {
        id: "focus-composer",
        label: "Focus composer",
        defaults: &["cmd-l", "ctrl-l"],
    },
    RebindableAction {
        id: "load-older",
        label: "Load older messages",
        defaults: &["cmd-up", "ctrl-up"],
    },
    RebindableAction {
        id: "open-search",
        label: "Search",
        defaults: &["cmd-k", "ctrl-k"],
    },
    RebindableAction {
        id: "open-chat-search",
        label: "Search in chat",
        defaults: &["cmd-f", "ctrl-f"],
    },
    RebindableAction {
        id: "chat-search-newer",
        label: "Next search result",
        defaults: &["cmd-g", "ctrl-g"],
    },
    RebindableAction {
        id: "chat-search-older",
        label: "Previous search result",
        defaults: &["cmd-shift-g", "ctrl-shift-g"],
    },
    RebindableAction {
        id: "cancel-search",
        label: "Cancel search",
        defaults: &["escape"],
    },
    RebindableAction {
        id: "format-bold",
        label: "Bold",
        defaults: &["ctrl-b"],
    },
    RebindableAction {
        id: "format-italic",
        label: "Italic",
        defaults: &["ctrl-i"],
    },
    RebindableAction {
        id: "format-underline",
        label: "Underline",
        defaults: &["ctrl-u"],
    },
    RebindableAction {
        id: "viewer-prev",
        label: "Previous media",
        defaults: &["left"],
    },
    RebindableAction {
        id: "viewer-next",
        label: "Next media",
        defaults: &["right"],
    },
    RebindableAction {
        id: "viewer-zoom-reset",
        label: "Reset zoom",
        defaults: &["0"],
    },
    RebindableAction {
        id: "viewer-zoom-in",
        label: "Zoom in",
        defaults: &["="],
    },
    RebindableAction {
        id: "viewer-zoom-out",
        label: "Zoom out",
        defaults: &["-"],
    },
];

/// Parity slice (platform-custom-keybindings): build the key binding for a
/// rebindable action id and keystroke string. Returns `None` for unknown ids
/// or unparsable keystrokes (a bad saved pref never breaks startup).
pub fn keybinding_for(id: &str, keystroke: &str) -> Option<KeyBinding> {
    // `KeyBinding::new` unwraps the keystroke parse — validate first so a bad
    // saved pref can never panic startup.
    if Keystroke::parse(keystroke).is_err() {
        return None;
    }
    match id {
        "focus-sidebar" => Some(KeyBinding::new(keystroke, FocusSidebar, None)),
        "focus-composer" => Some(KeyBinding::new(keystroke, FocusComposer, None)),
        "load-older" => Some(KeyBinding::new(keystroke, LoadOlder, None)),
        "open-search" => Some(KeyBinding::new(keystroke, OpenSearch, None)),
        "open-chat-search" => Some(KeyBinding::new(keystroke, OpenChatSearch, None)),
        "chat-search-newer" => Some(KeyBinding::new(keystroke, ChatSearchNewer, None)),
        "chat-search-older" => Some(KeyBinding::new(keystroke, ChatSearchOlder, None)),
        "cancel-search" => Some(KeyBinding::new(keystroke, CancelSearch, None)),
        "format-bold" => Some(KeyBinding::new(keystroke, FormatBold, None)),
        "format-italic" => Some(KeyBinding::new(keystroke, FormatItalic, None)),
        "format-underline" => Some(KeyBinding::new(keystroke, FormatUnderline, None)),
        "viewer-prev" => Some(KeyBinding::new(keystroke, ViewerPrev, None)),
        "viewer-next" => Some(KeyBinding::new(keystroke, ViewerNext, None)),
        "viewer-zoom-reset" => Some(KeyBinding::new(keystroke, ViewerZoomReset, None)),
        "viewer-zoom-in" => Some(KeyBinding::new(keystroke, ViewerZoomIn, None)),
        "viewer-zoom-out" => Some(KeyBinding::new(keystroke, ViewerZoomOut, None)),
        _ => None,
    }
}

/// Fixed bindings: window chrome and app lifecycle — not rebindable.
fn fixed_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-q", QuitApp, None),
        KeyBinding::new("ctrl-q", QuitApp, None),
        // kit Phase 7: window-chrome shortcuts (HIG: Cmd+W close, Cmd+M
        // minimize; F11 / Cmd+Ctrl+F fullscreen).
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("ctrl-w", CloseWindow, None),
        KeyBinding::new("cmd-m", MinimizeWindow, None),
        KeyBinding::new("ctrl-m", MinimizeWindow, None),
        KeyBinding::new("f11", ToggleFullscreen, None),
        KeyBinding::new("cmd-ctrl-f", ToggleFullscreen, None),
    ]
}

fn default_bindings() -> Vec<KeyBinding> {
    let mut bindings = fixed_bindings();
    for ra in REBINDABLE_ACTIONS {
        push_defaults(ra, &mut bindings);
    }
    bindings
}

/// Push a rebindable action's default key bindings.
fn push_defaults(ra: &RebindableAction, bindings: &mut Vec<KeyBinding>) {
    for default in ra.defaults {
        if let Some(kb) = keybinding_for(ra.id, default) {
            bindings.push(kb);
        }
    }
}

/// The keystrokes reserved by the fixed (window-chrome/app-lifecycle)
/// bindings — a custom binding may never take one of these.
fn fixed_keystrokes() -> Vec<Keystroke> {
    fixed_bindings()
        .iter()
        .flat_map(|b| b.keystrokes().iter().map(|k| k.inner().clone()))
        .collect()
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys(default_bindings());
}

/// Parity slice (platform-custom-keybindings): rebuild the keymap from
/// defaults with the user's overrides applied. An override replaces all
/// default keystrokes for its action. Overrides that are unparsable — or
/// that collide with a fixed (window-chrome/app-lifecycle) keystroke — fall
/// back to the action's defaults.
pub fn apply_custom_bindings(cx: &mut App, customs: &[CustomKeybinding]) {
    // Review fix (#242 defect 1): `clear_key_bindings` wipes the whole app
    // keymap, including kit-internal bindings (List / command-palette
    // navigation) registered by `gpui_kit::init`. Keep everything that isn't
    // Quill-managed — Quill's own set is rebuilt below. Every Quill action
    // lives in the `quill_ui` actions! namespace (src/ui/actions.rs).
    let mut bindings: Vec<KeyBinding> = cx
        .key_bindings()
        .borrow()
        .bindings()
        .filter(|kb| !kb.action().name().starts_with("quill_ui::"))
        .cloned()
        .collect();

    let fixed = fixed_bindings();
    // Review fix (#242 defect 3): fixed bindings are added first and GPUI
    // resolves same-keystroke ties later-added-wins, so a custom keystroke
    // on cmd-q/cmd-w/cmd-m/f11 would silently disable Quit — reject it and
    // fall back to defaults, exactly like unparsable input.
    let fixed_keys = fixed_keystrokes();
    bindings.extend(fixed);
    for ra in REBINDABLE_ACTIONS {
        let custom = customs
            .iter()
            .find(|c| c.id == ra.id)
            .and_then(|c| keybinding_for(ra.id, &c.keystroke))
            .filter(|kb| {
                !kb.keystrokes()
                    .iter()
                    .any(|k| fixed_keys.contains(k.inner()))
            });
        match custom {
            Some(kb) => bindings.push(kb),
            None => push_defaults(ra, &mut bindings),
        }
    }
    cx.clear_key_bindings();
    cx.bind_keys(bindings);
}

/// kit Phase 7: the application menus — File / Edit / View / Window / Help,
/// every item wired to a working action. `setup_app_menus` installs them
/// twice from this one definition: `cx.set_menus` drives the native menu bar
/// on macOS, and `GlobalState::set_app_menus` feeds kit's `AppMenuBar`,
/// rendered in-window on Linux/Windows.
fn app_menus() -> Vec<Menu> {
    // Local aliases: `Copy` would shadow the derive macro's `Copy` at
    // module scope.
    use gpui_kit::component::input::{
        Copy as CopyAction, Cut as CutAction, Paste as PasteAction, Redo as RedoAction,
        SelectAll as SelectAllAction, Undo as UndoAction,
    };
    let file_items = {
        let mut items = vec![MenuItem::action("Close Window", CloseWindow)];
        // HIG: on macOS Quit lives in the app menu, not File.
        #[cfg(not(target_os = "macos"))]
        {
            items.push(MenuItem::separator());
            items.push(MenuItem::action("Quit Quill", QuitApp));
        }
        items
    };
    let mut menus = Vec::new();
    // HIG: on macOS Quit lives in the app menu, not File.
    #[cfg(target_os = "macos")]
    menus.push(Menu::new("Quill").items([MenuItem::action("Quit Quill", QuitApp)]));
    menus.extend([
        Menu::new("File").items(file_items),
        Menu::new("Edit").items([
            MenuItem::action("Undo", UndoAction),
            MenuItem::action("Redo", RedoAction),
            MenuItem::separator(),
            MenuItem::os_action("Cut", CutAction, OsAction::Cut),
            MenuItem::os_action("Copy", CopyAction, OsAction::Copy),
            MenuItem::os_action("Paste", PasteAction, OsAction::Paste),
            MenuItem::separator(),
            MenuItem::os_action("Select All", SelectAllAction, OsAction::SelectAll),
        ]),
        Menu::new("View").items([
            MenuItem::action("Quick Switch", OpenSearch),
            MenuItem::action("Find in Chat", OpenChatSearch),
            MenuItem::separator(),
            MenuItem::action("Enter Full Screen", ToggleFullscreen),
            // kit Phase 8: light/dark switch for the whole app.
            MenuItem::action("Toggle Theme", ToggleTheme),
        ]),
        Menu::new("Window").items([
            MenuItem::action("Minimize", MinimizeWindow),
            MenuItem::action("Zoom", ZoomWindow),
        ]),
        Menu::new("Help").items([MenuItem::action("Quill on GitHub", OpenHelp)]),
    ]);
    menus
}

/// kit Phase 7: install the app menus — native on macOS, kit `AppMenuBar`
/// data on Linux/Windows. Call once after `gpui_kit::init` + `bind_keys`.
pub fn setup_app_menus(cx: &mut App) {
    let owned: Vec<OwnedMenu> = app_menus().into_iter().map(Menu::owned).collect();
    cx.set_menus(app_menus());
    GlobalState::global_mut(cx).set_app_menus(owned);
}

#[cfg(test)]
mod tests {
    // NOTE: explicit imports, not `use super::*` — `gpui_kit::*` re-exports
    // gpui's `#[test]` proc macro, which would shadow the builtin test
    // attribute and fail macro expansion ("recursion limit reached").
    use super::{Action, Keystroke, QuitApp, REBINDABLE_ACTIONS, fixed_keystrokes, keybinding_for};

    #[test]
    fn keybinding_for_valid_id_and_keystroke() {
        assert!(keybinding_for("focus-composer", "ctrl-l").is_some());
        assert!(keybinding_for("cancel-search", "escape").is_some());
    }

    #[test]
    fn keybinding_for_unknown_id_is_none() {
        assert!(keybinding_for("nope", "ctrl-l").is_none());
    }

    #[test]
    fn keybinding_for_invalid_keystroke_is_none() {
        // Must not panic (KeyBinding::new unwraps the parse).
        assert!(keybinding_for("focus-composer", "not-a-keystroke-%%%").is_none());
        assert!(keybinding_for("focus-composer", "").is_none());
    }

    #[test]
    fn rebindable_ids_are_unique() {
        let mut ids: Vec<&str> = REBINDABLE_ACTIONS.iter().map(|ra| ra.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), REBINDABLE_ACTIONS.len());
    }

    #[test]
    fn quill_action_names_match_preserve_filter() {
        // apply_custom_bindings keeps bindings whose action name does NOT
        // start with this prefix; if the actions! namespace ever changes,
        // the filter must change with it.
        assert!(QuitApp.name().starts_with("quill_ui::"));
    }

    #[test]
    fn custom_keystroke_on_fixed_keystroke_is_rejected() {
        // Mirrors apply_custom_bindings' collision rule: a custom "cmd-q"
        // for Search must not shadow the fixed Quit binding.
        let fixed = fixed_keystrokes();
        let kb = keybinding_for("open-search", "cmd-q").unwrap();
        assert!(kb.keystrokes().iter().any(|k| fixed.contains(k.inner())));
        let ok = keybinding_for("open-search", "ctrl-k").unwrap();
        assert!(!ok.keystrokes().iter().any(|k| fixed.contains(k.inner())));
    }
}
