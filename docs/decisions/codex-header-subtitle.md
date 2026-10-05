# codex/header-subtitle — member and online counts in the chat header

- The group/channel header subtitle only had a count after `getSupergroupFullInfo` answered,
  and basic groups never had one.
- TDLib already sends counts on the base objects. Quill now keeps `updateSupergroup`'s
  `member_count`, parses `updateBasicGroup`, and parses `updateChatOnlineMemberCount` (sent for
  opened groups).
- `Session::group_member_counts` prefers full info and falls back to the base object.
- The header reads "12.4K subscribers" for channels and "1.2K members, 56 online" for groups
  (online is shown only when more than one). Singular nouns for a count of 1. Basic groups get
  the same subtitle.
- Test: `tests/group_counts.rs`.
