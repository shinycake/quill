# Keyboard reach for the archive, story, playlist and folder tab menus

## What tdesktop does

`Ui::Menu` (lib_ui/ui/widgets/menu/menu.cpp, `handleKeyPress`) moves the
selection with Up and Down only. It wraps at both ends and skips disabled
actions (lines 381-420), and when nothing is selected Down picks the first
action and Up the last. Enter, Return and Space run the selected action
(menu_action.cpp, 222-232). Escape hides the popup, and Left/Right open and
close submenus (popup_menu.cpp, 577-590). Tab is not handled by menus
there.

## What changed

- `src/ui/context_menu_keys.rs` is new and holds everything shared:
  - `context_menu_item`: the one item builder (icon, label, hover fill,
    `Role::MenuItem`, `tab_index(0)`, focus treatment) used by the archive
    row, story tile, playlist track and folder tab menus. A disabled item
    is dimmed and has no tab stop, so keyboard movement skips it.
  - Focus treatment: the same fill as hover plus a border in the theme's
    `ring` colour. The border is always present and transparent, so
    focusing an item moves nothing. The kit's `focus_ring_style` needs a
    `Window` and a focus-state query, which a `focus_visible` style cannot
    do; `theme.ring` is the token it draws with.
  - `ContextMenuUp` / `ContextMenuDown`, bound to Up and Down in the
    `QuillContextMenu` key context set on each menu overlay. The handlers
    use GPUI's own tab stops (`focus_next` / `focus_prev`) and keep going
    until focus is back inside the menu, which gives the wrap. No selected
    index is stored; focus is the state.
  - `QuillApp::context_menu_open()`: the single "any context menu open"
    predicate, used by the focus hand-off in `app_render/mod.rs` and the
    Escape stack layer.
- `context_menu_captures_key` (the interceptor that keeps typing keys out of
  the chat while the chat, archive or folder menu is open) now lets Up and
  Down through, like Tab.
- The playlist menu is in the focus hand-off, so opening it moves focus into
  the menu frame.
- Enter and Space run a focused item through the kit's keyboard click;
  Escape closes through the Escape stack and focus goes back to where it
  was.

Left for later: the message and chat-list menus still have no tab stops, and
no menu has submenus.

## How verified

- GPUI tests (`context_menu_focus_tests`, `playlist_manage` tests): Tab and
  Down land on an item inside the menu, Down moves item to item, Up from the
  first item wraps to the last and Down wraps back, the story menu cycles
  through exactly its four items, Escape closes the menu and restores the
  previous focus.
- `--screenshot-demo ready-mute-archive` with `r:` on the archive row, then
  `k:tab` and `k:down`, in light, dark and high contrast
  (`QUILL_DEMO_THEME`); PNGs read back, not committed. No demo kind shows a
  story tile or the playlist page, so those two menus are covered by the
  tests and the shared builder.
