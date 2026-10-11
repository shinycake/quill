# Channel auto-translate and Switch Off Ads toggles

## What tdesktop does
- `boxes/peers/edit_peer_info_box.cpp` `fillAutoTranslateButton`: a "Auto-translate messages" toggle for broadcast channels when `canEditAutoTranslate()` (broadcast and `canEditInformation()`); locked behind a boost level (`channelAutoTranslateLevelMin`). Quill already had this row (`set_auto_translate`, `toggleSupergroupHasAutomaticTranslation`, rollback on refusal); no change needed.
- There is no sponsored-messages toggle in the edit box. The channel owner's "Switch Off Ads" switch lives on the channel earn page (`info/channel_statistics/earn/info_channel_earn_list.cpp`, `Api::RestrictSponsored`), locked until `channelRestrictSponsoredLevelMin` boosts; a failure shows the error as a toast and flips the switch back.

## What changed
- `supergroupFullInfo.can_have_sponsored_messages` is parsed into `SupergroupFullAdmin`.
- New request `toggleSupergroupCanHaveSponsoredMessages` (TDLib 1.8.68, owner only), driven through the existing optimistic `GroupToggle` path.
- The channel settings dialog shows "Switch Off Ads" only to the channel owner once the full info is loaded. A refusal rolls the switch back and reports "the channel needs a higher boost level" for error 400 (TDLib's message text is deliberately not stored), else the error code.
- Quill has no earn page, so the row sits in the channel settings dialog next to the other owner toggles.

## Verified
Unit tests: request JSON, controls gating, driver optimistic update and rollback with error line. `gate.sh` passes. Not visually captured: the dialog needs a live owner session and no demo kind reaches it; no real channel was changed.
