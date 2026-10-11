# Main profile tab

## What tdesktop does

`main_profile_tab` lives on the user and channel (`data_user.cpp`,
`data_channel.cpp`, `data_peer.cpp`). The profile tab host
(`info/profile/tabs/info_profile_tabs_host.cpp`) opens a profile on that tab
when the peer picked one, and the owner changes it from the tab strip.

## TDLib 1.8.68

`ProfileTab` (`profileTabPosts`, `Gifts`, `Media`, `Files`, `Links`, `Music`,
`Voice`, `Gifs`), `userFullInfo.main_profile_tab`,
`supergroupFullInfo.main_profile_tab`, `setMainProfileTab` (own profile) and
`setSupergroupMainProfileTab`. All present in `schema/td_api.tl`.

## What changed

- `src/profile_tab.rs`: `ProfileTab` with parse, serialize, labels and the
  mapping to Quill's shared-media gallery tabs. Posts and Gifts have no
  gallery tab in Quill, so they open the default (Media).
- `userFullInfo` parse keeps `main_profile_tab` in `UserProfileExtras`.
- `setMainProfileTab` request (`telegram/requests/main_profile_tab.rs`,
  `connect/main_profile_tab.rs`), purpose `SetMainProfileTab` in the users
  domain; a refusal shows the usual "could not save the change" notice. The
  new value arrives through `updateUserFullInfo`.
- Opening shared media for a private chat starts on the user's main tab
  (`SharedMediaState::open_for_tab`, `Session::main_profile_gallery_tab`).
- Your own details card has a "Main tab" row that opens a chooser dialog.

Not done: channels and groups (`supergroupFullInfo` parse and
`setSupergroupMainProfileTab`) and Posts/Gifts tabs, which need profile
surfaces Quill does not have yet.

## Verification

Unit tests: tab parse and round trip, request JSON, driver sends the request,
`updateUserFullInfo` stores the tab and the gallery opens on it, tabs without
a gallery fall back to Media, reopening keeps the current tab, refused request
shows the notice. The real account's setting was not changed.

Hotspot-change: `UserProfileExtras` in `envelope_types.rs` gets one field.
