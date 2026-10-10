# Profile panels, round 2

## What tdesktop does
- `info/profile/info_profile_inner_widget.cpp` (`AddUnofficialSecurityRiskWarning`): when `userFullInfo.uses_unofficial_app` is set, a divider note under the cover says the user runs an unofficial client and messages may be less secure.
- `info/profile/info_profile_top_bar.cpp`, `ui/controls/userpic_button.cpp`, `boxes/peers/edit_contact_box.cpp`: for a contact, "Set Profile Photo" (only you see it), "Suggest Profile Photo", and "Reset to Original", each behind a confirmation text.
- Media viewer: the contact's personal photo is labeled "Photo set by you"; other people's profile photos have a "Report" action.
- Birthday box: the privacy setting for the date of birth is reachable from the form.

## What changed
- `userFullInfo` now keeps `uses_unofficial_app` and `personal_photo`. The personal photo files are cached and `Session::profile_gallery` puts that photo first in the gallery (no duplicate when `getUserProfilePhotos` already lists it).
- Profile panel: a warning card for users on unofficial clients; "Set Profile Photo" (contacts), "Suggest Profile Photo" (non-bot users), "Reset to Original" (when a personal photo exists). The first two open the native file picker (JPEG, PNG, WebP), then a confirmation dialog with tdesktop's wording.
- Gallery viewer: header reads "Photo set by you" for the personal photo; a Report action (header icon and right-click menu) closes the viewer and opens a reason list that sends `reportChatPhoto`.
- Birthday form: a "Choose who can see your birthday" link opens Settings > Privacy directly on the date-of-birth rule.
- Requests: `setUserPersonalProfilePhoto`, `suggestUserProfilePhoto`, `reportChatPhoto`, all present in `schema/td_api.tl`.

## Not done
- Gift and More buttons in the action row, add bot to group, inline members list, media tabs for Photos/Videos/Polls/Stories/Gifts/Saved Music, shared media calendar, topic info panel. None of these are claimed.
- No cropper before setting a photo; the image is sent as picked.
- `reportChatPhoto` uses the user id as the chat id. If TDLib has not loaded that private chat, the request fails and the notice says so.

## Verification
- Unit tests: request shapes, report reasons, personal photo modes and file filter, unofficial warning text, reducer parsing, gallery merge, error notices.
- Demo captures (`ready-profile-panels`, `QUILL_DEMO_PROFILE=contact|gallery|birthday|report|personal-photo`) were viewed: warning card, "Photo set by you 1 of 4" with the report flag, birthday privacy link, report reasons, set-photo confirmation.
- No live account: the network round trips are unverified.
