# New Link / Edit Link box

## What tdesktop does

`EditLinkBox` (boxes/peers/edit_peer_invite_link.cpp) opens
`EditInviteLinkBox` (ui/boxes/edit_invite_link.cpp), titled "New Link" or
"Edit Link". From the top:

- Channels whose link is private get a "Require Monthly Fee" switch
  (`FillCreateInviteLinkSubscriptionToggle`, edit_invite_link_session.cpp).
  On, it shows a Stars field capped at `stars_subscription_amount_max` with
  "~$X / month" from `stars_usd_withdraw_rate_x1000`, then the "Charge a
  subscription fee…" note with "Learn more" to telegram.org/tos/stars.
- "Limit by time period": radio buttons No limit, 1 hour, 1 day, 1 week,
  Custom. Custom opens `ChooseDateTimeBox` titled "Expire after" (a day
  ahead by default, at most a year ahead). A chosen date becomes its own
  radio, placed among the presets by time.
- "Limit by number of users": No limit, 1, 10, 100, Custom. Custom opens
  a number box ("Enter custom limit"); the value becomes its own radio in
  numeric order. The whole section is hidden while approval is on.
- "Approve New Members" / "Approve New Subscribers", locked off for a public
  chat that doesn't already ask for approval, with the matching footer.
- "Link Name (optional)", 32 characters, "Only admins will see this name."
- Save is "Create" or "Save". Approval and the fee turn each other off.
  Editing a subscription link shows only the locked fee and the name, and
  the switch answers with the "To change the subscription fee…" toast.

## What changed

The old panel above the composer, with typed "Expires in days", "Member
limit" and "Stars per month" fields, is gone. The box is now a kit dialog
(`src/ui/invite_link_box/`) over the invite links page, with kit `Switch`,
`RadioGroup`, `Input` and `DatePicker` (the app's shared
`new_date_time_picker`). The Custom date and number boxes are their own
dialogs that return to the link box. The rules (preset lists and custom
insertion, exclusivity, locking, the request Save sends) live in
`src/invite_link_form.rs` with unit tests. The invite link rows trade the
subscription-only "Rename" for tdesktop's "Edit" on every non-primary
link.

Requests: `createChatInviteLink` / `editChatInviteLink` with
`member_limit` 0 whenever approval is on (TDLib requires it), and
`createChatSubscriptionInviteLink` / `editChatSubscriptionInviteLink` for
paid links. The fee cap is the TDLib option `subscription_star_count_max`
(2,500 until it arrives), and the dollar rate comes from the app config.

## Deviations, and why

- While "Require Monthly Fee" is on, the time and member limits hide.
  tdesktop keeps them, but `createChatSubscriptionInviteLink` takes no
  expiry or limit, so showing them would promise something Save can't send.
- The Custom limit box is titled "Limit by number of users". tdesktop
  reuses "Expire after" there, which reads like a slip. The cap is TDLib's
  99,999, not tdesktop's 200,000.
- Invalid Custom values disable Save instead of shaking the field.
- tdesktop names a guard bot in the approval footer; TDLib does not expose
  one, so that line is left out.
- After creating a link tdesktop opens the link's own box; Quill closes and
  the new link appears in the list.

## Verified

Unit tests in `src/invite_link_form/tests.rs`; GPUI tests in
`src/ui/invite_link_box/ui_tests.rs` pick presets, toggle approval and the
fee, set a custom date and a custom limit, and open an existing link.
Screenshot demos `ready-invite-link-new`, `-fee`, `-edit` and
`-custom-date`, captured in light and dark.
