# Main menu as a filterable icon grid

Owner ask (2026-11-07): the main menu (the chat list's top-left button) had
grown to 20 rows. Drop what the window already offers elsewhere, and show the
rest as an icon grid with a filter field over a see-through surface.

## What left the menu, and where it still lives

| Entry | Still reachable from |
| --- | --- |
| Contacts, Calls | The chat list's Chats / Contacts / Calls tabs |
| Saved Messages | Pinned in the chat list, chat search, the shortcut and deep link |
| New story | The "+" at the start of the stories strip |
| New supergroup | Gone: tdesktop has only New Group, and a group upgrades as it grows |
| New secret chat | A contact's profile menu (tdesktop has no menu entry either). The inline contact picker it opened is deleted |
| New Community | A "New Community" button in the Communities box |

Kept, because nothing else reaches them: My Profile, My Stories, Emoji
Status, New Group, New Channel, Communities, My Groups, My Channels,
Downloads, the archive pair (only while the archive lives in the menu), Mark
all chats as read, Accounts, Settings and Night Mode.

## Shape

- `quill::main_menu` owns the tile list, the filter (each query word starts a
  label word) and the arrow-key moves; `src/ui/main_menu_grid.rs` draws them
  from `MainMenuUi` (open flag, filter field, highlighted tile).
- The surface is the kit `Popover`, controlled by `MainMenuUi::open`. Its
  content is `occlude()`d, so a click on the panel never reaches the chat list
  under it; a click outside closes it; Escape closes it and returns focus.
- Night Mode is a switch under the grid, not a tile: it changes a state the
  window shows at once, so the menu stays open.
- The filter field has focus on open. Arrows move a highlight through the
  grid, Enter opens the highlighted tile. Typing highlights the first match.

## Why no blur

GPUI draws no backdrop blur inside a window on any of its renderers, and the
only blur on offer is the OS window material, which would need the menu in a
separate window. The panel uses the theme's popover color at 94% opacity: the
list shows faintly through it without hurting the labels. A real backdrop blur
needs a renderer pass that samples the frame under the panel, on Metal, wgpu
and DirectX alike.
