# Gift transfer picker and export through Fragment

## What Telegram Desktop does

Sources: `boxes/transfer_gift_box.cpp`, `boxes/passcode_box.cpp`, `boxes/star_gift_box.cpp` (`AddTransferGiftTable`), `Resources/langs/lang.strings`.

- **Picker.** "Transfer" on an owned collectible opens a peer list titled "Transfer {gift}". It has a search field, a "Send via Blockchain" row with the TON icon at the top, then a "Contacts" header and the contacts. Search reaches global results.
- **Export row status.** While `export_date` is in the future the row reads "unlocks in N days" or "unlocks in N hours". The hours are `max((left + 1800) / 3600, 1)` and the days are `hours / 24`. Tapping a locked row shows "Unlocking in progress" with OK.
- **Export.** Once unlocked, tapping the row shows "Manage with Fragment" with "Open Fragment". tdesktop then sends a probe with an empty password, which tells it whether a password exists and is old enough (`PASSWORD_MISSING` or `PASSWORD_TOO_FRESH_*` lead to a security-check box). Otherwise it asks for the cloud password ("Two-step verification", "Please enter your password to transfer."), sends `getUpgradedGiftWithdrawalUrl` with it and opens the URL. A wrong password shows "Wrong cloud password" under the field, and a flood wait shows the flood text. Other errors become a toast.
- **Transfer to a person.** Picking a contact shows a confirmation with the gift tile, an arrow and the recipient's avatar. The question is "Do you want to transfer ownership of {gift} to {name}?" over the model, backdrop and symbol rows. The button is "Transfer", or "Transfer for {price}" when the transfer costs Stars. tdesktop then sends `transferGift`, shows the "Gift Transferred" toast and opens the recipient's chat (`showPeerHistory`).
- **Held outside Telegram.** A collectible without a Telegram owner shows "Action Locked" with "Open Fragment", which opens `fragment.com/gift/<slug>?collection=my`.

## What changed in Quill

- `src/gift_manage/transfer.rs` (new): `ExportState` from `export_date`, the countdown (`unlock_hours`, `UnlockIn`), `TransferStage` (pick, confirm peer, locked, confirm export, password, security check, action locked), `TransferPicker` and `transfer_candidates` (contacts, never me, bots or the service account 777000; word-prefix search).
- `GiftFlow::Transfer` now holds a `TransferPicker`. `GiftManageState` gains `transferred` (the toast) and `export_url` (the URL to open). Both are one-shot and drained in frame upkeep.
- Requests: `getUpgradedGiftWithdrawalUrl` has a new `PaymentsPurpose::GiftExportUrl`. The `httpUrl` answer closes the picker and hands the URL to the UI. Errors map as follows:
  - `PASSWORD_HASH_INVALID` keeps the password box with "Wrong cloud password".
  - A flood wait keeps the box with the flood text.
  - `PASSWORD_MISSING` and too-fresh passwords open the security check.
  - Anything else sets a note and keeps the box.
- The connect driver sends `transferGift` only from the confirm step, to a known non-bot user who is not the account itself. It sends the export request only from the password box, with a non-empty password, while the export is unlocked.
- UI: `src/ui/gift_transfer_ui.rs` (actions, dialog, upkeep), `src/ui/gift_transfer_view.rs` (one view per stage) and `src/ui/gift_transfer_demo.rs`. The picker is a kit `Dialog` (priority 3870, above the gifts dialog) using `Input`, `ListItem`, `Avatar` and `Button`. "Transfer" in the gift's owner actions opens it and replaces the old "transfer to the open chat" section.
- URLs open only when they are `https` and the link policy says `Open`.
- The password field is masked. It is emptied as soon as Submit reads it, when the box is left and whenever the password stage is not on screen. The password is never kept in state or logged.
- Strings use `lng_*` keys with tdesktop's text and one new `qll_money_could_not_open_fragment`. Three strings of the removed section were dropped.

### Where Quill differs from tdesktop

- **No empty-password probe.** The brief forbids sending the export request before the user confirms and types the password. So Quill asks for the password first, and the security check appears after Submit when the server answers `PASSWORD_MISSING` or too fresh.
- **Search** covers contacts and, once a query is typed, private chats. tdesktop also searches globally.
- **The confirmation's value row** (`AddTransferGiftTable`) is left out because Quill does not load the gift's value yet.
- **Unexpected export errors** become a note in the gifts dialog instead of a toast with the raw error.
- **Escape on a step** goes back to the list; on the list it closes the picker.

## How it was verified

- Unit tests (`src/gift_manage/transfer_tests.rs`):
  - The countdown against a copy of tdesktop's formula, from 1 s to 30 days, including the half-hour and 23.5-hour boundaries.
  - The stage moves and the action-locked start.
  - The candidate filter.
- Driver tests (`src/connect/tests/gift_export.rs`), all with the recording sender:
  - `transferGift` goes only to the confirmed contact.
  - The export is sent only from the password box, with the exact request shape.
  - The password never appears in the state's `Debug`.
  - The `httpUrl` answer sets `export_url`.
  - Each error maps as listed above.
- GPUI tests (`src/ui/gift_transfer_tests.rs`):
  - The picker opens over the gift and walks its steps, then closes back to the gift.
  - The password field is emptied on Submit and when the box is left.
- Demo screenshots of every `QUILL_DEMO_GIFT_TRANSFER` stage (`picker`, `locked`, `confirm`, `password`, `peer`, `action-locked`), checked by eye.
- `gate.sh`.

Nothing was run against a live account. No gift was transferred or exported, and no withdrawal URL was requested from Telegram.

### Not verified

- The real server answers to `getUpgradedGiftWithdrawalUrl`, including the exact too-fresh error text, are covered only by the error mapping tests.
- The Fragment page opened by the URL was not checked.
