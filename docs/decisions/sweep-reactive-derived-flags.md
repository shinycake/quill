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

## Review round

- Test-environment flag: `AdvancedUi::test_environment()` read the registry JSON and the marker file on every render. The active account's flag now lives in memory (`ACTIVE_TEST` in `src/advanced/test_environment.rs`), computed on the first read and served from memory afterwards. Every registry write goes through `save_registry`, which invalidates it, and `mark_test_environment` does too, so `set_active_account`, `remove_account`, `add_account`/`add_environment_account` and the notify toggle all refresh it. An epoch counter stops a read that raced a write from storing an old value. Tests: removal flips the flag; add, switch and switch back follow, and a second read is proven to come from memory (the marker file is deleted under it).
- Connection-closed note: the latch compared the note text, so any other note re-armed it. It now lives on the connection (`ConnectionUi::claim_closed_note`, `closed_noted_generation`), true once per connection generation. A new connection bumps `generation` (`start_connection`, account switch), which re-arms it; other notes cannot. Tests: it fires once, waits for the drained queue, and a replaced note is kept while the bridge stays stopped.
- Clean logout: `StopGuard` (`src/telegram/hub.rs` around L116-126, `src/telegram/client.rs` around L40-50) only sets `stopped` when the receive loop ends without a shutdown request. Neither loop exits on a Closed authorization update (`receive_loop` hub.rs L278-300 only exits on shutdown or 16 consecutive panics; `injected_loop` client.rs L146 only on shutdown or a dead channel), and the shared hub is never shut down. So logout cannot read as a lost connection. Test: an injected Closed update is delivered and leaves the bridge not stopped.
- The camera note test was not strengthened: the call site needs a live driver.
