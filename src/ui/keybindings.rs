use super::actions::{
    CancelSearch, ChatSearchNewer, ChatSearchOlder, CloseWindow, FocusComposer, FocusSidebar,
    FormatBold, FormatItalic, FormatUnderline, LoadOlder, MinimizeWindow, OpenChatSearch, OpenHelp,
    OpenSearch, OpenShortcuts, QuitApp, ToggleFullscreen, ToggleTheme, ViewerNext, ViewerPrev,
    ViewerZoomIn, ViewerZoomOut, ViewerZoomReset, ZoomWindow,
};
use gpui_kit::component::*;
use gpui_kit::*;

/// One row of the app's keyboard-shortcut reference: the single source of
/// truth for both `bind_keys` (what the app listens for) and the shortcuts
/// reference dialog (what the user sees). `keystroke` is gpui format
/// (e.g. `"cmd-q"`); the dialog renders it through kit's `Kbd`.
pub struct ShortcutRow {
    pub keystroke: &'static str,
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
        keystroke,
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

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys(shortcut_rows().into_iter().map(|row| row.binding));
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
    let mut file_items = vec![MenuItem::action("Close Window", CloseWindow)];
    // HIG: on macOS Quit lives in the app menu, not File.
    #[cfg(not(target_os = "macos"))]
    {
        file_items.push(MenuItem::separator());
        file_items.push(MenuItem::action("Quit Quill", QuitApp));
    }
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
            // Slice parity:platform-shortcuts-reference: the reference
            // dialog renders every row of `shortcut_rows()`.
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
    use super::*;
    use std::collections::HashSet;

    /// The reference dialog renders these rows with `Kbd` via
    /// `Keystroke::parse` + `expect` — a row that doesn't parse would panic
    /// at dialog-open time, so the table is validated here instead.
    #[test]
    fn every_shortcut_row_parses_and_is_well_formed() {
        let rows = shortcut_rows();
        assert!(!rows.is_empty(), "shortcut table must not be empty");
        let mut seen = HashSet::new();
        for row in &rows {
            assert!(
                Keystroke::parse(row.keystroke).is_ok(),
                "unparsable keystroke: {}",
                row.keystroke
            );
            assert!(!row.label.is_empty(), "empty label for {}", row.keystroke);
            assert!(
                !row.section.is_empty(),
                "empty section for {}",
                row.keystroke
            );
            assert!(
                seen.insert(row.keystroke),
                "duplicate keystroke binding: {}",
                row.keystroke
            );
        }
    }

    /// `bind_keys` must install exactly the table's bindings — the dialog's
    /// "cannot drift" claim rests on both reading `shortcut_rows()`.
    #[test]
    fn table_covers_all_hardcoded_bindings() {
        // Count of bindings previously hardcoded in `bind_keys` (31).
        assert_eq!(shortcut_rows().len(), 31);
    }
}
