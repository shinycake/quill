//! Methods moved out of `keybindings.rs` to keep files under 1000 lines.

use super::*;

/// Cmd/Ctrl+1..8: the Nth pinned chat of the list on screen
/// (`Command::ChatPinned1..8`).
pub(super) fn pinned_chat_rows() -> Vec<ShortcutRow> {
    let chords = [
        primary!("1"),
        primary!("2"),
        primary!("3"),
        primary!("4"),
        primary!("5"),
        primary!("6"),
        primary!("7"),
        primary!("8"),
    ];
    let labels = [
        "Open pinned chat 1",
        "Open pinned chat 2",
        "Open pinned chat 3",
        "Open pinned chat 4",
        "Open pinned chat 5",
        "Open pinned chat 6",
        "Open pinned chat 7",
        "Open pinned chat 8",
    ];
    chords
        .into_iter()
        .zip(labels)
        .enumerate()
        .map(|(index, (chord, label))| row(chord, label, "Navigation", OpenPinnedChat { index }))
        .collect()
}

/// The message-history shortcuts: reply navigation, attach, scrolling and
/// deleting a selection (tdesktop `HistoryWidget::keyPressEvent`,
/// `HistoryInner::keyPressEvent`).
pub(super) fn message_rows() -> Vec<ShortcutRow> {
    vec![
        row(
            primary!("up"),
            "Reply to previous message",
            "Messages",
            ReplyToPrevious,
        ),
        row(
            primary!("down"),
            "Reply to next message",
            "Messages",
            ReplyToNext,
        ),
        row(primary!("o"), "Attach file", "Messages", AttachFile),
        row("pageup", "Scroll up a page", "Messages", HistoryPageUp),
        row(
            "pagedown",
            "Scroll down a page",
            "Messages",
            HistoryPageDown,
        ),
        row(
            "home",
            "Scroll to the first message",
            "Messages",
            HistoryToTop,
        ),
        row(
            "end",
            "Scroll to the latest message",
            "Messages",
            HistoryToBottom,
        ),
        row(
            "delete",
            "Delete selected messages",
            "Messages",
            DeleteSelection,
        ),
        row(
            "backspace",
            "Delete selected messages",
            "Messages",
            DeleteSelection,
        ),
        row(
            primary!("shift-a"),
            "Select the focused message",
            "Messages",
            ToggleMessageSelection,
        ),
        row(
            "up",
            "Focus the older message while selecting",
            "Messages",
            SelectionFocusOlder,
        ),
        row(
            "down",
            "Focus the newer message while selecting",
            "Messages",
            SelectionFocusNewer,
        ),
        row(
            "shift-up",
            "Extend the selection to the older message",
            "Messages",
            SelectionExtendOlder,
        ),
        row(
            "shift-down",
            "Extend the selection to the newer message",
            "Messages",
            SelectionExtendNewer,
        ),
    ]
}

/// Chords the composer's own `Input` context binds (page keys, the Cmd/Ctrl
/// up/down caret jumps on macOS, bracket indent). A binding without a
/// context loses to those, so these twins sit in the composer's context
/// and added after the kit's; each handler propagates when the composer
/// should keep the key, which lets the kit binding run next.
pub(super) fn composer_scoped_bindings() -> Vec<KeyBinding> {
    let ctx = Some(COMPOSER_INPUT_CONTEXT);
    vec![
        KeyBinding::new(primary!("up"), ReplyToPrevious, ctx),
        KeyBinding::new(primary!("down"), ReplyToNext, ctx),
        KeyBinding::new("pageup", HistoryPageUp, ctx),
        KeyBinding::new("pagedown", HistoryPageDown, ctx),
        KeyBinding::new(primary!("]"), ShowChatPreview, ctx),
    ]
}

/// kit Phase 7: the application menus — File / Edit / View / Window / Help,
/// every item wired to a working action. `setup_app_menus` installs them
/// twice from this one definition: `cx.set_menus` drives the native menu bar
/// on macOS, and `GlobalState::set_app_menus` feeds kit's `AppMenuBar`,
/// rendered in-window on Linux/Windows.
pub(super) fn app_menus() -> Vec<Menu> {
    // Local aliases: `Copy` would shadow the derive macro's `Copy` at
    // module scope.
    use gpui_kit::component::input::{
        Copy as CopyAction, Cut as CutAction, Paste as PasteAction, Redo as RedoAction,
        SelectAll as SelectAllAction, Undo as UndoAction,
    };
    // HIG: on macOS Quit lives in the app menu, not File.
    #[cfg(target_os = "macos")]
    let file_items = vec![MenuItem::action("Close Window", CloseWindow)];
    #[cfg(not(target_os = "macos"))]
    let file_items = vec![
        MenuItem::action("Close Window", CloseWindow),
        MenuItem::separator(),
        MenuItem::action("Quit Quill", QuitApp),
    ];
    let mut menus = Vec::new();
    // HIG: on macOS Quit lives in the app menu, not File.
    #[cfg(target_os = "macos")]
    menus.push(Menu::new("Quill").items([
        MenuItem::action("Settings…", OpenSettings),
        MenuItem::action("Lock Quill", LockApp),
        MenuItem::separator(),
        MenuItem::action("Quit Quill", QuitApp),
    ]));
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

// Context menus own typing and chat shortcuts; native focus traversal and
// activation still work, as do macOS window commands.
pub(in crate::ui) fn context_menu_captures_key(key: &Keystroke) -> bool {
    if key.modifiers.platform && matches!(key.key.as_str(), "q" | "w" | "m") {
        return false;
    }
    if !key.modifiers.control
        && !key.modifiers.platform
        && !key.modifiers.alt
        && matches!(key.key.as_str(), "tab" | "enter" | "space")
    {
        return false;
    }
    true
}
