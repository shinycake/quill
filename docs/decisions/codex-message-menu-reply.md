# Message menu: retract vote, fact check, notification tones, reply timecode

## What tdesktop does
`history/view/history_view_context_menu.cpp` builds the message menu from small `Add*Action` helpers.
- Polls (`AddPollActions`): "Retract vote" on an open, non-quiz poll you voted in while revoting is allowed, then "Stop Poll".
- Fact check (`AddFactcheckAction`): "Add Fact Check" or "Edit Fact Check" when `factchecks().canEdit(item)`. The box saves with `setMessageFactCheck`; an empty text removes it.
- Notification tones (`AddSaveSoundForNotifications`): "Save for Notifications" on songs and voice messages within the ringtone limits (100 KiB, 5 s, 100 saved by default), skipped for self-destructing messages and protected chats. It sits between Show in Folder and Save As.
- Reply with timecode (`AddTimecodeAction`): while a voice message plays, a row under Reply shows the position. Clicking it replies and inserts the position at the cursor, padded with spaces.
- Copy Post Link versus Copy Message Link by chat kind, then a toast: public links say so, private ones say "This link will only work for members of this chat."
- Copy Card Number comes from the link under the cursor.
- Go To Message is offered in the pinned list and chat previews. Reply options (Update Quote, Do Not Reply, Show in Chat, Reply in Another Chat) and the saved-tag menu live in `history_view_draft_options.cpp` and `ShowTagMenu`. Edit Image, Edit Video and Edit Cover are in the edit-caption box's preview menu.

## What changed
- `MessageActions` now reads `can_set_fact_check` and `can_be_replied_in_another_chat` from `messageProperties`.
- Retract vote: `poll::can_retract_vote`, `retract_poll_vote` on the driver (`setPollAnswer` with no options, optimistic clear), menu row above Stop Poll.
- Fact check: the message keeps the plain text of `fact_check` (`MessageExtras::fact_check`), `updateMessageFactCheck` updates it, and a dialog (`src/ui/fact_check.rs`) saves through `setMessageFactCheck`. Entities are not sent.
- Notification tones: `MediaAction::SaveForNotifications`, limits in `ToneLimits` (read from the `notification_sound_*_max` options when TDLib sends them), `addSavedNotificationSound` with `inputFileId`.
- Reply with timecode: shown on the voice message that is playing, inserts the timecode through the composer.
- Link note moved into `message_menu::link_copied_note` so it is tested. Card number copy got a test; no code change was needed.
- Saved tags on a message bubble get a right-click menu (Filter by Tag, Add or Edit Name, Remove Tag). In-chat search hits get Go To Message.
- Added the Tag, TagX and BellPlus icons to the bundled set (Tag was missing for the tag chips).

## Not done
- Reply in Another Chat and the reply options popover: sends only know same-chat replies (`SendReply` has no chat), and Update Quote needs a quote picker.
- Edit Image, Edit Video, Edit Cover: there is no editor for a sent photo, no trimmer and no cover picker. `editMessageMedia` replacement already exists.
- Emoji pack footer: not started.
- Go To Message in the pinned list: Quill has no pinned list view yet. It exists on shared media and search hits.
- Saved tag menu: the custom-emoji pack footer is missing and the popup could not be exercised in a capture.
- The bubble does not show the fact check text yet.

## How verified
Gate passes (fmt, clippy, core and UI tests). New tests cover the item rules (tone limits, timecode, retract, wording), request shapes, the driver paths and the `messageProperties` rights. `ready-message-menu` captures with `QUILL_DEMO_MENU` set to voice-tone, voice-timecode, poll-retract, card-number, fact-check-add, fact-check-edit and fact-check-dialog were viewed. Nothing ran against a live account.
