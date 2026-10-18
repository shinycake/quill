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
    `Role::MenuItem`, focus handle, focus ring) used by the archive row,
    story tile, playlist track and folder tab menus. A disabled item is
    dimmed and gets no focus handle, so keyboard movement skips it.
  - `ContextMenuItems` (in the frame state): the focus handles of the open
    menu's enabled items, in drawing order, kept by position so they survive
    re-renders. Up and Down step through this list only, with wrap; they
    never walk the window's tab order. When the menu has no enabled item the
    menu frame takes the focus.
  - Focus ring: an inset shadow in the theme's `ring` colour, shown for
    keyboard focus only. It takes no layout room (the earlier transparent
    border added 2 px to every row) and, being a ring and not a fill, it reads
    differently from hover. The kit's `focus_ring_style` needs a `Window` at
    render time, which the menu builders do not have; the inset shadow draws
    the same token without it.
  - `QuillApp::context_menu_open()` and `dismiss_context_menus()`: the single
    "any context menu open" predicate and the single close, used by the
    focus hand-off, the keystroke interceptor and the Escape stack.
- The keystroke interceptor (`app_demo.rs`) now covers every menu in that
  list (the story and playlist menus were missing) and takes Up and Down
  whenever a menu is open, like tdesktop's menu: with the focus still
  outside the menu the first arrow moves it in, and the arrows never reach
  the chat list underneath. The message and chat menus have no items, so
  there the arrows are swallowed and the frame keeps the focus.
- The menu arrows are fixed keys, not rebindable and not in the shortcut
  list: they are handled by the interceptor, not by the keymap.
- The playlist menu is in the focus hand-off, so opening it moves focus into
  the menu frame.
- Enter and Space run a focused item through the kit's keyboard click;
  Escape closes through the Escape stack and focus goes back to where it
  was.

Left for later: the message and chat-list menus still have no tab stops, and
no menu has submenus.

## How verified

- GPUI tests (`context_menu_focus_tests`, `playlist_manage` tests): Tab and
  Down land on an item inside the menu; Down and Up wrap with exact item
  counts (archive: derived from the unread state; story: from
  `story_menu_entries`, the same list the overlay draws; playlist: the claimed
  handles); arrows with the focus outside the menu enter it and leave the
  chat list selection alone; a menu with no items keeps the frame focused;
  disabled items get no handle and are never a target; with no menu open
  Down still reaches the chat list; Escape closes the menu and restores the
  previous focus.
- `--screenshot-demo ready-mute-archive` with `r:` on the archive row, then
  `k:down`, `k:down;k:up` and `k:up` from the first item (wrap), in light, dark and high contrast
  (`QUILL_DEMO_THEME`); PNGs read back, not committed. No demo kind shows a
  story tile or the playlist page, so those two menus are covered by the
  tests and the shared builder.
