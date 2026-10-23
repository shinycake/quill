# Edit box rows, counts and toggles

## What tdesktop does

`Controller::fillManageSection` (`boxes/peers/edit_peer_info_box.cpp`)
builds the manage list in a fixed order:

1. type
2. discussion
3. direct messages
4. chat history
5. topics
6. color
7. auto-translate
8. signatures, with a divider
9. reactions
10. welcome
11. permissions
12. invite links
13. administrators
14. members or subscribers
15. join requests
16. removed users
17. recent actions
18. affiliate programs
19. stickers
20. delete

Each count is shown only when it is above zero (`ToPositiveNumberString`).
Permissions show "on/total" (`RestrictionsCountValue`). Reactions show
All, Off or a number (`ReactionsCountValue`).

Three settings wait for Save: `saveAutotranslate`, `saveSignatures` and
`saveHistoryVisibility`. A basic group is upgraded first.

Auto-translate is locked below `channel_autotranslation_level_min`.
Tapping it while locked opens the boost box
(`lng_boost_channel_title_autotranslate`). "Show authors' profiles"
appears only while "Sign messages" is on, and the divider text under the
two switches changes with them.

Chat history opens `EditPeerHistoryVisibilityBox`, a radio box. Basic
groups get the legacy "Hidden" text. The box asks before it drops an
unsaved change.

## What changed

- `quill::group_manage` holds the pure rules:
  - the reactions value
  - positive counts
  - the auto-translate lock and click outcome
  - the signature dependency
  - the history labels
  - the custom-reaction level check (d6)
- `Session::manage_rows` decides which rows appear and their counts, using
  tdesktop's gates.
- The Edit box rows now follow tdesktop's order and show counts.
- The settings dialog lost its own main page. It now only hosts the pages
  those rows open:
  - reactions
  - discussion
  - invite links
  - join requests
  - administrators
  - recent actions
  - upgrade confirm
- The info panel no longer repeats the invite link, join request,
  administrator and recent action sections.
- Auto-translate, signatures and history visibility are staged in the box.
  Save sends them with the type and text edits.
- A locked auto-translate switch opens the existing boost box with
  auto-translate copy.
- New history box: a kit `RadioGroup` with Save and Cancel. Cancel asks
  first when the choice changed.
- Hard-coded English moved to `lang/en/group_manage.strings`:
  - the upgrade confirm
  - the secret-chat passcode hint
  - profile photo modes and their done notes
  - report reasons
  - the unofficial-client warning
  - the broadcast-group and recent-actions texts
- Lang keys that the removed paths used are deleted.

## Differences from tdesktop

- Invite links, administrators, join requests and recent actions open as
  pages of a dialog, not as sections.
- Escape is turned off in the history box (its Cancel asks first), where
  tdesktop sends Escape through the same prompt.
- Topics and hidden members still apply right away.
- The removed users row also needs the restrict right.
- If one of the staged switches fails to send, the box shows an error. It
  does not roll back the other edits (tdesktop `cancelSave`).
- The boost box opens with the cached level, then asks TDLib for a fresh
  one.
- Quill's reactions editor cannot pick custom emoji. So the level text
  appears only when the current setting already has more custom reactions
  than the boost level allows.

## Verification

- Unit tests: `group_manage_tests.rs` and the permissions count test.
- GPUI tests in `group_edit_box/ui_tests.rs`:
  - author profiles appear only while signing
  - locked auto-translate opens the boost box
  - the history box hands its choice to Save
- `/verify` captures of the group and channel Edit boxes in light and
  dark, the channel box scrolled down, the history box and the boost box.

A demo has no live TDLib, so these were not seen working:

- the staged switches reaching TDLib on Save
- the basic group upgrade
