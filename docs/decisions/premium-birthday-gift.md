# Gifts from the birthday suggestion

## What tdesktop does

- The chat list's birthday card (`suggestion_birthday_contacts.cpp`) shows only
  when Premium can be bought (`Session::premiumCanBuy`). One contact opens the
  gift box for them (`ShowStarGiftBox`). Several open the gift recipient list
  (`ChooseStarGiftRecipient`). Hiding the card shows the toast "You can send a
  Gift later in Settings".
- `ChooseStarGiftRecipient` (`boxes/star_gift_box.cpp`) is titled "Gift
  Premium or Stars" and has a search field and Cancel. It lists:
  - the account itself, with the status "buy yourself a gift"
  - "Birthdays": contacts with a birthday yesterday, today or tomorrow, today
    first, each with a birthday status
  - "Frequent contacts": top peers, without the account, bots or the
    birthday contacts (`CollectGiftFrequentUsers`)
  - "Contacts": everyone else
  A row opens the gift box.
- Settings has a "Premium Gifting" row that opens the same list, again only
  where Premium can be bought.

## What changed in Quill

- `quill::gift_recipients` builds the list as pure data: sections, order,
  statuses and search.
- `chatlist_suggestions::pick` takes `premium_can_buy`. The card hides when
  TDLib's `is_premium_available` is false.
- The card opens the gift picker for one contact, or the new
  `GiftRecipients` dialog for several. Hiding it shows tdesktop's toast.
- The driver opens the gift picker for a user id
  (`open_send_gift_to_user`), using the user record instead of a loaded
  private chat. Contacts without a chat no longer need `createPrivateChat`.
  `load_gift_recipients` loads the contacts and top chats.
- Settings > Telegram has a "Premium Gifting" row and a settings search
  entry. Both appear only where Premium can be bought.

## Not done

- tdesktop has no gift action in any other contact birthday list. Quill's own
  Settings > Contacts birthday block still opens the chat.
- The row context menu (Send message, View profile) is left out.
- With a search, Quill lists matching birthday, frequent and contact rows
  together in one list. tdesktop searches its contacts list.

## Verification

- Unit tests for the list rules (`src/gift_recipients.rs`) and the suggestion
  gate (`src/chatlist_suggestions.rs`).
- Driver tests (`src/connect/tests/birthday_gift.rs`). They cover:
  - the gift picker for a contact without a chat
  - bots and unknown users refused
  - the list loading contacts and top chats
- UI tests (`src/ui/gift_recipients_tests.rs`). They cover:
  - several birthdays open the list and a row opens the picker
  - one birthday opens the picker
  - hiding the card shows the toast
  - no card where Premium is not sold
  - search
  - the Settings row
- `/verify` with `ready-birthday-gift` and `ready-birthday-gift-one` in light
  and dark. Captured the card, the list, picking a row, the single-contact
  path and hiding the card. The offline demo has no gift catalog, so the
  gift picker shows its title and no gifts.
