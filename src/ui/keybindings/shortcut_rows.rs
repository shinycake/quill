//! The keyboard-shortcut reference rows: what the app listens for and what
//! the shortcuts dialog shows.

use super::*;

/// One row of the app's keyboard-shortcut reference: the single source of
/// truth for both `bind_keys` (what the app listens for) and the shortcuts
/// reference dialog (what the user sees). `keystroke` is gpui format
/// (e.g. `"cmd-q"`); the dialog renders it through kit's `Kbd`.
pub struct ShortcutRow {
    pub label: &'static str,
    pub section: &'static str,
    pub binding: KeyBinding,
}

pub(super) fn row<A: Action>(
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
/// share a label (e.g. the Next chat chords) render as one entry with all
/// their chips.
pub fn shortcut_rows() -> Vec<ShortcutRow> {
    let mut rows = vec![
        // General.
        row(
            primary!("q"),
            tr(keys::Q_PREFS_QUIT_QUILL),
            tr(keys::Q_PREFS_GENERAL),
            QuitApp,
        ),
        // Also in the startup keymap, so the app menu's Settings item reads
        // ⌘, before an account's keymap is applied.
        row(
            primary!(","),
            tr(keys::MENU_SETTINGS),
            tr(keys::Q_PREFS_GENERAL),
            OpenSettings,
        ),
        // kit Phase 7: window-chrome shortcuts (HIG: Cmd+W close, Cmd+M
        // minimize; F11 / Cmd+Ctrl+F fullscreen).
        row(
            primary!("l"),
            tr(keys::Q_PREFS_LOCK_QUILL),
            tr(keys::Q_PREFS_GENERAL),
            LockApp,
        ),
        row(
            primary!("w"),
            tr(keys::Q_PREFS_CLOSE_WINDOW),
            tr(keys::Q_PREFS_GENERAL),
            CloseWindow,
        ),
        row(
            primary!("m"),
            tr(keys::Q_PREFS_MINIMIZE_WINDOW),
            tr(keys::Q_PREFS_GENERAL),
            MinimizeWindow,
        ),
        // macOS's own full-screen chord (the Window menu's "Toggle Full
        // Screen" reads ⌃⌘F, as in Telegram Desktop); F11 elsewhere, since
        // macOS keeps F11 for Show Desktop.
        #[cfg(target_os = "macos")]
        row(
            "cmd-ctrl-f",
            tr(keys::Q_PREFS_TOGGLE_FULLSCREEN),
            tr(keys::Q_PREFS_GENERAL),
            ToggleFullscreen,
        ),
        #[cfg(not(target_os = "macos"))]
        row(
            "f11",
            tr(keys::Q_PREFS_TOGGLE_FULLSCREEN),
            tr(keys::Q_PREFS_GENERAL),
            ToggleFullscreen,
        ),
        row(
            primary!("shift-t"),
            tr(keys::Q_PREFS_REOPEN_CLOSED_WINDOW),
            tr(keys::Q_PREFS_GENERAL),
            ReopenClosedWindow,
        ),
        row(
            SIDEBAR_CHORD,
            tr(keys::Q_PREFS_TOGGLE_SIDEBAR),
            tr(keys::Q_PREFS_GENERAL),
            ToggleSidebar,
        ),
        // Navigation.
        row(
            primary!("alt-1"),
            tr(keys::Q_PREFS_FOCUS_CHAT_LIST),
            tr(keys::Q_PREFS_NAVIGATION),
            FocusSidebar,
        ),
        row(
            primary!("shift-l"),
            tr(keys::Q_PREFS_FOCUS_MESSAGE_COMPOSER),
            tr(keys::Q_PREFS_NAVIGATION),
            FocusComposer,
        ),
        row(
            "alt-down",
            tr(keys::Q_PREFS_NEXT_CHAT),
            tr(keys::Q_PREFS_NAVIGATION),
            NextChat,
        ),
        row(
            "ctrl-tab",
            tr(keys::Q_PREFS_NEXT_CHAT),
            tr(keys::Q_PREFS_NAVIGATION),
            NextChat,
        ),
        row(
            primary!("pagedown"),
            tr(keys::Q_PREFS_NEXT_CHAT),
            tr(keys::Q_PREFS_NAVIGATION),
            NextChat,
        ),
        row(
            "alt-up",
            tr(keys::Q_PREFS_PREVIOUS_CHAT),
            tr(keys::Q_PREFS_NAVIGATION),
            PrevChat,
        ),
        row(
            "ctrl-shift-tab",
            tr(keys::Q_PREFS_PREVIOUS_CHAT),
            tr(keys::Q_PREFS_NAVIGATION),
            PrevChat,
        ),
        row(
            primary!("pageup"),
            tr(keys::Q_PREFS_PREVIOUS_CHAT),
            tr(keys::Q_PREFS_NAVIGATION),
            PrevChat,
        ),
        row(
            "alt-pageup",
            tr(keys::Q_PREFS_LOAD_OLDER_MESSAGES),
            tr(keys::Q_PREFS_NAVIGATION),
            LoadOlder,
        ),
        // Search.
        row(
            primary!("k"),
            tr(keys::Q_PREFS_QUICK_SWITCH_CHATS),
            tr(keys::Q_PREFS_SEARCH),
            OpenSearch,
        ),
        row(
            primary!("f"),
            tr(keys::Q_PREFS_FIND_IN_CHAT),
            tr(keys::Q_PREFS_SEARCH),
            OpenChatSearch,
        ),
        row(
            primary!("g"),
            tr(keys::Q_PREFS_NEXT_SEARCH_RESULT),
            tr(keys::Q_PREFS_SEARCH),
            ChatSearchNewer,
        ),
        row(
            primary!("shift-g"),
            tr(keys::Q_PREFS_PREVIOUS_SEARCH_RESULT),
            tr(keys::Q_PREFS_SEARCH),
            ChatSearchOlder,
        ),
        row(
            "escape",
            tr(keys::Q_PREFS_CLOSE_SEARCH_CANCEL),
            tr(keys::Q_PREFS_SEARCH),
            CancelSearch,
        ),
        // Parity slice 5: the handlers no-op (and let the keystroke reach
        // text inputs) unless the media viewer is open.
        row(
            "space",
            tr(keys::Q_PREFS_PAUSE_OR_RESUME_STORY),
            tr(keys::Q_PREFS_MEDIA_VIEWER),
            StoryTogglePause,
        ),
        row(
            "left",
            tr(keys::Q_PREFS_PREVIOUS_ITEM),
            tr(keys::Q_PREFS_MEDIA_VIEWER),
            ViewerPrev,
        ),
        row(
            "right",
            tr(keys::Q_PREFS_NEXT_ITEM),
            tr(keys::Q_PREFS_MEDIA_VIEWER),
            ViewerNext,
        ),
        row(
            "0",
            tr(keys::Q_PREFS_RESET_ZOOM),
            tr(keys::Q_PREFS_MEDIA_VIEWER),
            ViewerZoomReset,
        ),
        row(
            "=",
            tr(keys::Q_PREFS_ZOOM_IN),
            tr(keys::Q_PREFS_MEDIA_VIEWER),
            ViewerZoomIn,
        ),
        row(
            primary!("="),
            tr(keys::Q_PREFS_ZOOM_IN),
            tr(keys::Q_PREFS_MEDIA_VIEWER),
            ViewerZoomIn,
        ),
        row(
            "-",
            tr(keys::Q_PREFS_ZOOM_OUT),
            tr(keys::Q_PREFS_MEDIA_VIEWER),
            ViewerZoomOut,
        ),
        row(
            primary!("-"),
            tr(keys::Q_PREFS_ZOOM_OUT),
            tr(keys::Q_PREFS_MEDIA_VIEWER),
            ViewerZoomOut,
        ),
        row(
            "h",
            tr(keys::Q_PREFS_FLIP_HORIZONTALLY),
            tr(keys::Q_PREFS_MEDIA_VIEWER),
            ViewerFlipHorizontal,
        ),
        row(
            "v",
            tr(keys::Q_PREFS_FLIP_VERTICALLY),
            tr(keys::Q_PREFS_MEDIA_VIEWER),
            ViewerFlipVertical,
        ),
        row(
            primary!("c"),
            tr(keys::Q_PREFS_COPY_PHOTO),
            tr(keys::Q_PREFS_MEDIA_VIEWER),
            ViewerCopy,
        ),
        row(
            primary!("s"),
            tr(keys::SETTINGS_SAVE),
            tr(keys::Q_PREFS_MEDIA_VIEWER),
            ViewerSave,
        ),
        // M1: composer formatting shortcuts; the handlers no-op unless
        // the composer textarea has focus.
        row(
            primary!("b"),
            tr(keys::Q_PREFS_BOLD),
            tr(keys::Q_PREFS_COMPOSER),
            FormatBold,
        ),
        row(
            primary!("i"),
            tr(keys::Q_PREFS_ITALIC),
            tr(keys::Q_PREFS_COMPOSER),
            FormatItalic,
        ),
        row(
            primary!("u"),
            tr(keys::Q_PREFS_UNDERLINE),
            tr(keys::Q_PREFS_COMPOSER),
            FormatUnderline,
        ),
        row(
            primary!("shift-x"),
            tr(keys::Q_PREFS_STRIKETHROUGH),
            tr(keys::Q_PREFS_COMPOSER),
            FormatStrikethrough,
        ),
        row(
            primary!("shift-m"),
            tr(keys::Q_PREFS_MONOSPACE),
            tr(keys::Q_PREFS_COMPOSER),
            FormatMonospace,
        ),
        // Quote rows are appended below (one per chord spelling).
        row(
            primary!("shift-p"),
            tr(keys::Q_PREFS_SPOILER),
            tr(keys::Q_PREFS_COMPOSER),
            FormatSpoiler,
        ),
        row(
            primary!("shift-n"),
            tr(keys::Q_PREFS_CLEAR_FORMATTING),
            tr(keys::Q_PREFS_COMPOSER),
            FormatClear,
        ),
        // Scoped to the composer's key context; with nothing selected it
        // falls back to quick switch (see `composer_bindings`).
        ShortcutRow {
            label: tr(keys::Q_PREFS_LINK_SELECTED_TEXT),
            section: tr(keys::Q_PREFS_COMPOSER),
            binding: KeyBinding::new(link_chord(), ComposerEditLink, Some(COMPOSER_INPUT_CONTEXT)),
        },
        row(
            paste_plain_chord(),
            tr(keys::Q_PREFS_PASTE_AS_PLAIN_TEXT),
            tr(keys::Q_PREFS_COMPOSER),
            ComposerPastePlain,
        ),
    ];
    rows.extend(
        QUOTE_CHORDS
            .iter()
            .map(|chord| {
                row(
                    chord,
                    tr(keys::Q_PREFS_QUOTE),
                    tr(keys::Q_PREFS_COMPOSER),
                    FormatBlockQuote,
                )
            })
            .chain(composer_extra_rows()),
    );
    // The reference dialog prints a heading whenever the section changes,
    // so each group goes in next to its own section.
    let at = rows
        .iter()
        .position(|r| r.section == tr(keys::Q_PREFS_SEARCH))
        .unwrap_or(rows.len());
    rows.splice(
        at..at,
        chat_nav_rows().into_iter().chain(pinned_chat_rows()),
    );
    let at = rows
        .iter()
        .position(|r| r.section == tr(keys::Q_PREFS_MEDIA_VIEWER))
        .unwrap_or(rows.len());
    rows.splice(at..at, message_rows());
    rows
}

/// Chat-list navigation chords from tdesktop's `fillDefaults`: first/last
/// chat, folders, Saved Messages, Archive, Contacts, read, menu, preview.
/// Not user-rebindable (they are claimed like window chrome).
pub(super) fn chat_nav_rows() -> Vec<ShortcutRow> {
    vec![
        row(
            primary!("alt-home"),
            tr(keys::Q_PREFS_FIRST_CHAT),
            tr(keys::Q_PREFS_NAVIGATION),
            FirstChat,
        ),
        row(
            primary!("alt-end"),
            tr(keys::Q_PREFS_LAST_CHAT),
            tr(keys::Q_PREFS_NAVIGATION),
            LastChat,
        ),
        row(
            "ctrl-shift-up",
            tr(keys::Q_PREFS_PREVIOUS_FOLDER),
            tr(keys::Q_PREFS_NAVIGATION),
            PrevFolder,
        ),
        row(
            "ctrl-shift-down",
            tr(keys::Q_PREFS_NEXT_FOLDER),
            tr(keys::Q_PREFS_NAVIGATION),
            NextFolder,
        ),
        row(
            primary!("0"),
            tr(keys::Q_PREFS_OPEN_SAVED_MESSAGES),
            tr(keys::Q_PREFS_NAVIGATION),
            OpenSavedMessages,
        ),
        row(
            primary!("9"),
            tr(keys::Q_PREFS_OPEN_ARCHIVE),
            tr(keys::Q_PREFS_NAVIGATION),
            OpenArchive,
        ),
        row(
            primary!("j"),
            tr(keys::Q_PREFS_OPEN_CONTACTS),
            tr(keys::Q_PREFS_NAVIGATION),
            OpenContacts,
        ),
        row(
            primary!("r"),
            tr(keys::SHORTCUTS_READ_CHAT),
            tr(keys::Q_PREFS_NAVIGATION),
            MarkChatRead,
        ),
        row(
            primary!("\\"),
            tr(keys::Q_PREFS_CHAT_MENU),
            tr(keys::Q_PREFS_NAVIGATION),
            ShowChatMenu,
        ),
        row(
            primary!("]"),
            tr(keys::Q_PREFS_CHAT_PREVIEW),
            tr(keys::Q_PREFS_NAVIGATION),
            ShowChatPreview,
        ),
    ]
}
