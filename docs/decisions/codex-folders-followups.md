# Folders follow-ups: new chats bar, tag colours, tab menu, limit boxes

## What tdesktop does
- `ui/chat/more_chats_bar.cpp` + `data/data_chat_filters.cpp`: a shared folder (one made from a chat-list link) shows "You can join N new chats / Click here to view them" above its list. Updates are asked at most once per update period (default an hour). Click opens the "Add chats to folder" box; the bar's close sends an empty list (`HideChatlistUpdates`).
- `boxes/filters/edit_filter_box.cpp`: "Folder color in chat list" row of 7 colours plus "No Tag" (-1). Shown when tags are on or the user is not Premium; a free user's click opens the Premium preview. Rows draw the folder name in capitals in that colour, only for coloured folders, and not the folder being viewed (`Entry::hasChatsFilterTags`).
- `window/window_filters_menu.cpp`, `ui/widgets/chat_filters_tabs_strip.cpp`: right-click on a tab: Edit folder, Mark as read (only when unread), Remove. The All tab: Mark all as read, folder settings. Remove asks (`lng_filters_delete_sure` if the user has links, else `lng_filters_remove_sure`) and for a shared folder offers the chats to quit, all ticked ("Remove Folder and Keep Chats" when none).
- `boxes/premium_limits_box.cpp`: Limit Reached boxes for folders, chats per folder (include / exclude), invite links and shared folders, with the Premium value; errors `FILTER_INCLUDE_TOO_MUCH`, `CHATLISTS_TOO_MUCH` open them.

## What changed
- `src/folder_limits.rs` (pure): limits from TDLib options (`chat_folder_count_max`, `chat_folder_chosen_chat_count_max`, `chat_folder_invite_link_count_max`, `added_shareable_chat_folder_count_max`, `chat_folder_new_chats_update_period`) and `getPremiumLimit`; box texts; error classification; tag gating; row tag chips; bar text.
- New requests: `getChatFolderNewChats`, `processChatFolderNewChats`, `readChatList` for `chatListFolder`, `getPremiumLimit`. `TdError` keeps a `limit_hint` (never the message text).
- Driver: new chats are asked when a shared folder opens and from the poll loop, rate-limited by the option; joining or hiding clears the bar at once.
- UI (`src/ui/folder_extras.rs`): the bar and join dialog, the tab menu (top strip tabs and the left column), the limit dialog, the colour picker in the folder editor, coloured tag chips on rows. The delete dialog now lists the suggested chats (all ticked, Select/Deselect all) for shared folders only.
- Limits are checked before New folder, Add recommended, saving the editor (included/excluded chats) and creating a link; failed requests that name a limit open the same box.
- Top strip: an icons-only tab keeps the folder name as its label so the kit overflow menu names it; "text and icons" now draws icon plus text (the kit draws an icon tab as the icon alone).

## Not done / differences
- "Mute all" in the folder menu: tdesktop has none, so it is not offered. "Share folder" is a Quill addition next to Edit.
- Premium upsell opens `t.me/PremiumBot`; there is no in-app Premium preview.
- Secret chats are not counted separately for the chosen-chat limit.
- The exact server/TDLib texts for limit errors are matched loosely (`TOO_MUCH`, "too many", "maximum number", "exceeded"); unverified without a live account.
- Pinned chats still have no per-folder UI.

## Verified
Unit tests: option and `getPremiumLimit` folding, limit counts, box texts, error classification, tag gating and row chips, bar text; request shapes; envelope `limit_hint`; reducer (new chats cached, limit error opens the box instead of an error line); driver (period, dedupe, join clears, read-folder only when unread, premium limit once). Demo captures (English fixtures, light): `ready-folders-tags`, `-tag-color`, `-menu`, `-new-chats`, `-new-chats-join`, `-limit`, `-delete`. No live account.
