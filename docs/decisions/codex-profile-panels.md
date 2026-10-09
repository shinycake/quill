# Profile and contact panels (B10)

## What tdesktop does
- `info_profile_actions.cpp`: profile rows (bio, phone, username, link, birthday, note) copy on click and have a right-click "Copy" menu with a toast; "Share this contact" shows when the phone is known; common groups and similar channels are lists on the profile.
- `edit_contact_box.cpp`: first/last name, private note, and "Share my phone number" only when the server asks for a contacts exception (`NeedContactsException`); saved through `contacts.addContact`.
- `window_peer_menu.cpp` `PeerMenuShareContactBox`: pick a recipient, confirm "share contact with X", send a contact message (no confirm for Saved Messages).
- Settings: birthday box (day/month/year, year optional, remove) and personal channel picker (own channels, remove).

## What changed in Quill
- Requests: `setBirthdate`, `getSuitablePersonalChats`, `setPersonalChat`, `setUserNote`, `getGroupsInCommon`, `getChatSimilarChats`, `getUserProfilePhotos`, `setProfilePhoto` + `inputChatPhotoPrevious`, `sendMessage` + `inputMessageContact`; `addContact` now carries the note and `share_phone_number`.
- State: `userFullInfo` keeps `personal_chat_id`, `note`, `need_phone_number_privacy_exception`; `Session::profile_chat_lists` (loading/loaded/failed, retry) and `user_profile_photos`.
- UI (`src/ui/profile_panels.rs`): copy rows (tap and right-click) for bio, phone, username, `t.me` link, note, and the name; birthday and personal channel rows (own profile opens editors, others open the channel); Edit contact and Share this contact rows; groups in common and similar channels lists; one `ProfilePanel` dialog for edit contact, birthday, personal channel and share contact (with a confirm step).
- Profile photo gallery: clicking an avatar opens the photos in the media viewer (`ViewerSource::Profile`, message-bound actions hidden); "Set as main photo" on your own earlier photos.
- Editing only the note uses `setUserNote`; other edits use `addContact` with the current note so it is not cleared. The note is plain text, so entities on an existing note are dropped when it is edited.

## Not done
- Add bot to group as admin (needs the add-to-group chooser and rights editor), personal photo for a contact, photo report / "photo set by you", paging of groups in common beyond 100.
- The birthday form uses text fields (not tdesktop's pickers).

## Verification
- Unit tests: request shapes, reducer (lists, gallery, errors, `userFullInfo` fields), connect driver (share contact, dedupe/retry, birthday validation), form validation, viewer profile source.
- Demo capture `ready-profile-panels` (`QUILL_DEMO_PROFILE=contact|self|edit-contact|birthday|channel|share|gallery|similar`); contact, edit-contact, gallery and similar viewed. No live account: network round trips unverified.
