use super::actions::{
    CancelSearch, ChatSearchNewer, ChatSearchOlder, CloseWindow, FocusComposer, FocusSidebar,
    FormatBold, FormatItalic, FormatUnderline, LoadOlder, MinimizeWindow, OpenChatSearch, OpenHelp,
    OpenSearch, OpenShortcuts, QuitApp, ToggleFullscreen, ToggleTheme, ViewerNext, ViewerPrev,
    ViewerZoomIn, ViewerZoomOut, ViewerZoomReset, ZoomWindow,
};
use gpui_kit::component::*;
use gpui_kit::*;
use quill::settings::CustomKeybinding;

/// Dismissing Appearance must also release ownership of composer keystrokes.
pub(super) fn close_appearance_capture(
    appearance_open: &mut bool,
    capture: &mut Option<String>,
    error: &mut Option<(String, String)>,
) {
    *appearance_open = false;
    *capture = None;
    *error = None;
}

/// Also discard stale capture state if a dialog was dismissed elsewhere.
pub(super) fn capture_active(
    appearance_open: bool,
    capture: &mut Option<String>,
    error: &mut Option<(String, String)>,
) -> bool {
    if !appearance_open {
        *capture = None;
        *error = None;
    }
    appearance_open && capture.is_some()
}

/// A new prefs root needs its own keymap, even when it has no overrides.
pub(super) fn invalidate_account_keybindings(
    applied: &mut bool,
    capture: &mut Option<String>,
    error: &mut Option<(String, String)>,
) {
    *applied = false;
    *capture = None;
    *error = None;
}

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
    // saved pref can never panic startup. An empty string parses as a blank
    // key, which is not a usable chord.
    let Ok(parsed) = Keystroke::parse(keystroke) else {
        return None;
    };
    if parsed.key.is_empty() {
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
    shortcut_rows().into_iter().map(|row| row.binding).collect()
}

/// The keystrokes reserved by the fixed (window-chrome/app-lifecycle)
/// bindings — a custom binding may never take one of these.
fn fixed_keystrokes() -> Vec<Keystroke> {
    fixed_bindings()
        .iter()
        .flat_map(|b| b.keystrokes().iter().map(|k| k.inner().clone()))
        .collect()
}

/// Why a chord cannot become the live shortcut for an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeybindingConflict {
    /// The string does not parse as a keystroke.
    Invalid,
    /// Reserved by quit, close, minimize, or fullscreen.
    FixedChrome,
    /// Already the live chord of another rebindable action.
    Rebindable { other_label: &'static str },
}

/// One rebindable action after saved overrides are filtered.
///
/// `live` is what the keymap will actually install. `rejected` is a saved
/// override that was not installed — the UI must not present that chord as
/// the active shortcut.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedKeybinding {
    pub id: &'static str,
    pub label: &'static str,
    pub live: Vec<String>,
    pub rejected: Option<KeybindingConflict>,
}

/// User-facing explanation. The chord named here is the one that was
/// refused, not the chord that remains active.
pub fn conflict_message(chord: &str, conflict: &KeybindingConflict) -> String {
    match conflict {
        KeybindingConflict::Invalid => {
            format!("{chord} isn't a valid shortcut and was not applied.")
        }
        KeybindingConflict::FixedChrome => format!(
            "{chord} is reserved for quit, close, minimize, or fullscreen and was not applied."
        ),
        KeybindingConflict::Rebindable { other_label } => {
            format!("{chord} is already used by {other_label} and was not applied.")
        }
    }
}

/// Whether `chord` may be saved as the shortcut for `action_id`.
///
/// Compares against fixed chrome and against the chords
/// [`resolve_keybindings`] would actually install for every other action.
/// The action's own current chord is not a conflict — rebinding replaces it.
pub fn keybinding_conflict(
    action_id: &str,
    chord: &str,
    customs: &[CustomKeybinding],
) -> Option<KeybindingConflict> {
    let Some(parsed) = parse_chord(chord) else {
        return Some(KeybindingConflict::Invalid);
    };
    if chord_taken(&fixed_keystrokes(), &parsed) {
        return Some(KeybindingConflict::FixedChrome);
    }
    for row in resolve_keybindings(customs) {
        if row.id == action_id {
            continue;
        }
        if row
            .live
            .iter()
            .any(|live| parse_chord(live).is_some_and(|other| same_chord(&other, &parsed)))
        {
            return Some(KeybindingConflict::Rebindable {
                other_label: row.label,
            });
        }
    }
    None
}

/// Resolve saved overrides into the chords that should be bound.
///
/// Fixed chrome is claimed first. Each rebindable action then takes its
/// saved chord only when that chord is still free; otherwise the save is
/// rejected and the action keeps whichever of its defaults are still free.
/// A chord is never installed twice, so two actions cannot both go live on
/// the same keystroke.
pub fn resolve_keybindings(customs: &[CustomKeybinding]) -> Vec<ResolvedKeybinding> {
    let fixed = fixed_keystrokes();
    let mut claimed = fixed.clone();
    let mut resolved = Vec::with_capacity(REBINDABLE_ACTIONS.len());
    for ra in REBINDABLE_ACTIONS {
        let saved = customs
            .iter()
            .find(|custom| custom.id == ra.id && !custom.keystroke.is_empty());
        if let Some(saved) = saved {
            if let Some(ks) = parse_chord(&saved.keystroke) {
                if !chord_taken(&claimed, &ks) {
                    let label = canonical_chord(&ks);
                    claimed.push(ks);
                    resolved.push(ResolvedKeybinding {
                        id: ra.id,
                        label: ra.label,
                        live: vec![label],
                        rejected: None,
                    });
                    continue;
                }
                let rejected = if chord_taken(&fixed, &ks) {
                    KeybindingConflict::FixedChrome
                } else {
                    KeybindingConflict::Rebindable {
                        other_label: owner_label(&resolved, &ks).unwrap_or("another shortcut"),
                    }
                };
                resolved.push(ResolvedKeybinding {
                    id: ra.id,
                    label: ra.label,
                    live: claim_defaults(ra, &mut claimed),
                    rejected: Some(rejected),
                });
                continue;
            }
            resolved.push(ResolvedKeybinding {
                id: ra.id,
                label: ra.label,
                live: claim_defaults(ra, &mut claimed),
                rejected: Some(KeybindingConflict::Invalid),
            });
            continue;
        }
        resolved.push(ResolvedKeybinding {
            id: ra.id,
            label: ra.label,
            live: claim_defaults(ra, &mut claimed),
            rejected: None,
        });
    }
    resolved
}

/// Modifier keys are the start of a chord, not a finished shortcut.
pub fn is_modifier_key(key: &str) -> bool {
    matches!(
        key,
        "control"
            | "ctrl"
            | "alt"
            | "option"
            | "shift"
            | "platform"
            | "cmd"
            | "super"
            | "win"
            | "meta"
            | "function"
            | "fn"
    )
}

/// Parseable chord for a captured key event (`"ctrl-k"`, `"cmd-shift-g"`).
///
/// `Keystroke`'s `Display` is the on-screen form (`ctrl-K`, or symbols on
/// macOS) and does not round-trip through `Keystroke::parse`. Returns
/// `None` for bare modifiers so holding Ctrl does not end capture.
pub fn canonical_event_chord(keystroke: &Keystroke) -> Option<String> {
    if keystroke.key.is_empty() || is_modifier_key(&keystroke.key) {
        return None;
    }
    let normalized = Keystroke {
        modifiers: keystroke.modifiers,
        key: keystroke.key.to_ascii_lowercase(),
        key_char: None,
    };
    let chord = canonical_chord(&normalized);
    parse_chord(&chord).map(|_| chord)
}

fn canonical_chord(ks: &Keystroke) -> String {
    let mut result = String::new();
    if ks.modifiers.function {
        result.push_str("fn-");
    }
    if ks.modifiers.control {
        result.push_str("ctrl-");
    }
    if ks.modifiers.alt {
        result.push_str("alt-");
    }
    if ks.modifiers.platform {
        result.push_str("cmd-");
    }
    if ks.modifiers.shift {
        result.push_str("shift-");
    }
    result.push_str(&ks.key);
    result
}

fn parse_chord(source: &str) -> Option<Keystroke> {
    let ks = Keystroke::parse(source).ok()?;
    if ks.key.is_empty() {
        return None;
    }
    Some(Keystroke {
        modifiers: ks.modifiers,
        key: ks.key,
        key_char: None,
    })
}

fn same_chord(a: &Keystroke, b: &Keystroke) -> bool {
    a.modifiers == b.modifiers && a.key.eq_ignore_ascii_case(&b.key)
}

fn chord_taken(claimed: &[Keystroke], ks: &Keystroke) -> bool {
    claimed.iter().any(|claimed| same_chord(claimed, ks))
}

fn owner_label(resolved: &[ResolvedKeybinding], ks: &Keystroke) -> Option<&'static str> {
    resolved.iter().find_map(|row| {
        row.live.iter().find_map(|live| {
            parse_chord(live)
                .filter(|other| same_chord(other, ks))
                .map(|_| row.label)
        })
    })
}

fn claim_defaults(ra: &RebindableAction, claimed: &mut Vec<Keystroke>) -> Vec<String> {
    let mut live = Vec::new();
    for default in ra.defaults {
        let Some(ks) = parse_chord(default) else {
            continue;
        };
        if chord_taken(claimed, &ks) {
            continue;
        }
        claimed.push(ks);
        live.push((*default).to_string());
    }
    live
}

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys(default_bindings());
}

/// Parity slice (platform-custom-keybindings): rebuild the keymap from
/// defaults with the user's overrides applied. An override replaces all
/// default keystrokes for its action. Overrides that are unparsable, that
/// collide with a fixed (window-chrome/app-lifecycle) keystroke, or that
/// collide with another rebindable action's live chord are not installed —
/// that action keeps the defaults that are still free. A chord is bound to
/// at most one action.
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

    // Fixed bindings are installed and claimed first. GPUI resolves
    // same-keystroke ties later-added-wins, so a custom chord on
    // cmd-q/cmd-w/cmd-m/f11 — or two rebindable actions on one chord —
    // must never be added. `resolve_keybindings` drops those.
    bindings.extend(fixed_bindings());
    for row in resolve_keybindings(customs) {
        for chord in row.live {
            if let Some(kb) = keybinding_for(row.id, &chord) {
                bindings.push(kb);
            }
        }
    }
    cx.clear_key_bindings();
    cx.bind_keys(bindings);
}

/// One row of the app's keyboard-shortcut reference: the single source of
/// truth for both `bind_keys` (what the app listens for) and the shortcuts
/// reference dialog (what the user sees). `keystroke` is gpui format
/// (e.g. `"cmd-q"`); the dialog renders it through kit's `Kbd`.
pub struct ShortcutRow {
    pub label: &'static str,
    pub section: &'static str,
    pub binding: KeyBinding,
}

fn row<A: Action>(
    keystroke: &'static str,
    label: &'static str,
    section: &'static str,
    action: A,
) -> ShortcutRow {
    ShortcutRow {
        label,
        section,
        binding: KeyBinding::new(keystroke, action, None),
    }
}

/// Every keybinding the app registers, in reference-dialog order. Rows that
/// share a label (e.g. `cmd-q` / `ctrl-q`) render as one entry with both
/// chips.
pub fn shortcut_rows() -> Vec<ShortcutRow> {
    vec![
        // General.
        row("cmd-q", "Quit Quill", "General", QuitApp),
        row("ctrl-q", "Quit Quill", "General", QuitApp),
        // kit Phase 7: window-chrome shortcuts (HIG: Cmd+W close, Cmd+M
        // minimize; F11 / Cmd+Ctrl+F fullscreen).
        row("cmd-w", "Close window", "General", CloseWindow),
        row("ctrl-w", "Close window", "General", CloseWindow),
        row("cmd-m", "Minimize window", "General", MinimizeWindow),
        row("ctrl-m", "Minimize window", "General", MinimizeWindow),
        row("f11", "Toggle fullscreen", "General", ToggleFullscreen),
        row(
            "cmd-ctrl-f",
            "Toggle fullscreen",
            "General",
            ToggleFullscreen,
        ),
        // Navigation.
        row("cmd-1", "Focus chat list", "Navigation", FocusSidebar),
        row("ctrl-1", "Focus chat list", "Navigation", FocusSidebar),
        row(
            "cmd-l",
            "Focus message composer",
            "Navigation",
            FocusComposer,
        ),
        row(
            "ctrl-l",
            "Focus message composer",
            "Navigation",
            FocusComposer,
        ),
        row("cmd-up", "Load older messages", "Navigation", LoadOlder),
        row("ctrl-up", "Load older messages", "Navigation", LoadOlder),
        // Search.
        row("cmd-k", "Quick switch chats", "Search", OpenSearch),
        row("ctrl-k", "Quick switch chats", "Search", OpenSearch),
        row("cmd-f", "Find in chat", "Search", OpenChatSearch),
        row("ctrl-f", "Find in chat", "Search", OpenChatSearch),
        row("cmd-g", "Next search result", "Search", ChatSearchNewer),
        row("ctrl-g", "Next search result", "Search", ChatSearchNewer),
        row(
            "cmd-shift-g",
            "Previous search result",
            "Search",
            ChatSearchOlder,
        ),
        row(
            "ctrl-shift-g",
            "Previous search result",
            "Search",
            ChatSearchOlder,
        ),
        row("escape", "Close search / cancel", "Search", CancelSearch),
        // Parity slice 5: the handlers no-op (and let the keystroke reach
        // text inputs) unless the media viewer is open.
        row("left", "Previous item", "Media viewer", ViewerPrev),
        row("right", "Next item", "Media viewer", ViewerNext),
        row("0", "Reset zoom", "Media viewer", ViewerZoomReset),
        row("=", "Zoom in", "Media viewer", ViewerZoomIn),
        row("-", "Zoom out", "Media viewer", ViewerZoomOut),
        // M1: composer formatting shortcuts; the handlers no-op unless
        // the composer textarea has focus.
        row("ctrl-b", "Bold", "Composer", FormatBold),
        row("ctrl-i", "Italic", "Composer", FormatItalic),
        row("ctrl-u", "Underline", "Composer", FormatUnderline),
    ]
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
        Menu::new("Help").items([
            MenuItem::action("Keyboard Shortcuts", OpenShortcuts),
            MenuItem::action("Quill on GitHub", OpenHelp),
        ]),
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
    use super::{
        Action, KeybindingConflict, Keystroke, Modifiers, QuitApp, REBINDABLE_ACTIONS,
        canonical_event_chord, capture_active, close_appearance_capture, conflict_message,
        default_bindings, fixed_keystrokes, invalidate_account_keybindings, keybinding_conflict,
        keybinding_for, resolve_keybindings, same_chord,
    };
    use quill::settings::CustomKeybinding;

    fn custom(id: &str, keystroke: &str) -> CustomKeybinding {
        CustomKeybinding {
            id: id.to_string(),
            keystroke: keystroke.to_string(),
        }
    }

    #[test]
    fn reference_table_matches_resolved_defaults() {
        let defaults = default_bindings();
        assert_eq!(defaults.len(), 31);
        for row in resolve_keybindings(&[]) {
            for chord in row.live {
                let binding = keybinding_for(row.id, &chord).unwrap();
                assert!(defaults.iter().any(|default| default.action().name()
                    == binding.action().name()
                    && same_chord(
                        default.keystrokes()[0].inner(),
                        binding.keystrokes()[0].inner()
                    )));
            }
        }
    }

    #[test]
    fn appearance_dismiss_clears_capture_and_error() {
        let mut open = true;
        let mut capture = Some("focus-composer".to_string());
        let mut error = Some(("focus-composer".to_string(), "Conflict".to_string()));
        close_appearance_capture(&mut open, &mut capture, &mut error);
        assert!(!open);
        assert_eq!(capture, None);
        assert_eq!(error, None);
        assert!(!capture_active(open, &mut capture, &mut error));
    }

    #[test]
    fn capture_only_consumes_keys_while_appearance_is_open_and_armed() {
        let mut capture = None;
        let mut error = None;
        assert!(!capture_active(true, &mut capture, &mut error));
        capture = Some("focus-composer".to_string());
        assert!(capture_active(true, &mut capture, &mut error));
        error = Some(("focus-composer".to_string(), "Conflict".to_string()));
        assert!(!capture_active(false, &mut capture, &mut error));
        assert_eq!(capture, None);
        assert_eq!(error, None);
    }

    #[test]
    fn account_change_invalidates_applied_bindings_and_capture() {
        let mut applied = true;
        let mut capture = Some("focus-composer".to_string());
        let mut error = Some(("focus-composer".to_string(), "Conflict".to_string()));
        invalidate_account_keybindings(&mut applied, &mut capture, &mut error);
        assert!(!applied);
        assert_eq!(capture, None);
        assert_eq!(error, None);
    }

    #[test]
    fn account_overrides_and_empty_prefs_resolve_independently() {
        let account_a = vec![custom("focus-composer", "ctrl-9")];
        let account_b = vec![];
        let account_c = vec![custom("focus-composer", "ctrl-8")];
        // Each apply rebuilds from this resolver, never from the prior keymap.
        for prefs in [&account_a, &account_b, &account_c, &account_b, &account_a] {
            let resolved = resolve_keybindings(prefs);
            if prefs.is_empty() {
                for (row, action) in resolved.iter().zip(REBINDABLE_ACTIONS) {
                    assert_eq!(row.id, action.id);
                    assert_eq!(row.live, action.defaults);
                    assert_eq!(row.rejected, None);
                }
                assert!(!resolved.iter().any(|row| {
                    row.live
                        .iter()
                        .any(|chord| chord == "ctrl-9" || chord == "ctrl-8")
                }));
            } else {
                let focus = resolved
                    .iter()
                    .find(|row| row.id == "focus-composer")
                    .unwrap();
                assert_eq!(focus.live, vec![prefs[0].keystroke.clone()]);
                assert_eq!(focus.rejected, None);
            }
        }
    }

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
        for reserved in [
            "cmd-q",
            "ctrl-q",
            "cmd-w",
            "ctrl-w",
            "cmd-m",
            "ctrl-m",
            "f11",
            "cmd-ctrl-f",
        ] {
            assert_eq!(
                keybinding_conflict("open-search", reserved, &[]),
                Some(KeybindingConflict::FixedChrome),
                "{reserved}"
            );
        }
    }

    #[test]
    fn rebindable_conflict_blocks_a_shared_live_chord() {
        // ctrl-l is still Focus composer's live default.
        assert_eq!(
            keybinding_conflict("open-search", "ctrl-l", &[]),
            Some(KeybindingConflict::Rebindable {
                other_label: "Focus composer",
            })
        );
        // Moving Focus composer off ctrl-l frees it. The new chord is taken.
        let moved = vec![custom("focus-composer", "ctrl-9")];
        assert_eq!(keybinding_conflict("open-search", "ctrl-l", &moved), None);
        assert_eq!(
            keybinding_conflict("open-search", "ctrl-9", &moved),
            Some(KeybindingConflict::Rebindable {
                other_label: "Focus composer",
            })
        );
        // An action may keep its own default.
        assert_eq!(keybinding_conflict("focus-composer", "ctrl-l", &[]), None);
        assert_eq!(
            keybinding_conflict("open-search", "not-a-keystroke-%%%", &[]),
            Some(KeybindingConflict::Invalid)
        );
    }

    #[test]
    fn rejected_chords_are_not_reported_as_live() {
        let customs = vec![
            custom("focus-composer", "ctrl-k"),
            custom("open-search", "cmd-q"),
            custom("format-bold", "ctrl-k"),
        ];
        let resolved = resolve_keybindings(&customs);
        let focus = resolved
            .iter()
            .find(|row| row.id == "focus-composer")
            .unwrap();
        assert_eq!(focus.live, vec!["ctrl-k".to_string()]);
        assert_eq!(focus.rejected, None);

        let search = resolved.iter().find(|row| row.id == "open-search").unwrap();
        assert_eq!(search.rejected, Some(KeybindingConflict::FixedChrome));
        assert_eq!(search.live, vec!["cmd-k".to_string()]);
        assert!(!search.live.iter().any(|chord| chord == "cmd-q"));

        let bold = resolved.iter().find(|row| row.id == "format-bold").unwrap();
        assert_eq!(
            bold.rejected,
            Some(KeybindingConflict::Rebindable {
                other_label: "Focus composer",
            })
        );
        assert_eq!(bold.live, vec!["ctrl-b".to_string()]);
        assert!(!bold.live.iter().any(|chord| chord == "ctrl-k"));

        let message = conflict_message("cmd-q", &KeybindingConflict::FixedChrome);
        assert!(message.contains("cmd-q"));
        assert!(message.contains("not applied"));
    }

    #[test]
    fn resolved_live_chords_do_not_overlap() {
        let customs = vec![
            custom("open-search", "cmd-q"),
            custom("focus-composer", "ctrl-k"),
            custom("format-bold", "ctrl-k"),
            custom("format-italic", "ctrl-b"),
        ];
        let resolved = resolve_keybindings(&customs);
        let mut seen: Vec<Keystroke> = fixed_keystrokes();
        for row in &resolved {
            for chord in &row.live {
                let ks = Keystroke::parse(chord).unwrap();
                assert!(
                    !seen.iter().any(|claimed| {
                        claimed.modifiers == ks.modifiers
                            && claimed.key.eq_ignore_ascii_case(&ks.key)
                    }),
                    "{} reuses {chord}",
                    row.id
                );
                seen.push(ks);
            }
        }
        let italic = resolved
            .iter()
            .find(|row| row.id == "format-italic")
            .unwrap();
        assert!(matches!(
            italic.rejected,
            Some(KeybindingConflict::Rebindable { .. })
        ));
        assert_eq!(italic.live, vec!["ctrl-i".to_string()]);
    }

    #[test]
    fn captured_event_chord_round_trips_and_matches_fixed_chrome() {
        let ks = Keystroke {
            modifiers: Modifiers {
                control: true,
                ..Modifiers::none()
            },
            key: "q".into(),
            key_char: Some("q".into()),
        };
        assert_eq!(canonical_event_chord(&ks).as_deref(), Some("ctrl-q"));
        assert!(
            canonical_event_chord(&Keystroke {
                modifiers: Modifiers::none(),
                key: "control".into(),
                key_char: None,
            })
            .is_none()
        );
    }
}
