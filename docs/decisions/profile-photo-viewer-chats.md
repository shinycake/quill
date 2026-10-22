# Profile photo viewer for groups, channels and users

## What tdesktop does

`OverlayWidget` (media_view_overlay_widget.cpp) shows a peer's photos in
two ways:

- Users go through `UserPhotos`. The personal photo ("Photo set by you")
  comes first, then `getUserProfilePhotos`, and the fallback ("public")
  photo comes last.
- Groups and channels go through `SharedMediaWithLastSlice` over
  `SharedMediaType::ChatPhoto`. That is the chat's photo-change service
  messages, with the current photo added as an "isolated" first entry when
  the newest change set a different photo. The counter's total is
  `fullCount + isolated`.

The header (`updateHeader`) reads "Photo {n} of {amount}". A lone photo is
named "Profile Photo", "Group Photo" or "Channel Photo". A user's last
photo is "Public Photo" when it is the fallback photo.

The viewer offers these actions:

- **Delete**
  - Your own photos: "Do you want to delete this photo?", then
    `PeerPhoto::clear`.
  - A chat's current photo, when you may edit its info: the same box.
  - An earlier chat photo: its service message goes through
    `DeleteMessagesBox`.
  - The viewer closes before the box opens.
- **Set as Main**
  - Your own earlier photos.
  - A chat's earlier photos, when you may edit its info.
- **Report**
  - Shown for someone else's photo, but never for the notifications
    account or the personal photo.
  - Shown for a chat's photo only when you cannot edit its info.

## What changed

**Opening the viewer.** The avatar in every info panel now opens the
viewer:

- users: `user_info_panel`;
- supergroups and channels: `supergroup_identity`;
- basic groups: `basic_group_info_panel`.

A group or channel opens on the current photo as soon as it is known. The
history then fills in behind it.

**Where the photos come from.**

- The current photo comes from `supergroupFullInfo.photo` /
  `basicGroupFullInfo.photo`. It is now parsed and kept in
  `users_state.chat_current_photos`.
- The history comes from `searchChatMessages` with
  `searchMessagesFilterChatPhoto`, in pages of 100, kept in
  `users_state.chat_photo_history`.
- Both caches are dropped when `updateChatPhoto` changes the photo.
- A change whose message was deleted (a history tombstone) leaves the
  gallery.

**Users.** The list now pages past 100 by offset. `userFullInfo.public_photo`
is parsed and added as the last entry once every page is in. It also
counts in the total before then.

**Building the gallery.** `Session::photo_gallery` (src/state/photo_gallery.rs)
builds one `PhotoGallery` for either kind of owner. It is a pure function
of session state. The UI derives the header and the actions from it.

**Paging in the viewer.** It asks for the next page
`VIEWER_PRELOAD_AHEAD` items before the end, and `MediaViewer::extend_photos`
appends pages. The counter uses the total, not the loaded count.

**Actions.** These follow the tdesktop rules above.

- Delete:
  - your own photo: `deleteProfilePhoto`;
  - a chat's current photo: `setChatPhoto` with no photo;
  - an earlier chat photo: `deleteMessages` on its
    `messageChatChangePhoto`. TDLib's `messageProperties` gate it once
    they arrive, the same gate as the message menu.
- Set as Main:
  - your own photo: `inputChatPhotoPrevious`;
  - a chat's photo: see "Deviations".
- Report: `reportChatPhoto`. A user's photos are reported in the private
  chat with them.

The delete box is a kit dialog (`DialogKind::PhotoDelete`) with no title,
like tdesktop's `MakeConfirmBox`.

## Deviations

- **Chat Set as Main.** It uses `setChatPhoto` with
  `inputChatPhotoStatic(inputFileId)` of the photo's largest size.
  TDLib's `inputChatPhotoPrevious` only accepts the current user's own
  photos. TDLib sends an already-uploaded file as `inputChatPhoto` with no
  new upload, which is what tdesktop's `PeerPhoto::set` sends.
- **Delete box for an earlier chat photo.** It has no "delete for
  everyone" checkbox. The choice comes from `can_be_deleted_for_all_users`,
  or before that arrives, from whether the message is yours. Supergroups
  always delete for everyone.
- **"Community Photo".** Not used: communities are not TDLib chats.
- **Video avatars (e7).** Not in this change. Gallery entries keep the
  photo id and file ids, so an animation can be added to `ProfilePhoto`
  later.

## How it was verified

**Tests.**

- Core tests:
  - src/state/tests/photo_gallery.rs: the full info photo, history paging,
    the isolated current photo, tombstones, cache drops, action rules,
    user paging with the public photo, and a failed later page.
  - src/state/photo_gallery_tests.rs: the pure gallery builders.
- Request JSON tests: `search_chat_photos`, `set_chat_photo_from_file`.
- Recording-sender driver tests (src/connect/tests/profile_photos.rs):
  full info plus search, paging by `from_message_id` and by offset,
  set-main rights, deleting a listed change, and report chat ids.
- GPUI test (photo_viewer_tests.rs): clicking the group avatar in the
  `ready-info-group-photos` demo opens "Photo 1 of 3" on the current
  photo, and stepping shows "Photo 2 of 3" with Set as Main.

**Captures.** The verify skill captured the same demo in light and dark:

- the current photo;
- the older photo;
- the delete box for the current photo ("delete this photo");
- the delete box for an earlier photo ("delete this message").

The captures could not show real TDLib answers, paging past 100 against a
server, or the server applying the delete, set-main and report requests.
