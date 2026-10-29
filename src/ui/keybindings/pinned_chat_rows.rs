//! Methods moved out of `keybindings.rs` to keep files under 1000 lines.

use super::*;
use crate::ui::actions::ComposerEditDate;
#[cfg(not(target_os = "macos"))]
use crate::ui::actions::ZoomWindow;
#[cfg(target_os = "macos")]
use crate::ui::actions::{HideApp, HideOtherApps, ShowAllApps};
#[cfg(target_os = "macos")]
use quill::lang::tr_with;
use quill::lang::{keys, tr};

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
        tr(keys::Q_PREFS_OPEN_PINNED_CHAT_1),
        tr(keys::Q_PREFS_OPEN_PINNED_CHAT_2),
        tr(keys::Q_PREFS_OPEN_PINNED_CHAT_3),
        tr(keys::Q_PREFS_OPEN_PINNED_CHAT_4),
        tr(keys::Q_PREFS_OPEN_PINNED_CHAT_5),
        tr(keys::Q_PREFS_OPEN_PINNED_CHAT_6),
        tr(keys::Q_PREFS_OPEN_PINNED_CHAT_7),
        tr(keys::Q_PREFS_OPEN_PINNED_CHAT_8),
    ];
    chords
        .into_iter()
        .zip(labels)
        .enumerate()
        .map(|(index, (chord, label))| {
            row(
                chord,
                label,
                tr(keys::Q_PREFS_NAVIGATION),
                OpenPinnedChat { index },
            )
        })
        .collect()
}

/// The message-history shortcuts: reply navigation, attach, scrolling and
/// deleting a selection (tdesktop `HistoryWidget::keyPressEvent`,
/// `HistoryInner::keyPressEvent`).
pub(super) fn message_rows() -> Vec<ShortcutRow> {
    vec![
        row(
            primary!("up"),
            tr(keys::Q_PREFS_REPLY_TO_PREVIOUS_MESSAGE),
            tr(keys::Q_PREFS_MESSAGES),
            ReplyToPrevious,
        ),
        row(
            primary!("down"),
            tr(keys::Q_PREFS_REPLY_TO_NEXT_MESSAGE),
            tr(keys::Q_PREFS_MESSAGES),
            ReplyToNext,
        ),
        row(
            primary!("o"),
            tr(keys::Q_PREFS_ATTACH_FILE),
            tr(keys::Q_PREFS_MESSAGES),
            AttachFile,
        ),
        row(
            "pageup",
            tr(keys::Q_PREFS_SCROLL_UP_A_PAGE),
            tr(keys::Q_PREFS_MESSAGES),
            HistoryPageUp,
        ),
        row(
            "pagedown",
            tr(keys::Q_PREFS_SCROLL_DOWN_A_PAGE),
            tr(keys::Q_PREFS_MESSAGES),
            HistoryPageDown,
        ),
        row(
            "home",
            tr(keys::Q_PREFS_SCROLL_TO_THE_FIRST_MESSAGE),
            tr(keys::Q_PREFS_MESSAGES),
            HistoryToTop,
        ),
        row(
            "end",
            tr(keys::Q_PREFS_SCROLL_TO_THE_LATEST_MESSAGE),
            tr(keys::Q_PREFS_MESSAGES),
            HistoryToBottom,
        ),
        row(
            "delete",
            tr(keys::Q_PREFS_DELETE_SELECTED_MESSAGES),
            tr(keys::Q_PREFS_MESSAGES),
            DeleteSelection,
        ),
        row(
            "backspace",
            tr(keys::Q_PREFS_DELETE_SELECTED_MESSAGES),
            tr(keys::Q_PREFS_MESSAGES),
            DeleteSelection,
        ),
        row(
            primary!("shift-a"),
            tr(keys::Q_PREFS_SELECT_THE_FOCUSED_MESSAGE),
            tr(keys::Q_PREFS_MESSAGES),
            ToggleMessageSelection,
        ),
        // tdesktop's chord. Some systems take Ctrl+Space themselves (an
        // input-source switch); the Shift+A chord above always works.
        row(
            "ctrl-space",
            tr(keys::Q_PREFS_SELECT_THE_FOCUSED_MESSAGE),
            tr(keys::Q_PREFS_MESSAGES),
            ToggleMessageSelection,
        ),
        row(
            "up",
            tr(keys::Q_PREFS_FOCUS_THE_OLDER_MESSAGE_WHILE),
            tr(keys::Q_PREFS_MESSAGES),
            SelectionFocusOlder,
        ),
        row(
            "down",
            tr(keys::Q_PREFS_FOCUS_THE_NEWER_MESSAGE_WHILE),
            tr(keys::Q_PREFS_MESSAGES),
            SelectionFocusNewer,
        ),
        row(
            "shift-up",
            tr(keys::Q_PREFS_EXTEND_THE_SELECTION_TO_THE),
            tr(keys::Q_PREFS_MESSAGES),
            SelectionExtendOlder,
        ),
        row(
            "shift-down",
            tr(keys::Q_PREFS_EXTEND_THE_SELECTION_TO_THE_2),
            tr(keys::Q_PREFS_MESSAGES),
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

/// Composer rows of the shortcut list kept out of `shortcut_rows`; their
/// bindings are installed with `composer_bindings`.
pub(super) fn composer_extra_rows() -> Vec<ShortcutRow> {
    vec![ShortcutRow {
        label: tr(keys::Q_PREFS_DATE_SELECTED_TEXT),
        section: tr(keys::Q_PREFS_COMPOSER),
        binding: KeyBinding::new(
            primary!("shift-d"),
            ComposerEditDate,
            Some(COMPOSER_INPUT_CONTEXT),
        ),
    }]
}

/// kit Phase 7: the application menus — File / Edit / View / Window / Help,
/// every item wired to a working action. `setup_app_menus` installs them
/// twice from this one definition: `cx.set_menus` drives the native menu bar
/// on macOS, and `GlobalState::set_app_menus` feeds kit's `AppMenuBar`,
/// rendered in-window on Linux/Windows.
pub(super) fn app_menus() -> Vec<Menu> {
    app_menus_with(crate::ui::os_menus::WindowMenuState::default())
}

/// The application menus with the Window menu's enabled items for `state`
/// (macOS; `os_menus` rebuilds the menu bar when the state changes).
pub(in crate::ui) fn app_menus_with(
    #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
    state: crate::ui::os_menus::WindowMenuState,
) -> Vec<Menu> {
    // Local aliases: `Copy` would shadow the derive macro's `Copy` at
    // module scope.
    use gpui_kit::component::input::{
        Copy as CopyAction, Cut as CutAction, Paste as PasteAction, Redo as RedoAction,
        SelectAll as SelectAllAction, Undo as UndoAction,
    };
    // HIG: on macOS Quit lives in the app menu, not File.
    #[cfg(target_os = "macos")]
    let file_items = vec![MenuItem::action(
        tr(keys::Q_PREFS_CLOSE_WINDOW_2),
        CloseWindow,
    )];
    #[cfg(not(target_os = "macos"))]
    let file_items = vec![
        MenuItem::action(tr(keys::Q_PREFS_CLOSE_WINDOW_2), CloseWindow),
        MenuItem::separator(),
        MenuItem::action(tr(keys::Q_PREFS_QUIT_QUILL), QuitApp),
    ];
    // macOS: Telegram Desktop's Window menu (`os_menus::window_menu`), which
    // carries Toggle Full Screen. Linux/Windows keep the in-window menu bar's
    // own Window menu (tdesktop shows no menu bar there).
    #[cfg(target_os = "macos")]
    let (view_items, window_items) = (
        vec![
            MenuItem::action(tr(keys::Q_PREFS_QUICK_SWITCH), OpenSearch),
            MenuItem::action(tr(keys::Q_PREFS_FIND_IN_CHAT_2), OpenChatSearch),
            MenuItem::separator(),
            MenuItem::action(
                tr(if state.sidebar_hidden {
                    keys::Q_CHAT_SHOW_SIDEBAR
                } else {
                    keys::Q_CHAT_HIDE_SIDEBAR
                }),
                ToggleSidebar,
            ),
            // kit Phase 8: light/dark switch for the whole app.
            MenuItem::action(tr(keys::Q_PREFS_TOGGLE_THEME), ToggleTheme),
        ],
        crate::ui::os_menus::window_menu_items(state),
    );
    #[cfg(not(target_os = "macos"))]
    let (view_items, window_items) = (
        vec![
            MenuItem::action(tr(keys::Q_PREFS_QUICK_SWITCH), OpenSearch),
            MenuItem::action(tr(keys::Q_PREFS_FIND_IN_CHAT_2), OpenChatSearch),
            MenuItem::separator(),
            // The in-window menu bar is built once, so its label does not
            // follow the state.
            MenuItem::action(tr(keys::Q_PREFS_TOGGLE_SIDEBAR), ToggleSidebar),
            MenuItem::action(tr(keys::Q_PREFS_ENTER_FULL_SCREEN), ToggleFullscreen),
            // kit Phase 8: light/dark switch for the whole app.
            MenuItem::action(tr(keys::Q_PREFS_TOGGLE_THEME), ToggleTheme),
        ],
        vec![
            MenuItem::action(tr(keys::Q_PREFS_MINIMIZE), MinimizeWindow),
            MenuItem::action(tr(keys::Q_PREFS_ZOOM), ZoomWindow),
        ],
    );
    let mut menus = Vec::new();
    // HIG: on macOS Quit lives in the app menu, not File.
    // The standard AppKit app menu: GPUI builds the menu bar from data, so
    // Services and the Hide commands exist only when listed here.
    #[cfg(target_os = "macos")]
    menus.push(Menu::new("Quill").items([
        MenuItem::action(tr(keys::Q_PREFS_SETTINGS), OpenSettings),
        MenuItem::action(tr(keys::Q_PREFS_LOCK_QUILL), LockApp),
        MenuItem::separator(),
        MenuItem::os_submenu(tr(keys::MAC_MENU_SERVICES), SystemMenuType::Services),
        MenuItem::separator(),
        MenuItem::action(
            tr_with(keys::MAC_MENU_HIDE_TELEGRAM, &[("telegram", "Quill")]),
            HideApp,
        ),
        MenuItem::action(tr(keys::MAC_MENU_HIDE_OTHERS), HideOtherApps),
        MenuItem::action(tr(keys::MAC_MENU_SHOW_ALL), ShowAllApps),
        MenuItem::separator(),
        MenuItem::action(tr(keys::Q_PREFS_QUIT_QUILL), QuitApp),
    ]));
    menus.extend([
        Menu::new("File").items(file_items),
        Menu::new("Edit").items([
            MenuItem::action(tr(keys::Q_PREFS_UNDO), UndoAction),
            MenuItem::action(tr(keys::Q_PREFS_REDO), RedoAction),
            MenuItem::separator(),
            MenuItem::os_action(tr(keys::Q_PREFS_CUT), CutAction, OsAction::Cut),
            MenuItem::os_action(tr(keys::MAC_MENU_COPY), CopyAction, OsAction::Copy),
            MenuItem::os_action(tr(keys::Q_PREFS_PASTE), PasteAction, OsAction::Paste),
            MenuItem::separator(),
            MenuItem::os_action(
                tr(keys::MAC_MENU_SELECT_ALL),
                SelectAllAction,
                OsAction::SelectAll,
            ),
        ]),
        Menu::new("View").items(view_items),
        Menu::new("Window").items(window_items),
        Menu::new("Help").items([
            MenuItem::action(tr(keys::Q_PREFS_KEYBOARD_SHORTCUTS_2), OpenShortcuts),
            MenuItem::action(tr(keys::Q_PREFS_QUILL_ON_GITHUB), OpenHelp),
        ]),
    ]);
    menus
}

/// kit Phase 7: install the app menus — native on macOS, kit `AppMenuBar`
/// data on Linux/Windows. Call once after `gpui_kit::init` + `bind_keys`.
pub fn setup_app_menus(cx: &mut App) {
    #[cfg(target_os = "macos")]
    {
        cx.on_action(|_: &HideApp, cx| cx.hide());
        cx.on_action(|_: &HideOtherApps, cx| cx.hide_other_apps());
        cx.on_action(|_: &ShowAllApps, cx| cx.unhide_other_apps());
    }
    let owned: Vec<OwnedMenu> = app_menus().into_iter().map(Menu::owned).collect();
    cx.set_menus(app_menus());
    GlobalState::global_mut(cx).set_app_menus(owned);
}

// Context menus own typing and chat shortcuts; native focus traversal and
// activation still work, as do macOS app and window commands.
pub(in crate::ui) fn context_menu_captures_key(key: &Keystroke) -> bool {
    if key.modifiers.platform && matches!(key.key.as_str(), "q" | "w" | "m" | "h") {
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
