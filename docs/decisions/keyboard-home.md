# Every window keeps the keyboard where its shortcuts reach

2026-11-26. Code: `src/ui/keyboard_home.rs`, tests in
`src/ui/app_shortcuts_tests.rs`.

## The bug

⌘, stopped opening Settings, in Classic and Floating alike. GPUI runs a
key's action, and decides whether a menu item is enabled, along the path
from the focused element up to the window root. Quill binds every app-wide
action (62 of them: Settings, lock, close, the chat and folder keys, the
viewer and formatting keys) on the root `div` of `QuillApp` and of each
chat window, one level below GPUI's own root.

A click anywhere in Settings gives its page the keyboard. Closing Settings
removes that page from the tree, but the window's focus still names it.
GPUI then dispatches from its own root, where none of Quill's actions are
bound: every window shortcut did nothing and the macOS menu greyed its
items until the user clicked into a field. Settings opened from a menu
over a focused composer had the same hole the other way: Escape could not
close it.

## The rule

Each window draws a home, a zero-size element inside the element that
binds its actions, and hooks GPUI's focus-lost report (`on_focus_lost`).
When the focused element is gone and the window's focus still names it
(a window blurred on purpose, with nothing focused, is left alone), the
keyboard moves to:

1. the nearest focusable ancestor still drawn (`focus_lost_restore_target`);
2. else the first place the window names that is drawn where its actions
   reach: Settings while open, then the composer, then the chat list (the
   main window); a chat window names none;
3. else home.

"Drawn where the actions reach" is `Window::is_action_available_in` for
`OpenSettings`, which every Quill window root binds.

## No key window

With the main window closed to the Dock no Quill window is key, so a key
or menu command goes to the app's global handlers. Quit had one; Settings,
Keyboard Shortcuts, Lock and Help did not, so ⌘, and those items did
nothing. They now bring the main window back and run there (not while
locked) (`os_menus::register_app_menu_actions`).
