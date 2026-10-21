# Private-chat menu date order and deleting a channel's gifts

## Message menu date line

tdesktop's `AddWhoReactedAction` handles a private chat (`peer->isUser()`) by
calling `AddWhenEditedForwardedAuthorActionHelper` first (forwarded original
date, or the edited/sent line) and then adding the read-time row. In a group
the reactors row comes first and the date lines follow. Quill sorted the date
line (`order::SENT`) after the audience row in both cases. `menu_audience_rows`
now gives a private chat the date line `order::AUDIENCE` and the read time
`order::PRIVATE_READ`; the origin row of a forward already took the slot above
it.

## Delete Gift in a channel

tdesktop's `CanDeleteSavedStarGift` allows deleting from a channel's list when
the box owner is that channel and `canTransferGifts()` holds (the channel
creator), and never for a collectible, a limited-supply gift, an auction gift
or a refunded one. Quill had only the own-gifts rule (#1056).

- `Session::gifts_deletable` is true for the user's own list and for a channel
  the user created (`ChatSummary::my_member_status == Creator`).
- `ReceivedGift::can_delete` takes that flag and also refuses limited and
  auction gifts (`GiftInfo::limited`, from `overall_limits.total_count` and
  `auction_info`).
- The driver's `delete_received_gift` uses the same gate and sends the usual
  `deleteGift` with the `received_gift_id` from the channel's list.
- Other owner actions (pin, upgrade, transfer, resale, collections, craft)
  stay own-gifts only; the panel renders Delete alone for a channel.

Verified with GPUI tests over `ready-gift-delete` (`QUILL_DEMO_GIFT_DELETE=channel|admin`),
a driver test with the recording sender, and light/dark captures.
