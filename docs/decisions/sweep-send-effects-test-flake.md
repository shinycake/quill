# Send-effect card fade runs on the executor clock

`ui::send_effects::tests::a_click_on_the_card_hides_it_and_one_outside_closes_the_menu` failed when the machine was loaded.

## What tdesktop does
The "Send with Effect" card (`ChooseSendEffect` preview) fades with a short animation
(`_shownAnimation`, 400 ms here as `PREVIEW_TOGGLE_MS`) and is removed when it ends. Nothing there depends on a wall clock a test could miss.

## Cause
A click on the card sets `Preview::hiding` to `Instant::now()`. The card is dropped, and its tracked bounds cleared, once
`hiding.elapsed()` passes 400 ms of real time. The kit menu's "press outside" handler runs after the click is dispatched
and asks whether the pointer is still over the menu, the strip or the card. With the CPUs oversubscribed, more than 400 ms of
real time passed before that check, the card was already gone, the press counted as outside, and the menu closed. The test
then failed at `expect("still open")`.

## Change
`Preview::shown`, `Preview::hiding` and the fade's `progress` now read `cx.background_executor().now()` instead of
`Instant::now()` / `elapsed()`. In the app that is the real clock, so the fade is unchanged. In a GPUI test it is the
simulated clock, which only moves when a test advances it. The assertions are unchanged. A new test,
`a_hidden_card_is_dropped_once_its_fade_has_run`, advances the executor clock by the fade length and checks the card is
dropped while the menu stays open, so the fade itself stays covered.

`Preview::playing` still uses the real `Instant`: it is the start of the sticker animation, which `LayeredClip` reads on
the real clock, and no test asserts on it.

## Other tests
The whole `send_effects` module, then the whole ui suite (1135 tests), ran under load. No other test failed. The
message-effect badge test reads real `Instant`s in `emoji_effect.rs` (a 2 s sticker wait and a 400 ms badge grace) but
passed every run, so it was left alone.

## Verified
30 runs of the test with 32 busy loops on 8 cores: 12 of 30 passed before, 30 of 30 after. The whole ui suite passed
3 of 3 runs under the same load.
