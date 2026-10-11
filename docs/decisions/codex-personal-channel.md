# Personal channel

## What tdesktop does
- Settings (`settings_information.cpp`, `SetupPersonalChannel`): a "Personal channel" row showing the channel name or "Add a channel". Tapping it opens `internal:edit_personal_channel`, a list of the channels you own; right-click offers "Remove".
- Profiles (`info_profile_actions.cpp`, `makePersonalChannel`): a card under the bio. The label is "Personal channel" plus the subscriber count when it is above one. The card shows the channel name and the latest post, and tapping opens the channel.

## What Quill already had
The profile details card for your own profile already had the row, the chooser (`getSuitablePersonalChats`), "Remove personal channel" (`setPersonalChat(0)`), and the request, parse and apply tests. Other users' profiles showed only the channel name.

## What changed
- `src/personal_channel.rs`: pure card text. Label gains " • N subscribers" above one subscriber (as tdesktop); value is the channel name with the latest post preview under it.
- `src/ui/profile_panels/mod.rs`: the personal channel row uses the card, with the count from `group_member_counts` and the preview from the channel's `last_preview`. `DetailRow.label` became a `String`.

TDLib's `userFullInfo` carries no post id (tdesktop gets one over MTProto), so the channel's own last message stands in for the featured post.

## Verified
Unit tests for the card text, plus the existing request/parse/apply tests. gate.sh. Not run against a live account (the real personal channel must not be changed); no visual capture.
