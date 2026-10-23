# Emoji status selector

## What tdesktop does

`MainMenu::chooseEmojiStatus` shows `Info::Profile::EmojiStatusPanel` under the
menu's badge, or the Premium preview when the account has no Premium. The
panel is the emoji tab alone (`TabbedSelector` in `Mode::EmojiStatus`):

- a search field;
- one untitled section: a star cell (removes the status), the first 7 themed
  statuses, recent and default statuses not yet listed, then the account's
  current status;
- a Collectibles section (upgraded gifts), then the installed emoji packs;
- a click sets the status with no end date and closes the panel;
- a right click (`EmojiListWidget::fillEmojiStatusMenu`) offers "Set Status
  for" 1 hour, 8 hours, 1 day, 7 days, and Custom;
- Custom hides the panel and opens `PickUntilBox` ("Set Status for..."): a
  vertical drum over `DefaultTimePickerValues` phrased by `FormatMuteFor`,
  with Set Status and Cancel;
- the Premium gate is `EmojiStatusPanel::filter`: choosing a status without
  Premium shows the Premium preview; removing needs no Premium.

The task brief listed 1h/2h/8h/2d for the menu; that is the old Quill list. The
source has 1h/8h/1d/7d, so that is what ships.

## What changed

- `src/emoji_status_selector.rs`: the rules (slot order, gate, drum values,
  duration phrases), with unit tests.
- `src/ui/media_panel/status.rs`: the status mode of the existing emoji
  panel (rows, remove cell, right-click menu), reusing its list, cells and
  custom-emoji rendering. `status_time_box.rs`: the "Set Status for..." box
  (a registered dialog). The kit has no drum picker, so the drum is drawn
  there; wheel, click and arrow keys move it.
- `ConnectDriver`: the choices now include `getUpgradedGiftEmojiStatuses`;
  `change_collectible_emoji_status` sets a collectible as its gift;
  removing a status no longer needs Premium; `close_emoji_status_choices`.
- Removed from the pack settings dialog: the status section, the load
  button, duration buttons, the typed hours field and "Clear recent
  statuses" (tdesktop has no such control in the status panel).

## Not done

- The drum snaps to rows without a settle animation.
- A recent collectible (no custom emoji id of its own in the recent list)
  is shown through the Collectibles section only.
- The panel opens at a fixed corner under the menu button: the menu is a
  native dropdown that does not report its bounds.

## Verified

Unit and driver tests, GPUI UI tests on the demo account, and /verify
captures (`ready-emoji-status`, `ready-emoji-status-until`) in light and dark.
Nothing ran against a live account.
