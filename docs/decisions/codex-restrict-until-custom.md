# Restrict until a custom date, and the exceptions list

## What tdesktop does
- `EditRestrictedBox` (`boxes/peers/edit_participant_box.cpp`) offers Duration: Forever, 1 day, 1 week, Custom date and time (`ChooseDateTimeBox`, up to 366 days). Editing an existing restriction adds its old end date as a choice and starts from the member's own rights.
- "Exceptions" (`edit_peer_permissions_box.cpp`, `AddBannedButtons`) opens the Restricted participants list (`ParticipantsBoxController`, `Role::Restricted`). Tapping a row edits that member's rights in the same box. It is not a separate per-right list; "per-right exceptions" are the member's switches in that box.

## State before this change
Quill already had the duration choices with a custom date picker, the Restricted tab (`supergroupMembersFilterRestricted`), the `chatMemberStatusRestricted` request and the parsed per-member `restriction`. Missing: the list did not show the end date, and editing an exception started from the chat defaults on Forever.

## What changed
- `src/moderation_exceptions.rs`: forever/limited rule, the starting Duration for an edit, and the row status text (unit tested).
- `src/ui/restricted_exceptions.rs`: finds a member's loaded restriction; row shows "restricted until <date>".
- Restrict dialog opens with the member's own rights and their end date in the custom picker.

## Verified
`gate.sh` (GATE OK), unit tests above. Not visually captured: no demo shows a restricted member with a date, and demo kinds were not added.
