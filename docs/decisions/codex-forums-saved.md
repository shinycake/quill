## codex/forums-saved (2026-10-09)

Batch B16: forum view mode and topic extras, Saved Messages sublists and tags.

### What tdesktop does
- **View as Topics / Messages** (`window_peer_menu.cpp` `addViewAsMessages` / `addViewAsTopics`): a forum without tabs shows its topic list; the menu switches to the plain history and back. Strings `lng_forum_view_as_topics` / `lng_forum_view_as_messages`.
- **Topic editor** (`edit_forum_topic_box.cpp`): an icon preview next to the name; clicking it picks the next of six colors (`ForumTopicIcons`: 0x6FB9F0, 0xFFD67E, 0xCB86DB, 0x8EEE98, 0xFF93B2, 0xFB6F5F) while the topic is new and has no emoji; below, "Choose a topic name and icon" and a grid of default icon emoji. Editing sends only the name and the icon emoji, the color is fixed.
- **Topic menu**: Edit Topic, Copy Topic Link (public vs members-only toast), pin / unpin, mention and reaction markers, delete.
- **Saved sublists** (`info_saved_sublists_widget.cpp`, `data_saved_sublist.cpp`): Saved Messages lists the chats its messages came from as dialog rows (avatar, title, last message, date, pin mark), pinned first; "My Notes" for own notes and "Author Hidden" for hidden senders; empty text "You can save messages from other chats here."; the row menu pins / unpins and deletes the chat's saved messages after a confirmation naming the chat.
- **Tags** (`history_view_context_menu.cpp`): "Filter by Tag", "Add Name" / "Edit Name" (12 characters, Premium).

### What changed
- Requests (`telegram/requests/forum_saved.rs`, schema-checked tests): `toggleChatViewAsTopics`, `getForumTopicDefaultIcons`, `getForumTopicLink`, `setPinnedForumTopics`, `readAllForumTopicMentions`, `readAllForumTopicReactions`, `unpinAllForumTopicMessages`, `createForumTopic` / `editForumTopic` with an icon, `loadSavedMessagesTopics`, `getSavedMessagesTopicHistory`, `deleteSavedMessagesTopicHistory`, `toggleSavedMessagesTopicIsPinned`, `getSavedMessagesTags`, `setSavedMessagesTagLabel`, `searchSavedMessages`.
- Envelope: `chat.view_as_topics` + `updateChatViewAsTopics`, `savedMessagesTopic` (+ `updateSavedMessagesTopic`, `...Count`, `updateSavedMessagesTags`, `savedMessagesTags`), unread mention / reaction counts on forum topics.
- State: `Session::chat_view_as_topics` (forum default on, Saved default off), `Session::saved` (sublists, tags, open sublist, tag filter) with its own rows kept in sync with deletes and edits; `forum_topic_icons`.
- Driver (`connect/forum_saved.rs`): all actions above, deduped; the topic link reuses the message-link clipboard path; a 404 on `loadSavedMessagesTopics` means everything is loaded; the tag name is refused without Premium and cut to 12 characters.
- UI: header menu "View as Topics / Messages / Chats" and "New Topic"; topic menus (list rows and tabs) with Edit Topic, Copy Topic Link, Move Up / Down among pinned topics, Mark all mentions as read, Read all reactions, Unpin all messages; the manage dialog now has the topic editor with the icon picker; Saved Messages sublist list, sublist view (back button, read-only, no composer), tags bar (All + chips, context menu Filter by Tag / Add or Edit Name), tag name dialog, delete-chat confirmation.

### Not done
- No drag reorder: pinned topic order uses Move Up / Down (the schema's `setPinnedForumTopics` takes the whole order). Saved sublist pin order (`setPinnedSavedMessagesTopics`) is not wired; pin / unpin only.
- Custom-emoji topic icons are chosen and sent, but topic tabs and rows still draw the colored letter (the picker shows the emoji thumbnail when the file is local, else the emoji character).
- "Remove Tag" on the tag chip, search text combined with a tag, and `getSavedMessagesTopicMessageByDate` are not implemented. Tag filter pages do not track reactions changed elsewhere beyond edits already routed through `edit_loaded_message`.
- Forum topics as a second column and enabling topics on an existing group are other batches.

### How verified
- Recorded-JSON tests: request shapes against the schema lines, envelope parsing (topics, types, hidden name, tags), reducer (view mode defaults, sorting, titles, history paging, deletes, tag search, 404, pin order, mark counts, default icons) and driver flows (toggle, link to clipboard slot, pin order, read-all, icon create / edit, sublist load / open / page / pin / delete, tag rename Premium gate and filter).
- Demo `ready-forums-saved` (`QUILL_DEMO_FORUMS_SAVED_VIEW=sublists|sublist|tag|editor`), captured in light and viewed. No live account was used; nothing was sent or deleted.
