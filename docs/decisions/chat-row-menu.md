# Chat row menu: Mute submenu, Add to folder, delete or leave per chat kind

## What tdesktop does

`Telegram/SourceFiles/window/window_peer_menu.cpp`, `Filler::fillContextMenuActions`
(line 1897) builds the chat list context menu in this order: new window, ungroup,
hide promotion, archive, pin, view profile (option), mute submenu, mark read or
unread, topic closed, add to folder, block user (non-contacts), ban, clear
history, delete chat, leave chat, delete topic.

- Mute, `addToggleMuteSubmenu` (line 584) and `PeerMenuAddMuteSubmenuAction`
  (line 241): not offered for Saved Messages, sublists and forums. A muted chat
  shows one "Unmute" action. Otherwise "Mute notifications" opens
  `MuteMenu::FillMuteMenu` (`menu/menu_mute.cpp:305`): Select tone, Disable or
  Enable sound, Mute for 1 hour / 8 hours / 2 days, "Mute for...", Mute forever.
- Add to folder, `addToggleFolder` (line 672): only when the account has
  folders. The submenu (`FillChooseFilterMenu`, `boxes/choose_filter_box.cpp:280`)
  lists every folder with a check on the ones holding the chat. Choosing one
  adds or removes the chat.
- `addDeleteChat` (line 879): "Delete chat" for a user, "Delete and leave" for a
  basic group, nothing for a channel or supergroup. `addLeaveChat` (line 895):
  "Leave group" or "Leave channel" for a supergroup or channel the user is in.

## What Quill had

`src/ui/message_actions/close_chat_preview.rs` had Mute as a forever toggle,
no folder entry, and one "Delete chat" label for every chat kind. Already there
and unchanged: new window, archive, pin, view profile, mark read or unread,
select, clear history (and for everyone), report, block, export. The mute
durations and the folder add and remove calls already existed
(`apply_chat_mute`, `add_open_chat_to_folder`, `remove_open_chat_from_folder`).

## What changed

- `src/chatlist_menu.rs`: `mute_offered` and `exit_entry` (pure, tested).
- `src/ui/message_actions/chat_menu_submenus.rs`: the Mute row and submenu, the
  Add to folder row and submenu, and the delete or leave row. A submenu is a kit
  `anchored` panel hanging off its row and opens on hover or click. Its open state
  is `ChatMenuState.submenu`, so the render stays a function of state.
- Delete chat of a basic group reads "Delete and leave" and its confirmation uses
  `lng_sure_delete_and_exit`. Supergroups and channels get "Leave group" or
  "Leave channel" (the existing leave confirmation) instead of "Delete chat".
- New keys in `lang/en/chat.strings` copied from tdesktop's `lang.strings`.

## Deviations

- "Select tone" and "Mute for..." open the chat's existing notification panel
  (which owns the tone list and the custom duration) instead of a standalone box.
  That also opens the chat.
- tdesktop adds the remaining mute time to "Unmute". TDLib gives the seconds
  left at update time only, so Quill omits it.
- A full folder is not pre-checked against the limit; TDLib's error shows as the
  existing folder failure note.

## Verification

GPUI UI tests in `chat_menu_submenus_tests.rs` (mute duration applies, muted chat
offers a direct Unmute, folder row hides without folders and toggles membership,
leave replaces delete for a channel). Captures of both submenus in light and dark
from `ready-chat-list-3` (`QUILL_DEMO_CHAT_MENU_SUBMENU=mute|folders`).
