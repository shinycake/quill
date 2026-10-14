# Gift auctions, crafting and resale filters

Parity item `parity:premium-gift-auctions`. Design by the architect session
(2026-10-14) for branch `gift-auctions-craft-resale`; copy this file to
`docs/decisions/gift-auctions-craft-resale.md` in that branch and fill in
"How verified" at the end. Build it after `suggested-posts` has merged.

## What tdesktop does

Reference: `boxes/star_gift_auction_box.cpp` (2,100 lines),
`boxes/star_gift_craft_box.cpp` (1,770), `boxes/star_gift_resale_box.cpp`
(840), `boxes/star_gift_box.cpp` (`SendGiftBox`, gift tiles, resale and
auction click handling around lines 4600–4800 and 5195–5300),
`info/peer_gifts/info_peer_gifts_common.cpp` (tile badges),
`dialogs/suggestions/suggestion_gift_auctions.cpp` (chat-list bar),
`data/components/gift_auctions.h`, strings `lng_auction_*`,
`lng_gift_craft_*`, `lng_gift_resale_*`, `lng_gift_buy_resale_*`,
`lng_gift_stars_auction*`.

### 1. Resale listings and filters

Entry points: in the send-gift box a sold-out limited gift that has resale
listings shows its tile with the badge "resale" (green) and the price line
"{min_resale}+" (star icon); clicking it opens the **resale box** instead
of the send box. (A collectible someone owns can also be bought from its
own gift box; Quill already has that "buy by link" path in
`marketplace.rs`.)

Resale box (`GiftResaleBox`, width `boxWideWidth`):
- Pinned header: the gift title (box title style) and under it, muted,
  "{count} gifts in resale" ("No listings" when 0); the count updates with
  the filter.
- A horizontally scrollable row of pill **filter tabs** (filled pills,
  each ending with a small dropdown chevron):
  1. the sort: "Price" / "Date" / "Number" with a small icon; its menu:
     "Sort by Price", "Sort by Date", "Sort by Number" (one checked), a
     separator, "All Listings" / "For Stars Only" (one checked).
  2. "Model" (or "{n} Models" when some are selected); menu lists every
     model with its sticker as the icon and the count bold after the
     name, sorted by count descending, each a checkbox; when any is
     selected the menu starts with "Select All" (clears the filter).
     Unselected items read as checked while nothing is selected.
  3. "Backdrop" / "{n} Backdrops": same, icon is a color swatch of the
     backdrop center color.
  4. "Symbol" / "{n} Symbols": same, icon is the symbol sticker.
- The grid of listings: gift tiles (collectible preview on its backdrop,
  "#number" badge, price line with star icon, or the TON icon when the
  seller accepts only Grams); infinite scroll by `next_offset`.
- Bottom: left button "For Stars Only" / "All Listings" (toggles the same
  flag as the sort menu), right button "Back".
- Empty filter result: "No gifts found for your search."
- Clicking a listing opens the **buy box** ("Buy {name}", the collectible
  cover, a table Model / Backdrop / Symbol with rarity per-mille, button
  "Buy for {cost}"; when the seller allows both currencies a toggle "Pay
  in Stars" / "Pay in Grams", and the hint "Equals to {cost}"; Grams-only
  listings say "The seller only accepts Grams as payment."). Confirm:
  "Do you want to buy {name} for {price} and gift it to {user}?" (or "...
  for {price}?" for self). On `giftResaleResultPriceIncreased` the box
  re-shows with the new price; the sent gift lands as a message.

### 2. Auctions

Some limited gifts are sold by auction (`gift.auction_info`). Rounds of a
fixed duration drop `gifts_per_round` gifts to the top bidders; a bid that
drops out of the top carries over to the next round; losing bids are
refunded at the end.

Tiles in the send-gift box: badge "auction" (credits color) or "soon"
before `start_date`; button text "Join" (or "View" when sold out or
upcoming) instead of the price.

Clicking the tile (`ShowStarGiftAuction`) loads the auction state, then
chooses one box:
- the user already bid and the auction is running → **bid box**;
- the user already bid for *another* recipient → confirm "Change
  Recipient": "You've already placed a bid on this gift for {name}. Do you
  want to raise your bid and change the recipient to {name}?" with
  "Change" (opens the send box for this recipient);
- not started, finished, or the about box was already shown once →
  **auction info box**;
- otherwise the **about box** first (shown once per account, remembered
  locally), then the info box.

**About box** ("Auction", "Join the battle for exclusive gifts."): three
feature rows: "Top {n} Bidders" / "{n} gifts are dropped in {rounds} to the
top {n} bidders by bid amount."; "Bid Carryover" / "If your bid leaves the
top {n}, it will automatically join the next round."; "Missed Bidders" /
"If your bid doesn't win after the final round, your Stars will be fully
refunded."; button "Understood".

**Auction info box** (gift-box style, no content margin): the collectible
cover cycling random model/backdrop/symbol combinations every 1 s
(pretitle "Upcoming Auction" before start; subtitle link "Learn more about
Telegram Gifts →"); a close and a "⋯" menu ("About", "Copy Link", "Share",
link `t.me/auction/{slug}`); a table:
- before start: "Starts" date, "Ends" date, "Availability" total,
  "Rounds" count, then one row per distinct round setting: "Round {n}" /
  "Rounds {n}-{m}" = "{d} minutes each" (+ " + {x} for late bids in top
  {k}" when the round extends);
- running: "Started", "Ends", "Current Round" = "{n} of {m}",
  "Availability" = "{left} of {total} left";
- finished: "First sale", "Last sale", "Average Price" with a tooltip
  "{amount} is the average sale price for {gift} gifts."
- a link "{n} {emoji} items bought →" when the user won gifts (opens the
  **bought box**: "{n} Items Bought", per gift a table "{name} in round
  {n}", "To" peer, "Date", "Accepted Bid" = stars + "#position" badge,
  and the message text) or, before start, "View {emoji} {n} Variants →"
  (opens the model preview);
- the bottom button is two-line: "Join Auction" / "{h} h {m} m left"
  (minutes tick), "Place an Early Bid" / "starts in h:mm:ss" before
  start, "OK" when over. It opens the send-gift box for this gift.

**Send box for an auction gift**: the usual gift send box (message, hide
name) but its button reads "Place a Bid" / "{time} left" and the limited
slider is replaced by the text "{n} gifts are dropped to the top {n}
bidders by bid amount. Learn more →". Submitting opens the **bid box**.

**Bid box** (wide, paid-reaction style): title "Place a Bid" ("Place an
Early Bid" before start), subtitle "Top {n} bidders will win", a slider
with a value bubble (min = `min_bid` or the user's `next_bid_star_count`,
max = max(30,000, bid × 1.2, top bid × 1.2); the bubble subtext is "your
bid" at the current bid, "click to bid more" at the max, "+{delta}" when
raising), a "+" button and clicking the bubble open "Custom Amount" (a
Stars field, "Save"/"Cancel"). Three info blocks: min bid ("minimum bid",
click sets it), countdown "until next round" / "before start", "{emoji}
{left}" "left". Then "Your bid will be" / "You're winning" / "You've been
outbid" (green when position ≤ gifts per round, red otherwise) with the
user's row (place number, avatar, name, stars), and "Top winners": three
rows with 🥇🥈🥉. Button: "Place a {stars} Bid" or "Add {delta} to Your
Bid" ("OK" when not raising); balance line at the bottom. Success toast:
"Your bid has been placed" / "Your bid has been increased" + "If you fall
below the **top {n}**, your bid will roll over to the next round."
A "⋯" menu (About / Copy Link / Share) while running. When the user wins
a gift the box closes and the chat with the gift opens.

**Chat-list bar** (top suggestions slot): while the user has active bids:
title "{emoji} Active Auction" (or up to three emoji + "{n} Active
Auctions"), text "You're winning ({n}th place)." / "You've been outbid."
(red) / "You're winning in all of them." / "You've been outbid in {n} of
them." / "... in all of them."; right button: a countdown to the next
round for one auction, "View" for several (opens a list box "{n} Active
Auctions": per auction "Round {n} of {m}", the gift sticker, "Your bid
{stars} is ranked #{n}" / "... is outbid", and a full-width "Raise Bid
h:mm:ss" button).

Link preview for `t.me/auction/...` messages: button "Join" / "View
Results"; the deep link opens the auction.

### 3. Crafting

A collectible the user owns with `craft_probability_per_mille > 0` (not a
channel's gift) gets "Craft" in its "⋯" menu (also the gift tile shows
"{percent}" at a corner in craft contexts).

**Craft info box** (first): the gift cover titled "Gift Crafting",
subtitle "Turn your gifts into rare, epic and legendary versions.", three
rows: "Get Rare Models" / "Select up to 4 gifts to craft a new exclusive
model."; "Maximize Chances" / "Combine more gifts to increase your odds of
success."; "Affect the Result" / "Use gifts with the same attribute to
boost its chance."; button "Start Crafting".

**Craft box** ("Craft Gift", white text on a dark gradient backdrop whose
colors come from the chosen gifts): a 2×2 grid of slots; filled slots show
the gift tile with a "{percent}" badge (its own chance) at one corner and
a remove "×" at the other; empty slots are "+" buttons. Text: "Add up to
**4** gifts to craft new {emoji} **{name}**." / "If crafting fails, all
used gifts will be lost." Under it a row of up to eight attribute chips
(backdrops first, then symbols, sorted by how many chosen gifts carry
them) each with a radial percent "{p}% chance the crafted gift will have
**{name}** background/symbol." The chance per attribute comes from
`giftsForCrafting.attribute_persistence_probabilities[total-1]
.persistence_chance_per_mille[count-1]` (1000 ‰ when out of range). A
pill "{3 random crafted model emoji} View all craftable models →" opens
the model preview filtered to rare/epic/legendary models. The button is a
gradient two-line button "Craft {name}" / "{sum of chances}% Success
Chance".

Clicking "+" or a filled slot opens **Select Gifts**: section "Your
gifts" (the user's other collectibles of the same collection,
`getGiftsForCrafting`; already chosen ones are marked), the hint "You
don't have other gifts from this collection." when empty, then "{n}
suitable gifts on sale" with the resale list in craft mode (filters
without counts, only non-rare models, Stars/TON) whose tiles buy the
listing and put it in the slot. Choosing a gift that cannot be crafted
yet shows "Crafting unavailable" / "This gift will become available for
crafting on {date} at {time}."; a gift exported to the blockchain in the
first slot shows "This gift has been exported to the blockchain and can't
be used as the primary gift for crafting."

Crafting runs `craftGift(received_gift_ids)` under a forge animation
("Crafting", "Crafting **{name}**", "{p}% success chance", "If crafting
fails, all used gifts will be lost."). Result: success shows the new gift
(its box) with "Craft Another Gift"; failure shows "Crafting Failed" /
"This crafting attempt was unsuccessful. **{n}** gifts were lost." with
"Craft New Gift"; `craftGiftResultTooEarly` shows the "available on
{date}" box; `craftGiftResultInvalidGift` toasts.

## TDLib mapping (pinned schema)

| Need | TDLib |
|---|---|
| auction flag per gift | `gift.auction_info: giftAuction{id, gifts_per_round, start_date}` (`getAvailableGifts`) |
| auction state | `getGiftAuctionState(auction_id)` → `giftAuctionState{gift, state}`; `auctionStateActive{start_date, end_date, min_bid, bid_levels[auctionBid{star_count, bid_date, position}], top_bidder_user_ids, rounds[auctionRound{number, duration, extend_time, top_winner_count}], current_round_end_date, current_round_number, total_round_count, distributed_item_count, left_item_count, acquired_item_count, user_bid: userAuctionBid{star_count, bid_date, next_bid_star_count, owner_id, was_returned}}`; `auctionStateFinished{start_date, end_date, average_price, acquired_item_count, telegram_listed_item_count, fragment_listed_item_count, fragment_url}` |
| live updates | `openGiftAuction(gift_id)` while a box is open, `closeGiftAuction` on close; `updateGiftAuctionState`; `updateActiveGiftAuctions{states}` for the chat-list bar |
| bid | `placeGiftAuctionBid(gift_id, star_count, user_id, text, is_private)`, `increaseGiftAuctionBid(gift_id, star_count)` |
| won gifts | `getGiftAuctionAcquiredGifts(gift_id)` → `giftAuctionAcquiredGift{receiver_id, date, star_count, auction_round_number, auction_round_position, unique_gift_number, text, is_private}` |
| cover variants, model preview | `getGiftUpgradePreview(regular_gift_id)` → models / symbols / backdrops |
| link / deep link | `linkPreviewTypeGiftAuction{gift, auction_end_date}`, `internalLinkTypeGiftAuction{auction_id}` |
| resale search | `searchGiftsForResale(gift_id, order: giftForResaleOrderPrice/PriceChangeDate/Number, for_crafting, for_stars, attributes[upgradedGiftAttributeIdModel{sticker_id}/Symbol{sticker_id}/Backdrop{backdrop_id}], offset, limit)` → `giftsForResale{total_count, gifts[giftForResale{gift: upgradedGift, received_gift_id}], models[upgradedGiftModelCount], symbols[..], backdrops[..], next_offset}` (the count lists come only on the first page without attributes) |
| buy | `sendResoldGift(gift_name, owner_id, price, text, is_private)` → `giftResaleResultOk / PriceIncreased` (already in `marketplace.rs`) |
| craft candidates | `getGiftsForCrafting(regular_gift_id, offset, limit)` → `giftsForCrafting{total_count, gifts[receivedGift], attribute_persistence_probabilities[4], next_offset}` |
| craft | `craftGift(received_gift_ids)` → `craftGiftResultSuccess{gift, received_gift_id} / TooEarly{retry_after} / InvalidGift / Fail` |
| per-gift craft data | `upgradedGift.craft_probability_per_mille`, `is_crafted`, `is_burned`, `upgradedGiftOriginCraft`; `receivedGift.craft_date` |
| limits | options `gift_resale_star_count_min/max`, `gift_resale_toncoin_cent_count_min/max`, `gift_resale_star_earnings_per_mille`, `craft_attribute_permilles` (fallback when `attribute_persistence_probabilities` is empty) |

Everything exists in the pinned TDLib; nothing is blocked. The tile text
"Transactions in Stars may be reversed" etc. already exist in
`gift_owner_actions.rs`.

## Design for Quill

### Core (`quill` lib)

- `src/gift_market/` (new directory, split by concern, each file < 1,000
  lines): `resale.rs` (`ResaleFilter { sort, stars_only, attributes:
  BTreeSet<AttributeId> }`, `ResaleListing`, `ResalePage` with the count
  lists, `to_request_json`, tab labels "Model" / "{n} Models" ...),
  `auction.rs` (`AuctionState` mirroring `auctionStateActive/Finished`
  with `my: Option<MyBid>`, `finished()`, `my_position()`, `bid_slider(
  state) -> { min, mine, max }` with tdesktop's max rule, `bid_rows(
  state, chosen) -> { my_row, top3 }`, the countdown helpers
  `seconds_left`, `nice_countdown` ("h:mm:ss" / "m:ss"), the chat-list
  bar texts), `craft.rs` (`CraftSlot`, `CraftSession { slots: Vec<Gift>
  (≤ 4), probabilities }`, `success_per_mille`, `attribute_chances`,
  `craft_result` parsing, the "available on {date}" gate), `tests.rs`.
- `premium_hub::GiftInfo` grows the collectible attributes it lacks:
  model, symbol and backdrop as `UpgradedGiftAttribute { name, sticker,
  rarity_per_mille }` (+ backdrop id and colors), `craft_per_mille`,
  `is_crafted`, `regular_gift_id`, `name` (slug). Keep the parse in a new
  `premium_hub/upgraded.rs` to stay under the size limit.
- `gift_manage::AvailableGift` grows `auction: Option<GiftAuctionInfo {
  id, gifts_per_round, start_date }>`; `SendGate::Auction` becomes a
  real state (the tile opens the auction instead of refusing).
- Payments domain: `PaymentsPurpose::GiftMarket(GiftMarketPurpose)` with
  `SearchResale { gift_id, page }`, `BuyResale`, `AuctionState { gift_id
  }`, `OpenAuction`, `CloseAuction`, `PlaceBid`, `IncreaseBid`,
  `AcquiredGifts`, `UpgradePreview { gift_id }`, `GiftsForCrafting {
  gift_id, page }`, `Craft`; `PaymentsPayload::GiftMarket(
  GiftMarketPayload)` with the answers and the two updates
  (`updateGiftAuctionState`, `updateActiveGiftAuctions`). Parse in
  `envelope/domains/payments/parse.rs` by delegating to
  `gift_market::parse_payload` (keeps the hub file small).
- State: `PaymentsState.gift_market: GiftMarketState` (resale pages by
  gift id + filter, auction states by gift id, active auctions, craft
  session, open box, errors, `about_shown` persisted through the existing
  local settings store).
- `connect/gift_market.rs`: one driver method per request; subscribe /
  unsubscribe (`openGiftAuction` / `closeGiftAuction`) tied to the UI box
  open/close; driver tests in `connect/tests/gift_market.rs`.
- `telegram/requests/gift_market.rs`: JSON builders (int64 ids as
  strings).
- Deep link `t.me/auction/<slug>` and `internalLinkTypeGiftAuction`:
  resolve through `getGiftAuctionState(auction_id)` then open the box.

### UI

- `ui/gift_market_state.rs` (`GiftMarketUi`): slider state
  (`SliderState`), custom-bid `InputState`, filter menu state, craft
  slot selection, countdown tick registration.
- `ui/gift_resale_ui.rs`: resale box (`Dialog` wide), the filter pill row
  (kit `Button` pills with `DropdownMenu` / `PopupMenu` items with
  checkmarks; model and symbol items use the sticker thumbnail via
  the existing sticker tile helper, backdrops a color dot), the grid
  (reuse `premium_ui::gift_tile` with a price line), infinite scroll
  through the existing list-end hook, the buy box (reuse
  `ui/marketplace.rs`).
- `ui/gift_auction_ui.rs` + `ui/gift_auction_bid.rs`: about box, info
  box (table via `DescriptionList` or the existing `fact_row`; cover
  cycle driven by the frame clock at 1 fps, never a repeating
  `with_animation`), bought box, bid box (kit `Slider` with a value
  bubble, `Avatar` rows, `Badge` for "#n"), the send-box changes (button
  text + about text), the chat-list bar entry in
  `quill::chatlist_suggestions` (new `Suggestion::GiftAuctions`) and its
  card in `ui/suggestions_block.rs`, the "Active Auctions" list box.
  Countdowns: a 1 s tick from the frame clock while any auction box or
  the bar is visible; text from `nice_countdown`.
- `ui/gift_craft_ui.rs` + `ui/gift_craft_select.rs`: info box, craft box
  (2×2 slots, attribute chips with `ProgressCircle` for the radial
  percent, gradient button), select-gifts box (own gifts grid + resale
  list in craft mode), result boxes. The forge animation: a simplified
  version (glow pulse + spinning progress ring + the lost gifts fading,
  frame clock, ~3 s) instead of tdesktop's 2,000-line particle forge;
  record this in Deviations.
- "Craft" item in the gift "⋯" menu (`gift_owner_actions.rs`) and the
  per-tile "{percent}" badge in craft contexts.
- Link preview card for `linkPreviewTypeGiftAuction` ("Join" / "View
  Results" button) next to the existing upgraded-gift preview.

### Tests

- Unit: filter JSON and tab labels; auction slider bounds, position,
  rows, countdown text, bar texts (1 vs many, winning vs outbid); craft
  sum and attribute chances with the probability table and the
  out-of-range fallback; result parsing; price-increased handling.
- Driver: search with attributes and order; place / increase bid;
  open/close auction on box open/close; craft; acquired gifts; the
  `updateActiveGiftAuctions` → bar.
- GPUI UI tests: resale box opens from a sold-out tile with listings and
  the sort menu changes the pill; bid box slider sets the button text;
  craft box adds and removes a slot and updates the chance; the chat-list
  bar shows for an active auction.
- Visual: temporary captures of the resale box with filters, the bid box,
  and the craft box, light and dark.

## Build plan (one PR, branch `gift-auctions-craft-resale`)

1. Core parse + state + requests (resale, auction, craft) with tests.
2. Resale box + filters + buy (tile entry points).
3. Auction boxes (about, info, bid, bought), send-box changes, chat-list
   bar, link preview, deep link.
4. Crafting (info, craft, select, result, simplified forge).
5. UI tests, captures, decision doc, `parity-fragments/
   gift-auctions-craft-resale.txt` = `parity:premium-gift-auctions`.

If the PR grows past ~6,000 lines, split step 4 into its own PR
(`gift-crafting`) and tick the parity id only with the last one.

## Deviations from tdesktop

- Split in two PRs. This one (`gift-auctions-craft-resale`) ships the
  core for all three features (parsing, state, requests, driver) plus the
  resale and auction UI. The crafting boxes (info, craft, select, result,
  forge) follow in `gift-crafting`, which also ticks
  `parity:premium-gift-auctions`. The whole thing passed ~6,000 lines.
- Forge animation simplified (crafting PR).
- The "Learn more about Telegram Gifts" link opens Quill's Premium
  features page (gifts section) like tdesktop's premium preview.
- Resale listings load page by page with a "Show more" button under the
  grid instead of tdesktop's load-on-scroll.
- The chat-list "Active Auction" bar is drawn first by
  `suggestion_card` (tdesktop gives it the top `Priority::GiftAuctions`)
  rather than as a new `Suggestion` enum variant, which is a hotspot.
- The auction box's "⋯" menu has "About" and "Copy Link"; "Share" (the
  forward box) is left out.
- "Custom Amount" opens inline in the bid box instead of a separate box.
- The info box's "{n} items bought ›" link drops the gift emoji tdesktop
  puts in front of the count; the text is a kit ghost button label, which
  cannot hold a custom emoji.
- The "⋯" menu sits in the dialog's title row, left of the kit's close
  button, and is hidden once the auction has finished, as in tdesktop. The
  bid box has only close and its main button (no "Back").
- "About shown" is stored once for the app (`gift_market.json`), not per
  account.
- Gift stickers that are not downloaded yet show the gift glyph; the demos
  have no sticker files, so every capture shows it.
- The attribute filter popups cannot be captured statically, so the
  resale capture shows the pills with a model filter applied
  ("2 Models").

## How verified

- `quill-tools/gate.sh`: `GATE OK core=4373 0 ui=667 0` (fmt, clippy with
  `-D warnings`, core and UI tests).
- Unit tests in `src/gift_market/tests.rs` (filters, sort, attribute
  counts, auction phases, bid rows, bid actions; the bid slider follows
  tdesktop's `StarSelectDiscreterForMax` scale, so a ★4,000 bid sits near
  40% of a ★30,000 track, positions round-trip exactly, and dragging
  below the minimum stops at the user's own bid like `PaidReactionSlider`),
  driver tests in `src/connect/tests/` (resale search, buy with price
  change, auction state and bids: place vs increase for the same
  recipient), deep link parse, and link preview button labels.
- UI tests (`src/ui/gift_market_tests.rs`, `#[gpui_kit::test]`): resale
  filters, sort and Back; listing to buy steps; the bid slider sets the
  button label ("Add ★26,000 to Your Bid", the minimum-bid block → ★4,050);
  About → Info; the chat-list bar shows while a bid runs, and one click
  on its button opens the bid box once (the button stops the click from
  reaching the bar; the test fails with the old double handler).
- Review fixes: `#![deny(clippy::indexing_slicing)]` on `gift_market` and
  every gift UI module; the value bubble rides above the slider thumb; the
  info box no longer adds "Top N bidders…" / "Auction ended." under the
  cover (tdesktop shows only the "Learn more" link there); sticker
  downloads go out only when the set of needed files changes, not every
  frame.
- `scripts/check-hotspots.sh origin/main` and `scripts/check-file-size.sh`
  pass.
- Captures in light and dark through the demo-capture build
  (`ready-gift-resale`, `ready-gift-auction` with `QUILL_DEMO_MARKET` =
  `bid`, `info`, `about`, `send`, `active`, `bought`, and the resale
  `confirm` step). Looking at them found two bugs, both fixed: the about
  box cut its lines off instead of wrapping, and the minimum-bid block
  used a plain star instead of the gold Star icon.
- Not verified against a live account: no bid, buy or resale request was
  sent to Telegram.
