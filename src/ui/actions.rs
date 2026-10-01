//! app action definitions (keybindings).

use gpui_kit::*;

actions!(
    quill_ui,
    [
        FocusSidebar,
        FocusComposer,
        LoadOlder,
        OpenSearch,
        OpenChatSearch,
        ChatSearchNewer,
        ChatSearchOlder,
        CancelSearch,
        QuitApp,
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
