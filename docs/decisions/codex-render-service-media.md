# Render service media: view buttons, birthday card, stop sharing

## What Telegram Desktop does

- `history_view_web_page.cpp` (`PageToPhrase`) puts a call-to-action under a
  link preview that points at a Telegram entity: "View channel", "View
  group", "View bot", "Send message", "View message", "View story",
  "Request to Join", "Boost", "Launch" and a few more.
- `history_view_birthday_suggestion.cpp` draws a suggested birthday as a
  Day / Month / Year table. An incoming suggestion has a "View" button that
  opens the edit-birthday box filled with the date.
- A suggested profile photo is a service row with the photo and "View Photo".
- Expired media are service rows ("Expired photo", "Voice message expired").

## What changed

- Link previews carry `view_button`, parsed from `linkPreview.type` by the
  pure `view_button_label`, with Desktop's labels. The card shows it as a
  label under a divider; the whole card already opens the link.
- A suggested birthday shows the Day / Month / Year table. Incoming ones get
  "View", which opens the existing birthday form with the date filled in, so
  the user confirms before `setBirthdate` goes out.
- Your own running live location has a "Stop sharing" button that sends
  `editMessageLiveLocation` with a null location. The driver refuses the call
  for other people's shares and for ones that already ended.
- The live-location countdown is now computed from the time the state was
  read, so it is right whenever the row is redrawn instead of frozen at
  parse time.
- The title in link-preview and location cards now sets its own colour. In
  an outgoing bubble it inherited white on a light card and was unreadable.
- New demo kind `ready-service-media` (English fixtures).

## Already in place, now claimed

- Photo-change and suggested-photo rows show the new photo and open the viewer.
- Suggested photo has "View Photo" and "Set as My Photo".

## Not done

- Live-location countdown only refreshes when the row redraws; there is no
  per-second timer, so `render-live-location` stays unchecked.
- Dice slot machines are not composed (`render-dice-playback`).
- Reply-header media thumbnail, emoji pattern and timestamp seeking were not
  started.
- `render-expired-media` stays unchecked: the wording matches Desktop, but
  the README note about other unsupported media was not audited.

## How it was verified

- Unit tests: view-button labels, live-location countdown and stop guard,
  `stop_live_location` request shape against `schema/td_api.tl`, driver
  guards, birthday table and form prefill.
- `ready-service-media` capture inspected: cards, buttons and outgoing-bubble
  colours. The birthday form opening and the stop request were not clicked
  in a live session.
