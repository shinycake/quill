# Chat-list selection bar: every action reachable at any sidebar width

Bug: at the default window the text buttons (Pin Read Mute Archive Select unread Delete) overflowed the sidebar;
Archive was clipped and the rest unreachable.

tdesktop: the chat-list selection actions are icon buttons in a top bar (no text labels). Quill follows the
gpui-kit Design Guides: icon-only ghost `Button`s with tooltips for the four common actions (Pin, Read, Mute,
Archive), a kit `dropdown_menu` ("More") for Select unread and Delete, and a close button. The count label
truncates first. The bar is built from `chat_list.selected` each render and keeps no state of its own.
Code: `src/ui/chat_list/select_bar.rs`. Buttons are real kit buttons, so Tab order is bar order and the menu has
the kit's keyboard handling.

Verified: unit tests (every action is inline or in the menu exactly once; ids unique); captures of the
`ready-chat-list-3` demo at 1200x800 in light and dark, default and 240 px sidebar, and the More menu open.
