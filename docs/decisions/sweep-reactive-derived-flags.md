# Derived flags instead of synced copies

Owner rule: the UI is a pure function of state. Four flags were copies kept
in step by hand.

tdesktop has no equivalent copies: each reads the live session, the account
list or the window state when it paints.

## Camera note (audit 8)

`toggle_call_camera` built the "Camera on/off" note from the requested value. The
driver keeps the camera off when video is not ready, so the note could say
"on" for a camera that stayed off. The note now comes from the call's
resulting `camera_on` (`camera_toggle_note`). Test:
`ui/calls/camera_note_tests.rs`.

## Test environment (audit 9)

`settings.advanced.test_environment` and the `ACTIVE_IS_TEST` atomic were
refreshed after a switch or `testmode` but not after an account was removed.
Both are gone. `test_environment::active_is_test()` reads the registry's
active account and its marker file on each call; screenshot demos (no store)
keep their own `testmode` switch inside `AdvancedUi`. Test:
`active_flag_is_read_from_the_store_and_follows_removal`.

## Lost connection (audit 10)

`connection.lost` could mask `current_auth` and was cleared by hand in two
places. It is deleted. `current_auth` and the Retry card read
`live.bridge.stopped_unexpectedly()`; a new connection brings a new bridge, so
nothing needs resetting. The one-shot status note is latched by the note text
itself. Test: `auth_for_bridge` unit test. A UI-level test needs a real
`LiveConnect` (TDLib), so the stopped-bridge path is covered at the pure
function and the existing `bridge_lost` tests.

## App icon badge (audit 12)

`sync_icon_badge` was already a function of the current unread total; the
`LAST_SENT` cache only suppressed identical pushes. What was missing: the OS
can lose the badge (app re-activated, badge permission toggled) while the
count is unchanged, so the cache skipped the repair. `invalidate_icon_badge`
clears the cache and wakes the chrome loop; `wake::poke_on_activation` and
`switch_account` call it. The SNI tray already rebuilds on `wake::CHROME`
pokes, so the same wake covers it. All three platforms share the path; no
macOS-only behaviour. Test: `badge_is_repushed_after_invalidation_but_deduped_otherwise`.
