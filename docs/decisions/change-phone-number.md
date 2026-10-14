# Change phone number

Parity item: `auth-change-number` (left unticked, see below).

## What tdesktop does

Telegram Desktop has no change-number flow. The phone row in Settings and the `tg://settings/edit/change-number` deep link both call `Ui::MakeInformBox(lng_change_phone_error)` (`settings_information.cpp`, `deep_links_settings.cpp`): "You can only change your phone number using mobile Telegram apps. Please use an official Telegram app on your phone to update your number." There is no code entry page, no `account.sendChangePhoneCode` call anywhere in the client.

TDLib's schema marks `phoneNumberCodeTypeChange` "for official Android and iOS applications only", so a third-party api_id cannot be expected to use it either.

## What changed

Quill's counterpart of tdesktop's Settings > My Account is the Edit Profile dialog, so it gets a "Phone number" section (the formatted number and a "Change number…" button) that opens a "Change phone number" box with the same note and an OK button (`change_phone_dialog.rs`). The kit dialog always renders a title, so the box has one; tdesktop's inform box has only text and OK. `tg://settings/edit/change-number` (and `change_number`) opens the same box (`SettingsTarget::ChangePhone`). The profile panel's phone row is unchanged: it only copies, like `info_profile_actions.cpp`. The existing backend (`sendPhoneNumberCode` with `phoneNumberCodeTypeChange`) is untouched and has no UI, matching tdesktop.

## Verified

- Unit tests for the exact string and the deep-link routing; a GPUI UI test opens and closes the box (`demo-capture`). The Edit Profile button and the OK click are not exercised by a test.
- No request is sent by this change; nothing was submitted to the real account.

## Unverified / why the item is not ticked

The README line describes moving contacts, groups and media to a new number, which tdesktop does not do. Quill now matches what tdesktop does (the note). Whether to tick or reword the README item is left to the owner.
