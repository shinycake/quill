# Quick replies (Business shortcuts)

## What tdesktop does
- `settings/business/settings_quick_replies.cpp`: Settings -> Quick Replies lists each shortcut that has messages as `/name` with its message count. "Add Quick Reply" asks for a name (1-32 characters; letters, digits, `_`, ZWNJ, middle dot, Sinhala block), then opens the shortcut. A name that already exists opens that shortcut. Without Premium, adding shows the Premium preview.
- `settings/business/settings_shortcut_messages.cpp`: the messages of a shortcut are composed, edited and deleted like chat messages. The menu renames the shortcut (a taken name says "This shortcut is already used.") and deletes it, with a confirmation only when it has messages. Limits come from app config (`quick_replies_limit` 100, `quick_reply_messages_limit` 20).
- `chat_helpers/field_autocomplete.cpp`: in a chat with a user who is not a bot, typing `/` lists the shortcuts that have messages, filtered by the typed prefix, each with "N messages". Choosing one calls `processShortcut`, which sends all its messages (`sendQuickReplyShortcutMessages` here). A first "Edit" row opens the settings.

## What changed
- `src/quick_replies.rs`: name validation, `quickReplyShortcut` / `quickReplyMessage` parsing, the four `updateQuickReply*` updates, and the request bodies (`loadQuickReplyShortcuts`, `loadQuickReplyShortcutMessages`, `addQuickReplyShortcutMessage`, `editQuickReplyMessage`, `deleteQuickReplyShortcutMessages`, `setQuickReplyShortcutName`, `deleteQuickReplyShortcut`, `sendQuickReplyShortcutMessages`).
- Settings domain: `SettingsPayload::QuickReplies`, `QuickRepliesState` on `SettingsState` (`state/domains/settings/quick_replies.rs`), three `SettingsPurpose` variants, errors recorded on the state. `Session::command_menu_items` returns the shortcut rows for a private chat with a person (not a bot, not yourself).
- `src/connect/quick_replies.rs`: driver methods. The send uses `RequestPurpose::SendMessageAlbum`, so the `messages` answer is stored as sent messages and a refusal gets the usual send-permission notice.
- `src/ui/quick_replies/`: a Quick Replies dialog (list, name field, shortcut editor with add / edit / delete of text messages, rename, delete with confirmation), registered with `register_dialogs!`. Settings gets a "Quick replies" row. The composer change is a `quick_reply: Option<i32>` field on `CommandMenuItem` and two early returns in the `/` menu pick and click handlers; the first `/` asks for the shortcuts.

## Same as tdesktop
- Saved Messages lists the shortcuts like any private chat with a user (only bots are excluded).
- The `/` list starts with an "Edit Quick Replies" row that opens the dialog; the highlight starts on the first shortcut below it. Tab inserts `/name ` into the composer; Enter and click send the shortcut.
- The shortcuts load once the session is ready (tdesktop preloads them with the dialogs), so the first `/` lists them.
- Limits come from the TDLib options `quick_reply_shortcut_count_max` and `quick_reply_shortcut_message_count_max` (named in the 1.8.68 schema), falling back to tdesktop's 100 and 20 until TDLib reports them.

## Differences and gaps
- The editor uses the kit text area with plain text. Media messages of a shortcut are listed with their caption and can be deleted, but not created or edited; formatting is not applied.
- A name that is already used is caught locally. Quill drops TDLib's error text, so a race with another device shows a generic error.
- Without Premium the dialog explains it and disables editing; there is no Premium preview.
- Tab now also inserts the highlighted bot command (tdesktop's Tab does this for every row).
- TDLib documents `sendQuickReplyShortcutMessages` as needing a Business subscription; not checked against a server.

## Verified
Unit tests for names, parsing, update application, request JSON and the `/` rows (`quick_replies`, `connect::tests::quick_replies`); seven GPUI UI tests (name checks, edit row, Tab, prefix filter) (`--features ui,demo-capture`) for the dialog's screens, name checks and the no-live-session message. gate.sh. Not run against a live account: creating, editing or sending quick replies would change real data and message a person.
