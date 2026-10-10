# Settings account: help links, version footer, call exceptions, archive switches

Cluster `settings-account`. Four items land and nine are skipped.

## What Telegram Desktop does

- `settings_main.cpp` ends the Settings list with Telegram FAQ, Telegram Features and Ask a Question. Ask a Question first shows a note that support is run by volunteers, with "Go to FAQ" and "Ask a Volunteer"; the second calls `help.getSupport` and opens the chat.
- `settings_privacy_security.cpp` has an "Archive and Mute" block under Privacy. The archive box in `settings_advanced.cpp` holds the three switches, including "chats from folders".
- Who can call me and Peer-to-peer calls are normal privacy rules with Always allow and Never allow lists.

## What changed

- Settings list: Telegram FAQ, Telegram Features, Privacy Policy and Ask a Question rows, then `Quill <version>` (from `quill::version::APP`) with a "What's new" button that opens the releases page. Ask a Question is a Settings subpage that mirrors the volunteer note. "Ask a Volunteer" sends `getSupportUser`; the `user` answer parks the id in `Session::support_user_ready` and the driver then calls `createPrivateChat`, which opens the chat.
- Privacy: `PrivacySettingKey` gained `AllowCalls` and `PeerToPeer`, so both rows go through the same editor as the other rules and get Always/Never lists. Choosing Everybody, Contacts or Nobody keeps the lists. The older `call_privacy_*` fields are mirrored from the rule state, and the Calls settings page now writes through the same path instead of replacing the rules without exceptions.
- Privacy overlay: an "Archive and Mute" block with the three archive switches inline.
- Pure logic lives in `src/settings_account.rs` (URLs, version line) with tests; driver tests cover the support flow and the call rules.

## Skipped

- settings-session-details: the details box exists, but TDLib has no method to rename a session, so the item cannot match.
- settings-phone-suggestion: needs a change-number flow, which Quill does not have.
- settings-main-profile-tab: Quill's profile has no tab strip to hang "Set as Main Tab" on.
- settings-profile-music, settings-photo-sources, settings-personal-channel, settings-name-color, settings-emoji-status-menu, settings-bots-websites: each is a feature of its own (pickers, uploads, colour catalogues, mini-app permissions) and did not fit this PR.

## Verified

- Unit and driver tests (`settings_account`), gate.
- Demo captures in English: `ready-settings-help`, `ready-ask-question`, `ready-privacy-calls`, `ready-privacy`.
- Not verified: the live `getSupportUser` round trip against Telegram, and opening links in a browser.
