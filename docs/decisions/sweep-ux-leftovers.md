# Sweep UX leftovers: slow-mode slider, revoked-links disclosure, loading spinners

## What tdesktop does
`AddSlowmodeSlider` (`boxes/peers/edit_peer_permissions_box.cpp`) shows a
pseudo-discrete slider over eight stops (Off, 5s, 10s, 30s, 1m, 5m, 15m, 1h:
`SlowmodeDelayByIndex`), a tiny label under each stop (`lng_seconds_tiny`,
`lng_minutes_tiny`, `lng_hours_tiny`) and an explanation that turns into
"... only one message every N seconds/minutes" once a delay is chosen. The
task text lists seven stops; tdesktop and TDLib have eight (5s included), so
Quill keeps eight.

## What changed
- Slow mode: the row of outline buttons became the kit `Slider` (step 1, value
  = stop index) with the stop labels above it and tdesktop's sentence below
  (`src/ui/group_panels/slow_mode.rs`). The delay is sent on release; the thumb
  follows the server value whenever it changes. Strings are tdesktop's
  `lng_rights_slowmode_*` and `lng_*_tiny`.
- Revoked links: the title and the bare "Show"/"Hide" button became one
  disclosure row (chevron plus "Revoked links"); "Show"/"Hide" is its tooltip.
- Loading: the administrators list and both invite-link blocks in the
  per-admin view now show the kit spinner like the invite-link and
  join-request sections already did.

## Findings re-checked and skipped
- Session rows: Terminate and Details already share one right-aligned
  trailing row (fixed earlier); nothing left to align.
- Invite-link and join-request spinners in the group info panel: already there.
- Chat-row tab stops: skipped. Keyboard selection in the chat list needs a
  separate highlighted-row concept, scroll-into-view in the virtual list and a
  focus model that does not fight `select_chat_with` moving focus to the
  composer; that is a large change to the chat list.

## Verified
GPUI UI tests (`slow_mode_tests.rs`, `invite_disclosure_tests.rs`), pure tests
for stops/labels/sentences, demo captures of `ready-group-info-edit` in light
and dark (kept in the scratch directory, not committed). Not exercised against
a live account: `setChatSlowModeDelay` goes through the existing path.
Labels use equal spacing (`justify-between`), tdesktop positions each label at
its exact stop; the first and last labels align with the track ends.
