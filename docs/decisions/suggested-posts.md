# Suggested posts and offers in channel direct messages

Parity item `parity:premium-suggested-posts`. Design by the architect
session (2026-10-14); the build fills in "How verified" at the end.

## What tdesktop does

Reference: `api/api_suggest_post.cpp`,
`history/view/controls/history_view_suggest_options.cpp`,
`history/view/media/history_view_suggest_decision.cpp`,
`history/history_widget.cpp` (`refreshSuggestPostToggle`,
`applySuggestOptions`), `history/history_item.cpp`
(`computeSuggestionActions`), strings `lng_suggest_*` in
`Resources/langs/lang.strings`, metrics in `ui/chat/chat.style`
(`chatSuggest*`, `suggestPriceBox`).

A channel can open "direct messages": a special supergroup
(`supergroup.is_direct_messages_group`, "monoforum"). Members write to the
channel there; the channel's admins read the same chat split by sender
(`is_administered_direct_messages_group`, topics of kind
`messageTopicDirectMessages`). In that chat a member can *suggest a post*:
a message with a price (Stars or TON) and an optional publication time
that the channel may accept, decline, or counter.

### Composer (member side only)

- When the open chat is a direct messages group and the viewer is **not**
  its admin, the composer shows one more icon button next to the attach
  button: `historySuggestPostToggle` (icon `chat/input_paid`). It is hidden
  while the suggest bar is up.
- Clicking it puts a **suggest bar** above the field (same slot and layout
  as the reply bar: icon on the left, two lines, close on the right) and
  cancels any reply in progress. Title "Suggest a Post Below" (accent
  color, semibold). Second line (muted):
  - no price, no date: "Click to offer a price for publishing."
  - price only: "{amount} for publishing anytime."
  - date only: "Publish on {date}"
  - both: "{amount}   📆 {date}"
  `{amount}` is a star or TON icon followed by the number.
- Clicking the bar opens the **Suggest a Message** box; closing the bar
  drops the suggestion. The bar's values are saved with the draft
  (`draftMessage.suggested_post_info`) and restored when the chat reopens.
- Sending a message while the bar is up sends it as a suggested post
  (`messageSendOptions.suggested_post_info`), then the bar is cleared.

### "Suggest a Message" box (`ChooseSuggestPriceBox`)

Width `st::boxWidth`; style `suggestPriceBox` (42 px primary button, 22 px
button padding). Title depends on mode: "Suggest a Message" (new) or
"Suggest Changes" (change). For a member the box has no title bar: the
title is a centered label, the close "×" sits top-right, and the current
balance (Stars or TON, whichever tab is active) sits top-left.

Content, top to bottom:
1. Two pill tabs (`giftBoxTab*` style, active pill filled): "Offer Stars"
   / "Offer Grams" for a member; "Request Stars" / "Request Grams" for an
   admin (the admin uses the same box when countering, see below).
2. Section title "Enter Price in Stars" (or "Enter Price in Grams") and a
   number field with the star (or TON) icon in it, with the approximate
   USD under it. Switching tabs converts the typed amount to the other
   currency. The field may be empty (free post).
   Below the field a divider text:
   - member: "Choose how many Stars to pay to publish this message." /
     "Choose how many Grams to pay to publish this message."
   - admin: "You will receive {count} Stars ({percent}) for publishing
     this post." plus, for Stars, "Transactions in **Stars** may be
     reversed by the payment provider within **21** days. Only accept
     Stars from people you trust."
   Limits come from TDLib options: `suggested_post_star_count_min/max`,
   `suggested_post_toncoin_cent_count_min/max` (fall back to
   `suggested_post_gram_cent_count_*` if the option name differs in the
   pinned TDLib). A non-empty amount outside the range shows the field
   error and, when it is too small, the toast "Minimum price is {count}
   Stars." / "... Grams.".
   Commission: `suggested_post_star_earnings_per_mille` /
   `suggested_post_toncoin_earnings_per_mille`; `{percent}` is
   `per_mille / 10` with one decimal ("85%").
3. A settings-style row "Time" with the value "Anytime" or the chosen
   date-time; clicking opens the **time box** (below). Divider text under
   it: "Select the date and time you want the message to be published. The
   post will remain available for at least 24 hours from this date."
4. The bottom button: "Offer for Free" when the amount is empty, "Offer
   {amount}" otherwise (icon + number), "Update Terms" in change mode.
   Enter in the field submits.
   Before closing, a member is checked against their balance: Stars short
   → the "buy more Stars" flow (tdesktop `MaybeRequestBalanceIncrease`),
   then the box resubmits on success; TON short → an inform box "{amount}
   Grams Needed" / "You can add funds to your balance via the third-party
   platform Fragment." with a button "Add Funds via Fragment" that opens
   `https://fragment.com/ads/topup`.

### Time box (`ChooseSuggestTimeBox`)

tdesktop's date-time chooser. Title "Time" (new / publish) or "Edit Time"
(change). Submit: "Publish" (publish mode), "Save" (new), "Update Time"
(change). A left button "Anytime" (or "Publish Now" in publish mode)
returns 0. Range: from now + `suggested_post_send_delay_min` + 60 s to
now + `suggested_post_send_delay_max`; default value now + 24 h, or the
current value clamped into the range.

### The suggested message in the history

Every message with `suggested_post_info` in a direct messages chat:

- Never shows the reply header (the counter-offer replies to the previous
  offer, but the header is hidden).
- Gets a **service pre-message** above the bubble: a centered service
  block (max width 236 px, service bubble background, service text color)
  that reads, as its first line (bold, centered, margins 16/16/16/6):
  - the original offer: "You suggest to post this message." (outgoing) or
    "{from} suggests to post this message." (`{from}` bold short name);
  - a counter-offer (the message replies to another suggestion): "{from}
    suggests a new price for the message." / "... a new time ..." / "... a
    new price and time ..." / "{from} suggests changes for the message."
    (when the text or media changed too, or when neither price nor time
    changed).
  Then a two-row attribute table (label left in faded service color,
  value right bold): "Price" (or "New Price") = "Free" / "{n} Stars" /
  "{n} Grams"; "Time" (or "New Time") = "Anytime" / the date-time. When
  the message itself changed the table is followed by a faded line "Check
  the suggested message below". Clicking the block jumps to the previous
  offer when there is one.
- Gets **buttons under the bubble**, laid out like an inline keyboard, one
  per row: "Accept", "Decline", "Suggest Changes". Which appear is
  decided by `suggestedPostInfo`: state must be `pending`; "Accept" needs
  `can_be_approved`; "Decline" needs `can_be_declined`; "Suggest Changes"
  appears whenever "Decline" does (tdesktop `AcceptAndDecline` →
  all three, `Decline` → the last two, `None` → nothing). An outgoing
  offer that the other side has not answered shows nothing.

### Accept

- Without a date (`send_date == 0`): first the time box in publish mode
  ("Time", submit "Publish", left button "Publish Now").
- Then the confirm box "Accept Terms" with text:
  - admin: "Do you want to publish this post from {from}?"; member: "Do
    you want to publish this post to {channel}?"
  - with a price, a second paragraph: admin: "{channel} will receive
    **{count} Stars** ({percent}) for publishing {date}." or "... for
    publishing right now." (count is the price after commission), then
    " It must remain visible for at least **24** hours after
    publication." and, for Stars, the 21-days warning; member: "You will
    pay **{count} Stars** for publishing {date}." / "... right now."
  - confirm button "Publish".
- A member paying Stars is first checked against the balance (same
  top-up flow as the price box); TON short shows the Grams-needed box.
- Then `approveSuggestedPost(chat_id, message_id, send_date)`. An error
  toasts its text. The decision arrives as a service message
  (`messageSuggestedPostApproved` etc., already rendered by Quill) and
  `updateMessageSuggestedPostInfo` flips the state so the buttons go.

### Decline

Confirm box titled "Decline": "Do you want to decline publishing this post
from {from}?" (admin) / "... to {channel}?" (member), a single-line field
"Add a reason (optional)" (0–128 characters), destructive confirm button
"Decline". Enter in the field confirms. Sends
`declineSuggestedPost(chat_id, message_id, comment)`.

### Suggest Changes

A popup menu with icons: "Edit Message" (only when the message has no web
page and its media allows caption editing, or has no media), "Edit Price",
"Edit Time".

- "Edit Price" opens the price box in change mode prefilled with the
  current price and time; "Edit Time" opens the time box in change mode.
  Either result is sent with `addOffer(chat_id, message_id, options)`
  where `options.suggested_post_info` carries the **complete** new terms
  (price and date). The answer is the new message, which TDLib delivers
  as a normal update.
- "Edit Message" loads the message text (or caption) into the composer as
  an edit with the suggest bar up in change mode ("Suggest Changes" title)
  prefilled with the current terms; saving sends a **new** message
  (`sendMessage`, `reply_to = inputMessageReplyToMessage{message_id}`,
  `options.suggested_post_info` from the bar), never `editMessageText`.

### Add Offer (member, context menu)

In a direct messages chat a member's context menu on a regular, non-service
message of their own that has no suggestion and is not part of an album
gets "Add Offer" (`lng_context_add_offer`), which opens the price box in
new mode and sends `addOffer` with the chosen terms.

### Deleting an accepted paid post (channel side)

When an admin deletes a published post whose Stars/TON have not been
released yet, tdesktop warns: "Stars will be lost" / "You won't receive
**Stars** for the post if you delete it now. The post must remain visible
for at least **24 hours** after it was published." with "Delete Anyway".
TDLib exposes this through `message.is_paid_star_suggested_post` /
`is_paid_gram_suggested_post` on the channel post: show the warning in the
delete box when the flag is set and the post is younger than
`suggested_post_lifetime_min` seconds.

## TDLib mapping (pinned schema in `native/td`)

| tdesktop | TDLib |
|---|---|
| `peer->isMonoforum()`, `amMonoforumAdmin()` | `supergroup.is_direct_messages_group`, `supergroup.is_administered_direct_messages_group` (`updateSupergroup`) |
| `HistoryMessageSuggestion` | `message.suggested_post_info: suggestedPostInfo{price, send_date, state, can_be_approved, can_be_declined}`; `updateMessageSuggestedPostInfo` |
| `SuggestOptions` on send | `messageSendOptions.suggested_post_info: inputSuggestedPostInfo{price, send_date}`; price `suggestedPostPriceStar{star_count}` / `suggestedPostPriceGram{gram_cent_count}` / null |
| draft suggest | `draftMessage.suggested_post_info` |
| toggle approval | `approveSuggestedPost`, `declineSuggestedPost` |
| counter-offer, add offer | `addOffer(chat_id, message_id, options)` |
| app config `suggestedPost*` | options `suggested_post_star_count_min/max`, `suggested_post_toncoin_cent_count_min/max`, `suggested_post_star_earnings_per_mille`, `suggested_post_toncoin_earnings_per_mille`, `suggested_post_send_delay_min/max`, `suggested_post_lifetime_min` (check `OptionManager.cpp` for the exact names in the pinned build) |
| decision service messages | `messageSuggestedPostApproved/Declined/Paid/Refunded/ApprovalFailed` (already parsed, `service_text/actions.rs`) |
| can add offer / edit terms | `messageProperties.can_add_offer`, `can_edit_suggested_post_info` |

Everything the item needs exists in the pinned TDLib; nothing is blocked.

## Design for Quill

### Core (`quill` lib, no GPUI)

- `src/suggested_posts.rs` (new, pure): `SuggestedPrice { Stars(i64),
  TonCents(i64) }`, `SuggestedPostInfo { price: Option<SuggestedPrice>,
  send_date: i32, state: Pending|Approved|Declined, can_be_approved,
  can_be_declined }` with `parse(&Value)`, `to_input_json()`;
  `SuggestTerms { price, send_date }` (the bar's values); `bar_text`,
  `price_label` ("Free" / "{n} Stars" / "{n} Grams"), `after_commission`,
  `percent_label`, `SuggestionButtons::for_(info, outgoing)` →
  `{accept, decline, change}` bools, `Changes::resolve(now, original)`
  (price/date/message flags, same rules as `ResolveChanges`),
  `SuggestedPostLimits` read from `updateOption` (`apply_option(name,
  value)`), `time_range(now, limits)`, `clamp_time`. Unit tests in
  `src/suggested_posts_tests.rs` (file naming so the size check skips it).
- `MessageExtras.suggested_post: Option<SuggestedPostInfo>` (parse in
  `envelope/message.rs`), plus `is_paid_suggested_post: bool` from
  `is_paid_star_suggested_post || is_paid_gram_suggested_post`.
- Groups domain: parse `is_direct_messages_group` and
  `is_administered_direct_messages_group` in `UpdateSupergroup` /
  `Supergroup`; store `supergroup_direct_messages: HashMap<i64,
  DirectMessagesRole>` (`Member` / `Admin`) in
  `state/domains/groups/state.rs`; `Session::direct_messages_role(chat)`.
- Payments domain (per the parity plan: own sub-enum, no `PaymentsPurpose`
  growth beyond one wrapper variant): `PaymentsPurpose::SuggestedPost(
  SuggestedPostPurpose)` with `Approve { chat, message }`, `Decline {..}`,
  `AddOffer {..}`; `PaymentsPayload::SuggestedPost(SuggestedPostPayload)`
  with `InfoUpdated { chat, message, info }` (from
  `updateMessageSuggestedPostInfo`, parsed in
  `envelope/domains/payments/parse.rs`) and the `addOffer` message
  answer (route the returned `message` through the normal new-message
  path). Errors in `domains/payments/error.rs` land in a one-shot
  `suggested_post_error: Option<String>` the UI drains into a toast.
- `SendOptions.suggested_post: Option<SuggestTerms>`;
  `message_send_options` emits `inputSuggestedPostInfo`; drafts
  (`requests/chats.rs` save draft, `envelope/chat_drafts.rs` parse)
  carry it.
- `connect/suggested_posts.rs` (new): `approve_suggested_post`,
  `decline_suggested_post`, `add_offer`, `send_suggested_change`
  (sendMessage with reply_to + terms). Driver tests with the recording
  sender in `connect/tests/suggested_posts.rs`.
- `telegram/requests/suggested_posts.rs` (new) builds the JSON.

### UI (`src/ui`)

- `ui/suggested_posts_state.rs` (`SuggestedPostsUi`): the bar's terms and
  mode, the open box (`None | Price { mode, draft fields } | Time { mode }
  | AcceptConfirm {..} | Decline {..} | TonNeeded {..}`), the price
  `InputState` (number), decline reason `InputState`, the time picker
  (`DatePickerState` + `TimeFieldState`, reuse `ui/scheduled.rs` helpers
  `picked_unix` / `picker_value`), balance snapshot. One `QuillApp` field,
  one `new_with_demo` line.
- `ui/suggested_posts_ui.rs`: the composer toggle button (place it in the
  composer's trailing icon row next to the attach button, visible only
  when `direct_messages_role == Member`; icon: the Quill asset used for
  paid messages if one exists, else `IconName::Coins`-like lucide icon,
  tinted accent while the bar is up), the suggest bar (reuse
  `composer_context_bar` with the title/text above), the dialogs
  registered with `register_dialogs!` (free priorities near 3960):
  price box, time box, accept-terms box, decline box, Grams-needed box.
  Kit components: `Dialog`, `Button` (primary / ghost / destructive),
  `Tab`/`TabBar` or two `Toggle` pills for the currency, `NumberInput`,
  `DatePicker` + `TimeField`, `Input` for the reason, `Alert` for the
  21-days warning text.
- `ui/history/suggested_post_block.rs`: the service pre-message block
  (service bubble style already used for service rows) and the button
  column (reuse the inline-keyboard row builder so hover / pressed states
  match). Hook from `session_history_row.rs` next to `fact_check_block`
  (keep that file's growth to the two call sites).
- Suggest Changes menu: kit `PopupMenu` with the three items; "Edit
  Message" goes through the existing edit path (`composer/edit.rs`) with
  the bar set to change mode and a flag that makes save send a new
  message instead of `editMessageText`.
- Context menu "Add Offer": one item in the message context menu, gated
  by `messageProperties.can_add_offer` when Quill already fetches the
  properties for that menu, else by the local rule (member, own regular
  message, no suggestion, no album).
- Delete warning: extend the delete-messages box with the "Stars will be
  lost" paragraph and "Delete Anyway" when `is_paid_suggested_post` and
  the post is younger than `suggested_post_lifetime_min`.

### Tests

- Unit: parse `suggestedPostInfo` (star, gram, null price), buttons
  matrix, bar text variants, changes resolution, commission math, time
  clamp.
- Driver: approve with/without date, decline with comment, addOffer JSON,
  sendMessage with `suggested_post_info`, draft round trip, supergroup
  flags → role.
- GPUI UI test (`#[gpui::test]`, demo session, no connection): the toggle
  shows only for a member of a direct messages chat; clicking it shows
  the bar and hides the toggle; the price box submit updates the bar
  text; the decline box exists with its reason field; a message with
  `suggested_post_info` renders the three buttons for an admin and none
  for the pending outgoing offer.
- Visual: one temporary demo capture (not committed) of a direct messages
  chat with an incoming suggestion (block + buttons) and the suggest bar,
  light and dark.

## Build plan (one PR, branch `suggested-posts`)

1. Core types, parsing, options, supergroup role (tests).
2. Requests, purposes, payloads, driver methods (driver tests).
3. Composer toggle + bar + draft round trip + send.
4. Price box, time box.
5. History block + buttons; accept / decline / changes flows; add offer;
   delete warning.
6. UI test, demo capture, decision doc "How verified", parity fragment
   `parity-fragments/suggested-posts.txt` = `parity:premium-suggested-posts`.

Never-crash rules apply (`docs/contributing/panic-safety.md`): no indexing
into bid tables or entity ranges, no `RefCell` borrows held across kit
calls, all TDLib numbers via checked parsing.

## Deviations from tdesktop

- Quill has no TON balance display outside the wallet; the price box shows
  the Stars balance on the Stars tab and the Grams balance only when TDLib
  (`updateOwnedGramCount`) or the wallet has reported it (otherwise
  nothing, never a fake 0).
- The approximate USD line under the price field shows only for Grams,
  from the wallet's TON/USD rate when Quill already has it. Stars have no
  USD line.
- Corrections to the plan above, taken from the tdesktop source while
  building: the decision buttons are `[Decline | Accept]` plus a full-width
  `[Suggest Changes]` (only `[Decline]` when the viewer can decline but not
  approve); the toggle sits after the bot keyboard toggle; "Add Offer" is
  not limited to the viewer's own messages (`CanAddOfferToMessage`); an
  admin with an empty price sees "You will receive 0 Stars (85%)"; the
  toggle clears the reply and puts the bar up without opening the box.
- Error toasts read "Couldn't {publish the post|decline the post|send the
  offer}: {reason}." Quill's `TdError` keeps only the error class, not
  TDLib's message, so the reason is the class text rather than tdesktop's
  server-specific strings.
- Not enough Stars for an offer: tdesktop opens its Stars purchase box for
  the missing amount. Quill shows a note and opens its Stars screen, where
  the viewer buys Stars; the offer is not resent afterwards.
- "Suggest Changes" > "Edit Message" from the channel side sends the
  changed post with `topic_id` null. Telegram routes it by the reply to the
  original suggestion; a direct messages topic id is not set yet.
- "Maximum price is ...", "Enter a number." and the time box's "Pick a
  date and time." are Quill copy; tdesktop shakes the field instead of
  showing text.
- The time box uses gpui-kit's `DatePicker` with time, not tdesktop's
  wheel picker.

## How verified

- `quill-tools/gate.sh`: GATE OK (fmt, clippy for lib and UI, core and UI
  test suites).
- Core tests (`src/suggested_posts_tests.rs`, 20): parsing of
  `suggestedPostInfo`, prices and updates, limits from options and price
  checks, button rules, the "Changes" resolution between an offer and its
  reply, block titles, bar lines and labels, Accept/Decline and submit
  wording, commission and time range, date labels, balance shortfall, and
  the input terms and draft sync.
- Driver tests (`src/connect/tests/suggested_posts.rs`, 8): the role from
  the supergroup flags, approve with the date and no second request on a
  double tap, decline with a reason and an error turning into a note, Add
  Offer carrying complete terms, a changed post sent as a new reply with
  media copied, a send with the bar up, the bar riding the draft both
  ways, and info updates changing the loaded message.
- UI tests (`src/ui/suggested_posts_tests.rs`, 7, `#[gpui_kit::test]` on
  the `ready-suggested-posts` demo): toggle and bar per role, price box
  minimum error and filling the bar, time box "Anytime" answering the price
  box, buttons only on the counter-offer, Accept Terms text and Approved
  state, Decline with a reason and Declined state, Add Offer gating.
- Screenshots of the new `ready-suggested-posts` demo at 1200x1000, light
  and dark (`after-{block,price,admin,time,accept,decline}-{light,dark}.png`
  in the scratch dir, not committed): history blocks for the offer and the
  counter-offer, decision buttons, the suggest bar, the Suggest a Message
  box, the admin's Suggest Changes box, the time box, Accept Terms and
  Decline. There is no "before" image: main has no such screen.
- Not verified against a live account: approving, declining and offering
  on a real channel's direct messages, paying in Grams, and the paid post
  delete warning timing.
