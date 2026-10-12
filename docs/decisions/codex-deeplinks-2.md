# Boost, voice chat and gift code entry points

## What tdesktop does
- Boost links (`t.me/boost/<name>`, `?boost`, `t.me/c/<id>?boost`) call `resolveBoostState`: `getBoostsStatus`, then `Ui::BoostBox` over the current view (the channel itself is not opened). The box shows the level, the boosts missing, what the next level unlocks and one button. The button runs `applyBoost`: it reads the viewer's slots (`ParseForChannelBoostSlots`: free, already on this chat, other). A free slot boosts at once. Otherwise: other slots open `ReassignBoostsBox` (a list, or a single "boost X instead?" confirmation, or a "can't boost too often" box when the only slot cools down); only "already" shows `BoostBoxAlready`; a viewer without Premium gets `PremiumForBoostsBox` (Yes opens Premium); gifted Premium gets `GiftedNoBoostsBox`.
- `?voicechat=`, `?videochat=`, `?livestream=` on a channel link open it and run `joinVoiceChatFromLink`: refresh the channel, no call means the toast "This invite link is invalid or has expired.", otherwise `startOrJoinGroupCall` with the hash and `JoinConfirm::Always` (a confirmation, a join-as choice when needed, a leave-current-call confirmation).
- `ResolveGiftCode` is called from the link, the giveaway info "View prize" button, the Premium gift code service message button ("Open Gift Link") and a boosts-list row that carries a code. There is no manual code entry.

## What changed
- `src/boost_box.rs` (pure): counters, titles and texts, slot sorting, the decision, the reassign list rules, box state. `connect/groups/boost_flow.rs` and `boosts.rs` send the requests; `ui/boost_box_ui.rs` draws the box. `getAvailableChatBoostSlots` now parses full slots. The old one-tap flow took the first slot blindly, which could silently move a boost from another chat; it is gone. The boost link, the channel menu entry and the info panel button all open the box. Only the Boost button reads slots, and only a boost, Reassign or Replace click sends `boostChat`.
- Call links: `internalLinkTypeVideoChat` keeps its hash and routes to `DeepLinkAction::OpenVideoChat`. `src/call_link.rs` plans the step (not a group, no call, already joined, busy, offer). The driver fetches the call (`getGroupCall`) and stores the hash, which `joinVideoChat` sends. The call screen's Join button is the confirmation, so nothing joins or opens the microphone from the link.
- Gift codes: the Premium gift code card has an "Open Gift Link" button and boost rows with a code open the gift code box. The code is kept in `SecretCode`, which `Debug` never prints.

## Follow-up in the same PR
- Leave-and-join: a call link while another call is on raises tdesktop's `confirmLeaveCurrent` question (the six texts, "End call" or "Leave"). Confirming ends the 1:1 call (`discardCall`) or leaves the group call, then offers the link's call once the old one is gone (`maybe_offer_pending_call_link`, run from `ingest` like the swap prompt). Cancel does nothing.
- Gift lines: TDLib 1.8.68 has `getApplicationConfig` (returns the server's JSON, which carries `boosts_per_sent_gift`; the schema does not list the keys). The box asks for it once on open. With a value above 0 it shows `GiftForBoostsBox` ("More boosts needed"), the gift part of the reassign and cooldown texts, and "Boost Again". With 0 or no answer it behaves as before.

## Not done
- Fireworks, user pictures in the reassign list, a live countdown for cooling-down rows (it updates on redraw).
- The join-as picker is the call screen's existing one.
- tdesktop's "pending gift code" box for boost rows without a code.

## Verified
Unit tests for slot sorting, decisions, texts, selection, link parsing and routing, call link plans, request JSON (`boostChat` slots, `joinVideoChat` hash), driver flows (open reads only, free slot, reassign needs a second click, cooldown, notices, errors, close), and GPUI tests for the box phases. Temporary demo captures (not committed) of the main box and the reassign list were viewed. Not run against a live account: nothing was boosted, joined or applied.
