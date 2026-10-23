# New Group and New Channel steps

Parity item: `parity:groups-create-steps`.

## What tdesktop does

`GroupInfoBox` (boxes/add_contact_box.cpp) opens first for every kind. It has
a photo button (`UserpicButton`, choose-photo role), the name field and, for
a supergroup or channel, a description. A group's button is Next; the others
say Create. A group also gets a top-bar menu with "Auto-Delete", which opens
`TTLBox` (Save, Cancel, and Disable when a period is set); the default is the
account's default auto-delete period.

- **Group.** Next opens `AddParticipantsBox` ("Add Members", an `N / max`
  counter that counts the viewer, Create and Cancel; Cancel goes back to the
  info box). Create sends `createChat` with the period, uploads the photo once
  the chat exists, and opens it.
- **Channel / supergroup.** Create sends `createChannel` (a supergroup
  carries the period), uploads the photo, makes sure there is an invite link,
  then shows `SetupChannelBox`: Public / Private (Public first), the link
  field with its live check, or for a private channel the invite link that
  copies on click ("Create an invite link" while there is none). Save and
  Skip both continue to `AddParticipantsBox` with Add and Skip; closing that
  opens the channel.
- **Errors.** `NO_CHAT_TITLE` marks the name field, `USERS_TOO_FEW`,
  `PEER_FLOOD` and `USER_RESTRICTED` show their messages, and
  `CHANNELS_TOO_MUCH` opens the channels limit box.

## What changed in Quill

The old single-form create dialog (`ui/dialogs/create_chat.rs`) is gone.

- `quill::new_chat` is the pure model: the kinds, the stages the dialog
  stores, and `view()`, which derives the visible step, busy state and
  failure from the stage plus session facts. `plan_new_chat` trims the
  name, checks lengths, dedupes the members and drops the viewer.
- The driver sends `createNewBasicGroupChat` / `createNewSupergroupChat`
  with `message_auto_delete_time`. The post-apply hook in
  `connect/core` records how the create ended and, once the chat exists,
  sends `setChatPhoto`, `replacePrimaryChatInviteLink` for a supergroup or
  channel with no link, and opens a group.
- `ui/new_chat` is one dialog that walks the steps, so no overlay stacks on
  another. Escape closes the topmost step as tdesktop does: it goes back from
  members or auto-delete, skips the type step, and finishes from the
  subscribers step.
- The photo button, its source menu and the crop come from the Edit box
  (`chat_photo_button`, `PhotoTarget::NewChat`).
- The type step is the merged Type box in a setup mode: only the choice and
  then the link field or the invite link. Its Save sends the username at
  once; a refusal shows under the field and keeps the step open.
- The member picker reuses the contacts picker from the group members
  dialog; its rows now show each contact's userpic, so the Add Members and
  story audience pickers that share it gain them too. The counter writes
  both numbers with thousands separators (`3 / 200,000`).
- A failed create keeps everything typed and shows the reason on the step
  the user came from. `CHANNELS_TOO_MUCH` opens the existing premium limit
  box.

## Deviations

- The brief said members first for a group; tdesktop asks for the name and
  photo first, then members. Quill follows tdesktop.
- Each step keeps a dialog title; tdesktop's info box has none.
- The picker lists contacts only (the reused picker), not recent chats or a
  global search.
- `PEER_FLOOD` shows its message without the "Learn more" link.
- The "could not add these members" box after a group create
  (`failed_to_add_members`) is not built.
- Add is disabled while nobody is picked; tdesktop's does nothing.

## How it was verified

- Unit tests: the step machine and the request planning (`new_chat.rs`),
  and the request JSON (`tests_groups.rs`).
- Driver tests (`connect/tests/new_chat.rs`): channel create, then the photo,
  the invite link, the username and the subscribers; a group created with
  its members and opened; a refused create keeps the reason and sends
  nothing more; an invalid photo is refused before sending.
- GPUI tests (`ui/new_chat/ui_tests.rs`): group Next to members and back,
  an empty name blocked, and a created channel walked through type and
  subscribers into the open chat.
- Screenshot demos `ready-new-group`, `-members`, `-auto-delete`,
  `ready-new-channel`, `-type`, `-private`, `-subscribers`, plus the group
  menu open, captured in light and dark.

Not verified: a real create against Telegram (the demos have no TDLib), the
photo upload on a real account, and the server error paths beyond the
mapped strings.
