# Edit profile as one form

Sweep item 37 (UX audit finding 7).

## What tdesktop does

`Settings::Information` (`settings/settings_information.cpp`) is a single
form: avatar with a "Set Profile Photo" button, First name / Last name
fields with floating labels, Bio with a remaining-characters counter, one
Save, and Cancel. Save sends only what changed; the photo goes through the
native file dialog and is uploaded at once, outside Save.

## What changed

- One form: labelled First name, Last name and Bio fields (kit `v_form` /
  `field`), a bio counter, and a footer with Cancel and one primary Save.
  Save stays disabled until name or bio differs from the opening values, the
  first name is empty, or the bio is over the limit.
- Save sends only the changed requests (`setName` when either name part
  changed, `setBio` when the bio changed) and closes the dialog. Cancel,
  Escape, the close button and the backdrop discard.
- The bio limit is the TDLib `bio_length_max` option (default 70 until the
  option arrives), kept in `UsersState`.
- The photo path text box is gone. "Set Photo" opens the native file dialog
  (`cx.prompt_for_paths`, as the attachment pickers do) and sends
  `setProfilePhoto`. Remove photo, Camera, emoji and Video avatar flows are
  unchanged.
- The username row is unchanged (its own Check / Set username buttons).
- Logic lives in `src/ui/profile_edit_save.rs`.

## Verified

- Unit tests: the changed-field plan, bio normalisation and counter, and the
  `bio_length_max` option.
- GPUI UI tests (demo-capture): Save gated until a change (a click on the
  disabled Save sends nothing, a click after a change attempts the save),
  one form with no per-field Save buttons, Escape and Cancel discard.
- Light and dark captures of `ready-profile-edit` (demo fixtures).

## Unverified

- Real `setName` / `setBio` round trips against a live account (demo mode has
  no live driver); the request builders are the existing ones.
- Field labels are checked in the captures, not by a UI assertion (kit labels
  are plain text without test identity).
- The native file dialog itself (no automation).
