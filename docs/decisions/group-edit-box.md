# group-edit-box: the group and channel Edit box

## tdesktop
`boxes/peers/edit_peer_info_box.cpp` opens one Edit box for groups and channels:

- At the top is a round photo button with a camera badge. Its menu (`ui/controls/userpic_button.cpp`)
  offers "Photo or video" from the file dialog, the camera, and "Use an emoji". A picked
  picture goes through a crop step (square selection, round preview) before it is set.
  Pictures with a bad aspect ratio or format show a "can't process that type of image" note.
- Under it are the name field (`lng_dlg_new_group_name` / `lng_dlg_new_channel_name`, max 128)
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
- The management rows moved from `group_admin_panels` into the box. The Permissions row is
  hidden for channels.
- The envelope parses `basicGroupFullInfo.description`, and the basic group info panel shows it.
- The photo-editor buttons now draw white text on the black layer in both themes. With the
  ghost variant, the text was unreadable in the light theme.

## Deviations
- The title field has no emoji panel, and the photo menu has no "From clipboard".
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
  name keeps it open, Save sends and closes, Escape discards, and the crop hands the box back.
- The `verify` skill captured the group, channel and basic group boxes, the photo menu, the
  round crop, the box after the crop, and the info panel, in light and dark themes.
- Not verified: real TDLib saves and errors, the camera, the native file dialog, and the
  video avatar path for chats.
