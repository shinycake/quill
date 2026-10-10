# Account sync updates

Cluster `updates-sync`: TDLib `update*` objects Quill ignored or handled only in part.

## What tdesktop does

- Freeze state: a red bar above the chat list ("Your account is frozen!", "Click to view details") opens a box with three paragraphs and a "Submit an Appeal" button. The composer is replaced by a write restriction that opens the same box.
- Speech recognition trial: the free weekly count and its reset date show as "You have N free transcriptions left until DATE." The text switches to the used-up message when the count reaches zero.
- Age verification: turning on "Show 18+ Content" while the server wants verification opens an "Age Verification" box with a "Verify My Age" button that starts the verification bot. Desktop cannot use the camera, so the flow ends in the bot.
- Dice: a message that is only a dice emoji rolls a die. The emoji list comes from the server.
- Downloads: the list follows the server, so a download started on another device shows up and a removal elsewhere removes the row.
- Live location: a running share has a countdown and a stop action.

## What changed in Quill

- Envelope (`src/telegram/envelope/updates_sync.rs`): parses `updateChatDefaultDisableNotification` (and `chat.default_disable_notification`), `updateFileDownloads`, `updateFileAddedToDownloads`, `updateFileRemovedFromDownloads`, `updateDiceEmojis`, `updateFreezeState`, `updateSpeechRecognitionTrial`, `updateActiveLiveLocationMessages`, `updateMessageLiveLocationViewed` and `updateAgeVerificationParameters`.
- State (`src/state/updates_sync.rs`, `Session::sync`): reducers for all of the above plus the pure copy helpers (trial text, live countdown, age prompt text).
- Chat flags: the composer sends silently when the chat default says so. "Send without sound" can be switched off per chat. Sender and view-as-topics updates already applied live.
- Downloads: the panel shows the list totals and picks up downloads added or removed on other devices, with file names from the update's message.
- Dice: the attach menu has a Dice submenu built from the server list, and a lone dice emoji is matched against that list, falling back to the built-in six.
- Freeze: banner, details dialog with the appeal link, hidden composer with a note, and a send guard.
- Speech trial: the counter appears next to "Transcribe" on voice and video notes.
- Live location: a strip above the chat list lists running shares with time left, a "Viewed" mark and Stop. The driver accepts Stop for a share that is not in a loaded history.
- Age verification: the 18+ switch opens the prompt first. "Verify My Age" opens the bot link, after which the switch goes to the server.

## Not done

- `updateAnimatedEmojiMessageClicked` is not handled, so the dice item stays unchecked in the README.
- Age verification cannot finish inside Quill. It hands off to the verification bot, as tdesktop does on platforms without a camera flow.

## How it was checked

- Envelope tests parse schema-shaped JSON for every new update.
- Reducer tests cover each update, the viewed mark surviving a refresh, and the pure helpers.
- Driver tests cover Stop on an unloaded share and the dice list guard.
- Demo captures with `--screenshot-demo ready-updates-sync` and `QUILL_DEMO_SYNC=frozen|live|speech|age|downloads`, each looked at.
- Not verified against a live account: nothing here touches one.
