# Hiding the chat list column

Owner request (Idan, 2026-10-21): a way to hide the whole left side of the
main window, the folder rail and the chat list, so the open chat gets the
full width. While it is hidden, the pointer at the window's left edge
brings it back over the chat for a moment.

## What tdesktop does

Nothing like it. Telegram Desktop's main window picks one, two or three
columns from the window's width
(`window/window_session_controller.cpp`, `computeColumnLayout`) and has no
user toggle to drop the chat list while there is room for it. This is a
Quill feature, so the GPUI Kit Design Guides decide the details.

## What it does

- **Hide / show.** A ghost icon button leads the chat header: Kit's
  `IconName::PanelLeftClose` ("Hide Sidebar") while the column is shown,
  `PanelLeftOpen` ("Show Sidebar") while it is hidden. It stays in one
  place in both states. The chat list's header was the first choice, but
  one more button there drops its Chats / Contacts / Calls labels to icons
  at the default 300 px width (`header_fit`), which would change the
  default look. The tooltip names the live shortcut
  (`tooltip_with_action`). The View menu has the same command: "Hide
  Sidebar" / "Show Sidebar" on macOS (the native menu is rebuilt when it
  changes), "Toggle Sidebar" in the in-window menu bar on Linux and
  Windows, which Kit builds once.
- **Shortcut.** Rebindable action `toggle-sidebar` (Settings > Keyboard
  Shortcuts and the shortcuts reference). macOS: Cmd+Ctrl+S, the system's
  Show/Hide Sidebar chord in Finder, Mail and Xcode. Linux and Windows:
  Ctrl+Shift+S. Neither was bound before (fixed, pack, rebindable and
  composer bindings checked); plain Ctrl+S stays free because it reads as
  Save.
- **Remembered.** `window_state.json` gets `sidebar_hidden` next to
  `sidebar_width` (`#[serde(default)]`, so older files load as shown). It is
  staged with the rest of the window geometry.
- **Peek.** While hidden, a 6 px strip at the left edge of the column row
  reveals the column. It slides in over the conversation without moving it,
  on the theme's sidebar fill with Kit's `shadow_xl` (the left `Sheet`
  surface), under Kit dialogs and notifications. Moving the pointer onto
  the chat starts a 280 ms grace, after which it slides back out. Clicking a
  chat in it opens the chat and it stays until the pointer leaves. It is
  held open while a context menu is up or the search field has the
  keyboard, and Escape closes it (an `esc_stack` layer).
- **One column.** In a narrow window nothing changes: no button, no strip,
  and the shortcut does nothing.

## Data shape

- `SidebarReveal` (`src/ui/sidebar_reveal.rs`, pure and unit-tested) owns
  the preference and its motion: `hidden`, two `motion::Glide` values
  (`docked`, 1 shown and 0 hidden, and `peek`, how far the hidden column
  has slid in), and the pointer state `Peek::{Off, Hovering, Leaving
  { since }}`. Reversing mid-flight starts from the current value
  (`Glide::go`). Showing the column while it peeks docks it where it is.
- `ShellInput::docked` feeds `shell_layout`, which stays a pure function: the
  list keeps its full `list` width (it slides out clipped, never squeezed,
  so the header fit checks are unchanged), while `chat_insets`,
  `chat_pane_width`, the side panels' room and the resize handle follow the
  visible width `ShellLayout::list_column`. A fully hidden column has no
  resize handle. `max_list_width` keeps describing the list beside the chat;
  a drag can only happen while the column is shown.
- The view reads both values on every render (`shell_layout/sidebar_view.rs`)
  and asks the frame clock for 60 fps ticks while either moves; no
  `with_animation`. The slide takes 200 ms with an ease-out cubic.
  Motion is instant under the OS reduce-motion preference or with
  Interface animations off (`power_saving::reduce_motion_now`).
- Leaving is detected by mouse moves on the column row outside the peeking
  column: the peek area blocks the mouse over its full width from the first
  frame, so a fast move into it is never taken for leaving, and a Kit menu
  opened from it (which blocks the mouse too) does not close it.

## Not covered

- Kit 0.8 has no collapsible pane for this: `Sidebar::collapsed` shrinks a
  Kit `Sidebar` to icons and `Sheet` is modal; Quill's chat list is
  neither. The buttons use the icons of Kit's `SidebarToggleButton`, built
  as plain ghost `Button`s because that component is fixed to the small size
  and one id and takes no tooltip, and the tooltip carries the shortcut.
- macOS traffic lights sit in the title bar above the column row, so the
  chat header needs no extra inset when the column is hidden.
- While the forum topic list replaces the chat list (`ForumColumn::Replacing`)
  there is no list column to hide.
