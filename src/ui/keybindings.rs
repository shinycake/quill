use super::actions::{
    CancelSearch, ChatSearchNewer, ChatSearchOlder, CloseWindow, FocusComposer, FocusSidebar,
    FormatBold, FormatItalic, FormatUnderline, LoadOlder, MinimizeWindow, OpenChatSearch,
    OpenGiftPurchase, OpenHelp, OpenSearch, QuitApp, ToggleFullscreen, ToggleTheme, ViewerNext,
    ViewerPrev, ViewerZoomIn, ViewerZoomOut, ViewerZoomReset, ZoomWindow,
};
use gpui_kit::component::*;
use gpui_kit::*;
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
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
        KeyBinding::new("cmd-1", FocusSidebar, None),
        KeyBinding::new("ctrl-1", FocusSidebar, None),
        KeyBinding::new("cmd-l", FocusComposer, None),
        KeyBinding::new("ctrl-l", FocusComposer, None),
        KeyBinding::new("cmd-up", LoadOlder, None),
        KeyBinding::new("ctrl-up", LoadOlder, None),
        KeyBinding::new("cmd-k", OpenSearch, None),
        KeyBinding::new("ctrl-k", OpenSearch, None),
        KeyBinding::new("cmd-f", OpenChatSearch, None),
        KeyBinding::new("ctrl-f", OpenChatSearch, None),
        KeyBinding::new("cmd-g", ChatSearchNewer, None),
        KeyBinding::new("ctrl-g", ChatSearchNewer, None),
        KeyBinding::new("cmd-shift-g", ChatSearchOlder, None),
        KeyBinding::new("ctrl-shift-g", ChatSearchOlder, None),
        KeyBinding::new("escape", CancelSearch, None),
        // Parity slice 5: the handlers no-op (and let the keystroke reach
        // text inputs) unless the media viewer is open.
        KeyBinding::new("left", ViewerPrev, None),
        KeyBinding::new("right", ViewerNext, None),
        KeyBinding::new("0", ViewerZoomReset, None),
        KeyBinding::new("=", ViewerZoomIn, None),
        KeyBinding::new("-", ViewerZoomOut, None),
        // M1: composer formatting shortcuts; the handlers no-op unless
        // the composer textarea has focus.
        KeyBinding::new("ctrl-b", FormatBold, None),
        KeyBinding::new("ctrl-i", FormatItalic, None),
        KeyBinding::new("ctrl-u", FormatUnderline, None),
    ]);
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
    let mut file_items = vec![
        MenuItem::action("Close Window", CloseWindow),
        // Slice parity:gifts-signed-comment: "Buy collectible gift" dialog
        // (`sendResoldGift` with the personal comment).
        MenuItem::action("Buy Collectible Gift…", OpenGiftPurchase),
    ];
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
