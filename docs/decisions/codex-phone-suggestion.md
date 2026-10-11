# Phone number suggestion

Parity item: `settings-phone-suggestion`.

## What tdesktop does

`SetupValidatePhoneNumberSuggestion` (`settings/sections/settings_main.cpp`) shows a block at the top of Settings while the `VALIDATE_PHONE_NUMBER` promo suggestion is pending: "Is {phone} still your number?" with the formatted number, an explanation with a Learn more link, and Yes / No buttons. Yes dismisses the suggestion. No opens a box telling the user to change the number in the official app on their phone, since a number cannot be changed from Desktop.

## What changed

Quill already tracked `suggestedActionCheckPhoneNumber` (parsing, state, `hideSuggestedAction`) and showed it as a chat-list card. This PR:

- adds the same prompt to the top of Settings (`src/ui/settings_phone_suggestion.rs`), with Yes (hides the action), No (shows the note inline), and Learn more;
- formats the number with the profile's phone formatter in both places (the chat-list card used the raw digits);
- puts the shared copy in `src/phone_suggestion.rs`.

There is no change-number flow: TDLib's `changePhoneNumber` needs a code sent to the new number and tdesktop itself sends the user to the phone app, so No explains that instead.

## Verified

- Unit tests: title formatting, the `hideSuggestedAction` JSON for the phone action, and a driver test (update adds the action, hiding sends the right request and clears it).
- Not verified: the live suggestion against Telegram. The real suggestion was not dismissed.
