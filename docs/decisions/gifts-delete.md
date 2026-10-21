# Delete a received gift (`gifts-delete`)

## What tdesktop does

`settings/settings_credits_graphics.cpp` (`ShowSavedStarGiftBox`): the saved
gift box of the user's own gifts gets a "..." menu with one entry, "Delete
Gift" (`lng_gift_menu_delete`), when `CanDeleteSavedStarGift` holds: a saved
gift id exists, the gift is not unique, limited or an auction gift, it was
not refunded, and the owner is the user (or a channel the user may transfer
gifts for). The entry opens a confirm box titled "Delete Gift" with
`lng_gift_delete_sure_text` and a red "Delete" button. On confirm,
`DeleteStarGift` sends `payments.deleteStarGift`; on success the gift leaves
the grid and the box and the toast `lng_gift_deleted` ("Gift Deleted") shows.
A failure shows the error toast.

tdesktop offers it nowhere else: not in the profile grid's menu and not on
the gift service message.

## What changed

- `GiftManageInfo.can_be_deleted` parses `receivedGift.can_be_deleted`;
  `ReceivedGift::can_delete(mine)` is the gate. `GiftCard.can_be_deleted`
  parses `messageGift.can_be_deleted` (no UI, as in tdesktop).
- `deleteGift` request, `PaymentsPurpose::DeleteGift`, driver
  `delete_received_gift`, which refuses anything TDLib did not mark
  deletable or that is not the user's own.
- Reducer: `Session::finish_gift_delete` drops the gift from the grid and
  the box, lowers the total, marks the list stale (the server stays the source of truth) and raises a one-shot that the UI shows as the "Gift Deleted" kit toast. A failure
  keeps the gift and sets "Couldn't delete the gift: reason". Nothing is
  optimistic.
- UI: a "Delete Gift" button in the gift page's owner actions, shown only
  when `can_delete`, in danger-red text, opening a kit alert dialog with tdesktop's title and text and a danger-styled Delete button.
- Strings: `lng_gift_menu_delete`, `lng_gift_delete_sure_text`,
  `lng_gift_deleted`, Quill-only `qll_money_could_not_delete_the_gift` and
  `qll_money_delete_gift_not_sent`.

## Deviations

- The entry is a button on the gift page, next to Quill's other owner
  actions, not a "..." menu (Quill's gift page has none).
- tdesktop also lets the creator of a channel delete the channel's gifts
  (`owner->canTransferGifts()`). Quill's gift owner actions are for the
  signed-in user's own gifts only, so channel gifts are not covered yet.

## Verified

Unit tests (parsing, gate, request shape), driver tests with the recording
sender (request sent only for a deletable own gift, `ok` removes it, an error
keeps it), a UI test over `ready-gift-delete` (entry only for a deletable gift;
confirming removes it from grid and box). Light and dark captures of the entry and
the confirmation were checked by eye. Not shown: a real TDLib answer.
