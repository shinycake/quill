# group-edit-box: the group and channel Edit box

## tdesktop
`boxes/peers/edit_peer_info_box.cpp` opens one Edit box for groups and channels:

- At the top is a round photo button with a camera badge. Its menu (`ui/controls/userpic_button.cpp`)
  offers "Photo or video" from the file dialog, the camera, "From clipboard" (`addFromClipboard`,
  only when the clipboard holds an image when the menu opens) and "Use an emoji". A picked
  picture goes through a crop step (square selection, round preview) before it is set.
  Pictures with a bad aspect ratio or format show a "can't process that type of image" note.
- Under it are the name field (`lng_dlg_new_group_name` / `lng_dlg_new_channel_name`, max 128),
  with an emoji toggle that drops down a `TabbedPanel` in `Mode::PeerTitle` (standard emoji
  only; a pick goes in at the cursor),
  and the multiline description (max 255, with a counter).
- Below are the management rows: type, linked chat, history, permissions (groups only),
  admins, members, removed users, recent actions, stickers, and Delete last.
- Save runs a pipeline: title, then description, then photo. A failed step shows its error
  under the field and keeps the box open. The box closes when every step succeeds.
- Delete asks `lng_sure_delete_group` / `lng_sure_delete_channel` before deleting.
- `basicGroupFullInfo` carries the description, and the info panel shows it for basic groups too.

## What changed in Quill
- The info panel now shows one "Manage group" / "Manage channel" row. It replaces the old
  "Edit title", "Edit description" and "Change photo" rows (and their typed-path prompt), and
  opens the new box in `src/ui/group_edit_box/`.
- The box is a dialog at priority 7700. Its state (editing, saving, failed on a field, saved)
  comes from the pending request ids and `groups.info_edit_errors`, so the UI is a pure
  function of session state.
- The photo button reuses `profile_photo_sources` with a `PhotoTarget::Chat` target. The
  chosen photo stays in the box until Save. The crop step (`src/ui/photo_editor_profile.rs`,
  priority 5444) runs for both the own profile photo and chat photos. It opens over the box
  and hands the box back when it closes.
- `save_group_info` (`src/connect/groups/info_edit.rs`) sends setChatTitle, setChatDescription
  and setChatPhoto in that order. `record_group_info_edit_error` maps each error to its field.
- "From clipboard" reads GPUI's clipboard image (`ClipboardEntry::Image`, which macOS, X11,
  Wayland and Windows all provide) when the menu opens, decodes it off the UI thread and opens
  the same crop step.
- The name field has the emoji toggle. It opens Quill's shared emoji panel under the toggle,
  emoji tab only, with custom emoji left out, and routes picks into the name at the cursor
  (`group_edit_box/title_emoji.rs`). A click off the panel or Escape closes it first, and
  closing or saving the box closes it too.
- The management rows moved from `group_admin_panels` into the box. The Permissions row is
  hidden for channels.
- `Session::chat_can_edit_information` (from main's photo gallery work) now checks group
  membership through the status TDLib reported for the chat or its basic group, as tdesktop's
  `amIn` does, instead of only `Left`/`Banned` on the chat. The Edit box and the gallery's
  "delete the current photo" both use it.
- The envelope parses `basicGroupFullInfo.description`, and the basic group info panel shows it.
- The photo-editor buttons now draw white text on the black layer in both themes. With the
  ghost variant, the text was unreadable in the light theme.

## Deviations
- Removing a group photo is not part of this change.
- The photo is sent in the same save as the text edits rather than right after it is cropped.
- The bad-photo copy names Quill.
- New parity ids (admin-edit-box, admin-photo-crop, groups-photo-sources) are not in
  STATUS.md, because CI refuses STATUS.md edits in a feature PR. They need adding separately.

## How verified
- Driver tests in `src/connect/tests/message_ops/group_edit_box.rs` check the request order
  and JSON, the sticker photo input, the error map, the permission check and the basic group
  description.
- GPUI tests in `src/ui/group_edit_box/ui_tests.rs` check that the box opens filled, an empty
  name keeps it open, Save sends and closes, Escape discards, the crop hands the box back, a
  copied image opens the crop (and a too-long one is refused), and the emoji toggle inserts at
  the cursor and closes on Escape or a click off it.
- The `verify` skill captured the group, channel and basic group boxes, the photo menu, the
  round crop, the box after the crop, and the info panel, in light and dark themes. It also
  captured the emoji panel open and after a pick, and the photo menu with and without an
  image on the system clipboard, plus the clipboard image in the crop and back in the box.
- Not verified: real TDLib saves and errors, the camera, the native file dialog, the
  video avatar path for chats, and the clipboard on Linux and Windows (only macOS ran).
