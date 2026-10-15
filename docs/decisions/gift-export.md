# Gift transfer picker and export through Fragment

## What Telegram Desktop does

Sources: `boxes/transfer_gift_box.cpp`, `boxes/passcode_box.cpp`, `boxes/star_gift_box.cpp` (`AddTransferGiftTable`), `Resources/langs/lang.strings`.

- **Picker.** "Transfer" on an owned collectible opens a peer list titled "Transfer {gift}". It has a search field, a "Send via Blockchain" row with the TON icon at the top, then a "Contacts" header and the contacts. Search reaches global results.
- **Export row status.** While `export_date` is in the future the row reads "unlocks in N days" or "unlocks in N hours". The hours are `max((left + 1800) / 3600, 1)` and the days are `hours / 24`. Tapping a locked row shows "Unlocking in progress" with OK.
- **Export.** Once unlocked, tapping the row shows "Manage with Fragment" with "Open Fragment".
  - **Probe.** After "Open Fragment", tdesktop reloads the password state and sends `payments.getStarGiftWithdrawalUrl` with `inputCheckPasswordEmpty`. `PASSWORD_MISSING` or `PASSWORD_TOO_FRESH_*` / `SESSION_TOO_FRESH_*` open a security-check box (`PrePasswordErrorBox`); any other answer opens the password box.
  - **Password box.** "Two-step verification", "Please enter your password to transfer." With the password, the request returns a URL, which tdesktop opens.
  - **Errors** (`PasscodeBox::handleCustomCheckError`):
    - `PASSWORD_HASH_INVALID` and `SRP_PASSWORD_CHANGED` show "Wrong cloud password" under the field.
    - A flood wait shows the flood text.
    - `SRP_ID_INVALID` reloads the password data, then shows a server error.
    - Anything else becomes a toast.
  - **"Forgot password?"** shows only with a recovery email or no reset pending (`hasRecovery || !pendingResetDate`).
- **Transfer to a person.** Picking a contact shows a confirmation with the gift tile, an arrow and the recipient's avatar. The question is "Do you want to transfer ownership of {gift} to {name}?" over the model, backdrop and symbol rows. The button is "Transfer", or "Transfer for {price}" when the transfer costs Stars. tdesktop then sends `transferGift`, shows the "Gift Transferred" toast and opens the recipient's chat (`showPeerHistory`).
- **Held outside Telegram.** A collectible without a Telegram owner shows "Action Locked" with "Open Fragment", which opens `fragment.com/gift/<slug>?collection=my`.

## What changed in Quill

- **Core state.** `src/gift_manage/transfer.rs` (new) holds:
  - `ExportState` from `export_date`, and the countdown (`unlock_hours`, `UnlockIn`).
  - `TransferStage`: pick, confirm peer, locked, confirm export, probing, password, security check, action locked.
  - `TransferPicker`, with the probe routing in `probe_failed`, `probe_answered` and `password_state_known`.
  - `transfer_candidates`: contacts, never me, bots or the service account 777000; word-prefix search; one row per person.
- `GiftFlow::Transfer` now holds a `TransferPicker`. `GiftManageState` gains `transferred` (the toast) and `export_url` (the URL to open). Both are one-shot and drained in frame upkeep.
- **The probe.** "Open Fragment" sends `getUpgradedGiftWithdrawalUrl` with an empty password under its own purpose, `PaymentsPurpose::GiftExportProbe`, and reloads the password state.
  - TDLib answers an empty password itself with `PASSWORD_HASH_INVALID` (`StarGiftManager::get_star_gift_withdrawal_url`), so the server never sees it. Quill therefore also routes on the reloaded `passwordState`: `has_password == false` opens the security check instead of the password box. `PASSWORD_MISSING` and too-fresh answers route as in tdesktop.
  - A URL answer to the probe is dropped, never opened, and the password box opens.
- **The real request.** It uses `PaymentsPurpose::GiftExportUrl`. Its `httpUrl` answer closes the picker and hands the URL to the UI. Errors show under the password field:
  - `PASSWORD_HASH_INVALID` and `SRP_PASSWORD_CHANGED`: "Wrong cloud password".
  - A flood wait: the flood text.
  - `SRP_ID_INVALID`: "Internal server error. Please try again later."
  - `PASSWORD_MISSING` and too-fresh answers: the security check.
  - Anything else: "Couldn't open Fragment: {reason}", with the box kept open.
  - The two SRP errors are new `EarnErrorHint` classes.
- **Driver checks.** The connect driver sends:
  - `transferGift` only from the confirm step, to a known non-bot user who is not the account itself.
  - The probe only from "Manage with Fragment".
  - The password request only from the idle password box, with a non-empty password.

  Both export requests need the export to be unlocked. Every one of these is refused while another gift request is out, so a double click sends nothing.
- **UI files.** `src/ui/gift_transfer_ui.rs` (actions, dialog, upkeep), `src/ui/gift_transfer_view.rs` (one view per stage) and `src/ui/gift_transfer_demo.rs`.
  - The picker is a kit `Dialog` (priority 3870, above the gifts dialog) using `Input`, `ListItem`, `Avatar` and `Button`. "Transfer" in the gift's owner actions opens it, replacing the old "transfer to the open chat" section.
  - When the open chat is a private chat with someone who is not a bot or the user, that person is listed first, so one click reaches the confirmation (the design's preselected row).
  - The list is cached per search query, open chat and session revision.
- URLs open only when they are `https` and the link policy says `Open`.
- **Password field.**
  - It is masked, takes the keyboard when the box opens and after each answer, and Enter submits it.
  - It is emptied as soon as Submit reads it, when the box is left and whenever the password stage is not on screen. The kit's `set_value` also clears the undo and redo stacks, and its cached layout holds only the mask glyphs, so recreating the input is not needed.
  - The password is never kept in state or logged.
- **Strings.**
  - New `lng_*` keys use tdesktop's text.
  - New `qll_money_*` keys: `couldnt_open_fragment`, `couldnt_open_fragment_reason` and `server_error` (tdesktop hard-codes its server error text).
  - Three strings of the removed section were dropped.

### Where Quill differs from tdesktop

- **The probe's local answer.** As above, TDLib never sends the empty-password probe to the server, so "no password" comes from the reloaded password state. A password that is too fresh is only detected once the password is submitted.
- **Search** covers contacts and, once a query is typed, private chats. tdesktop also searches globally.
- **The confirmation's value row** (`AddTransferGiftTable`) is left out because Quill does not load the gift's value yet.
- **Unexpected export errors** show under the password field instead of as a toast, which would sit behind the picker.
- **`SRP_ID_INVALID`** shows the server error at once. TDLib fetches fresh password data for every request, so tdesktop's silent first retry is not needed.
- **Escape on a step** goes back to the list; on the list it closes the picker.

## How it was verified

- Unit tests (`src/gift_manage/transfer_tests.rs`):
  - The countdown against a copy of tdesktop's formula, from 1 s to 30 days, including the half-hour and 23.5-hour boundaries.
  - The stage moves, the probe routing (including a late password state) and the action-locked start.
  - The candidate filter, and the open chat first with no duplicate row.
- Driver tests (`src/connect/tests/gift_export.rs`), all with the recording sender:
  - `transferGift` goes only to the confirmed contact, and a second click sends nothing.
  - The probe is sent only from "Manage with Fragment" for an unlocked gift, with `password: ""` and a password state reload.
  - A URL answer to the probe is never opened.
  - The probe routes to the security check or the password box.
  - The export is sent only from the password box, with the exact request shape, and the password never appears in the state's `Debug`.
  - The `httpUrl` answer sets `export_url`, and each error, including the SRP ones and an unknown one, lands under the field as listed above.
- `earn` tests: the two new SRP error classes.
- GPUI tests (`src/ui/gift_transfer_tests.rs`):
  - The picker opens over the gift and walks its steps, then closes back to the gift.
  - The open private chat is listed first.
  - The password box takes the keyboard and Enter submits.
  - An unknown error leaves a visible message under the field.
  - The password field is emptied on Submit and when the box is left.
- Demo screenshots of every `QUILL_DEMO_GIFT_TRANSFER` stage (`picker`, `locked`, `confirm`, `probing`, `password`, `peer`, `open-chat`, `action-locked`), checked by eye.
- `gate.sh`.

Nothing was run against a live account. No gift was transferred or exported, and no withdrawal URL was requested from Telegram.

### Not verified

- The real server answers to `getUpgradedGiftWithdrawalUrl`, including the exact too-fresh error text, are covered only by the error mapping tests.
- The Fragment page opened by the URL was not checked.
