//! app action definitions (keybindings).

use gpui_kit::*;

actions!(
    quill_ui,
    [
        FocusSidebar,
        FocusComposer,
        /// Open the next / previous chat in the visible list.
        NextChat,
        PrevChat,
        LoadOlder,
        OpenSearch,
        OpenChatSearch,
        ChatSearchNewer,
        ChatSearchOlder,
        CancelSearch,
        QuitApp,
        OpenSettings,
        /// kit Phase 7: close the window (Cmd/Ctrl+W, File menu). Quits on
        /// Linux/Windows; on macOS the app stays alive for its menu bar.
        CloseWindow,
        /// kit Phase 7: minimize the window (Cmd/Ctrl+M, Window menu).
        MinimizeWindow,
        /// kit Phase 7: zoom (maximize/restore) the window (Window menu).
        ZoomWindow,
        /// kit Phase 7: toggle fullscreen (F11 / Cmd+Ctrl+F, View menu).
        ToggleFullscreen,
        /// kit Phase 8: flip the whole app between light and dark
        /// (`chat_theme::set_theme_mode` drives the kit theme and the
        /// Quill token palette together; View menu).
        ToggleTheme,
        /// kit Phase 7: open the Quill repo in the browser (Help menu).
        OpenHelp,
        /// Slice parity:platform-shortcuts-reference: open the keyboard
        /// shortcuts reference dialog (Help menu).
        OpenShortcuts,
        SubmitPhone,
        SubmitCode,
        SubmitPassword,
        /// M1: composer formatting shortcuts (ctrl-b / ctrl-i / ctrl-u).
        FormatBold,
        FormatItalic,
        FormatUnderline,
        /// Parity slice 5: step the fullscreen media viewer to the
        /// previous / next item (left/right arrows, viewer-open only).
        ViewerPrev,
        ViewerNext,
        /// Parity slice 5: reset the viewer visual's zoom/pan to fit (`0`).
        ViewerZoomReset,
        /// Parity slice 5: zoom the viewer visual in/out (`=` / `-`,
        /// viewer-open only).
        ViewerZoomIn,
        ViewerZoomOut
    ]
);

/// codex:spellcheck-native: composer context-menu "spelling" items. They
/// carry their word, so they are built per menu and dispatched by the
/// native menu (`NativeMenu` items are actions); never bound to keys.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = quill_ui, no_json)]
pub struct SpellingReplace {
    /// Byte range of `word` in the draft when the menu opened.
    pub start: usize,
    pub end: usize,
    pub word: String,
    pub replacement: String,
}

/// "Add to Dictionary".
#[derive(Clone, PartialEq, Action)]
#[action(namespace = quill_ui, no_json)]
pub struct SpellingLearn {
    pub word: String,
}

/// "Remove from Dictionary" (a word the user added earlier).
#[derive(Clone, PartialEq, Action)]
#[action(namespace = quill_ui, no_json)]
pub struct SpellingUnlearn {
    pub word: String,
}

/// "Ignore" — accepted until quit.
#[derive(Clone, PartialEq, Action)]
#[action(namespace = quill_ui, no_json)]
pub struct SpellingIgnore {
    pub word: String,
}
