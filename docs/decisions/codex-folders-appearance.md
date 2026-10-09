# Folders: icons, sharing, recommended, left column

## What tdesktop does
- `ui/filter_icon_panel.cpp`: a 6-per-row grid of 30 icons (the schema's `chatFolderIcon` names); no pick means the default icon computed from the rules (`ComputeDefaultFilterIcon`).
- `boxes/filters/edit_filter_links.cpp`: Share Folder lists the user's invite links (name, chat count, copy/edit/delete with a "delete this link?" confirmation) and a create form with a name and the shareable chats ticked. A folder with type rules or excluded chats cannot be shared (`GoodForExportFilterLink`).
- `settings/sections/settings_folders.cpp`: recommended folders with an Add button, "Tabs view" (left/top) and "Tabs appearance" (default, text, text and icons, icons).
- `window/window_filters_menu.cpp`: the vertical column, icon over a short name, an accent bar on the active one, an edit button at the foot.
- `addlist` links: a box listing the chats to join (all ticked, select/deselect all) and an Add button; an existing folder reads "Add chats to folder" / "Folder already added".

## What changed
- `ChatFolderSpec` carries `icon_name` and `color_id` (an edit used to reset the icon and tag colour); `ChatFolderInfo` carries `is_shareable` / `has_my_invite_links`.
- New requests: `getRecommendedChatFolders`, `getChatsForChatFolderInviteLink`, `getChatFolderInviteLinks`, `create/edit/deleteChatFolderInviteLink`, `checkChatFolderInviteLink`, `addChatFolderByInviteLink`, and `getChat` for unknown chats of an addlist link.
- `internalLinkTypeChatFolderInvite` now opens the Add folder dialog (was "not supported").
- UI: icon picker in the folder editor; icons on manage rows and tabs; Share dialog; recommended section and tab settings in Folders; left column (`folder_tabs.rs`) that sits beside the chat list (the list keeps its width, the column adds 52-68px).
- Prefs: `folder_tabs_view` / `folder_tabs_mode` in `appearance_prefs.json`.

## Not done
- The "N new chats" bar (`getChatFolderNewChats`) and the share-box QR/share entries.
- Folder tag colour picker, folder context menu, Premium limit boxes.
- Interface scale and Telegram wallpapers: see the appearance notes (separate PR).
- Icons-only tabs on the top strip list unnamed entries in the overflow menu (kit TabBar uses the label there).

## Verified
Unit tests (envelope parse, request shapes, reducer routing, driver dedupe/optimistic delete, deep-link route, tab slots, icon rules); demo captures in light and dark: `ready-folders-share`, `ready-folders-sidebar`, `ready-folders-add-link`, `ready-folders-icons`, `ready-folders-manage`. No live account: real server answers (limits, error texts) are unverified.
