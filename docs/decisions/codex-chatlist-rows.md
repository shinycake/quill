# Chat list rows: call badge, archive hint, folder picker and toast

## What tdesktop does
- `dialogs/dialogs_row.cpp`: a group or channel with a running video chat (`ChannelHasActiveCall`, the `CallNotEmpty` flag) paints a badge in the avatar's corner.
- `boxes/about_box.cpp` (`ArchiveHintBox`): the Archive context menu has "How does it work?". It opens a box titled "This is your Archive". The text depends on whether new messages unarchive unmuted chats, with a link to the archive settings. Three sections follow (Archived Chats, Hiding Archive, Stories) and a "Got it" button.
- `boxes/filters/choose_filter_box.cpp`: after adding a chat to a folder, or removing it, a toast reads "{chat} added to {folder} folder" or "{chat} removed from {folder} folder".
- `boxes/filters/edit_filter_box.cpp`, `edit_filter_chats_list.cpp`: the folder editor has "Included chats" and "Excluded chats" sections, each with its chats, an "N chats" count and an "Add Chats" button. The button opens a picker with search and an "N / limit" counter. Going past the limit opens the limit box.
- `info/profile/info_profile_actions.cpp`: right-clicking the username row offers Copy Username (`lng_context_copy_mention`), Copy Link and Share.
- `window/window_peer_menu.cpp`: the row menu also carries View profile, the mark-as-read entries for mentions, reactions and poll votes, and Export chat history.

## What was already there
- Emoji status after the row title (`TitleBadge::EmojiStatus`, still image, resolved through `getCustomEmojiStickers`). It had no capture, so this change adds one.
- `ChatSummary::video_chat` (`updateChatVideoChat`), but nothing drew it on the row.
- User profile rows already copied the username on click and in a right-click "Copy Username" menu.

## What changed
- `src/peer_badge.rs`: `shows_call_badge` (call exists and has participants). `src/ui/chat_row.rs`: `with_call_badge` draws a green microphone badge ringed in the sidebar color, bottom-right of the avatar.
- `src/chatlist_archive.rs` + `src/ui/archive_hint.rs`: the hint box and a "How does it work?" item in the Archive menu. The wording follows `keep_unmuted_chats_archived` (the inverse of tdesktop's unarchive-on-new-message). "Tap to change" opens the archive settings.
- `src/folders.rs` (`folder_membership_toast`) and `src/ui/folders.rs`: the folder picker shows the toast on add and on remove.
- `src/folder_picker.rs` + `src/ui/folder_picker_ui.rs`: the editor's single "In / Out" list became the two sections and a picker with search and the counter. Limits come from `FolderLimits` (chosen-chat limit, 100 or 200 by default).
- `src/chatlist_menu.rs` + `src/ui/message_actions.rs`: row menu gains View profile (group, channel info), "Mark all mentions as read", "Read all reactions", "Read all poll votes" and "Export chat history". The driver got `read_all_chat_unread_markers(chat_id, kind)`, so the read entries work for chats that are not open.
- `src/ui/profile_panels.rs`, `src/ui/group_panels.rs`: the username row menu offers Copy Username and Copy Link, for users and for groups and channels (their username line was plain text).

## Not done
- Row menu: "open in new window" (Quill has no per-chat windows) and "Report" with reasons (the row still sends the plain spam report). Because of that, `chatlist-row-menu-extras` is not claimed.
- Alphabetical section index (`chatlist-contacts-index`): the contacts list has no section headers, and the bar needs scrubbing and the fisheye effect. It is a separate piece of work.
- Share in the username menu.
- The call badge is static; tdesktop animates it while someone speaks.
- The toast shows when the request is sent, not when TDLib confirms it.

## Verified
- Unit tests: `shows_call_badge`, archive hint wording, folder toast text, picker search, counters and limit gating, row menu extras.
- Demo captures (English fixtures, light), all viewed: `ready-chat-badges` (call badge and emoji status), `ready-archive-hint`, `ready-folders-chats`, `ready-folders-chat-picker`, `ready-folders-toast`, and `ready-chat-list` (View profile in the row menu).
- The username context menus are native popups and cannot be captured in the demo harness, so they are unverified visually.
- No live account. The gate's UI clippy step fails on files this change does not touch (new lints from the current toolchain); core clippy, formatting and both test suites pass.
