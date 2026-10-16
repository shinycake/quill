# Emoji and message effect timers run on the executor clock

Follow-up to `sweep-send-effects-test-flake`.

## What tdesktop does
`Reactions::EffectsManager`/`HistoryView::Element` effects wait for their sticker (`kMaxDelay`, 2 s here as `START_WAIT`) and end when their view is gone. Nothing there depends on a clock a test could miss.

## Change
In `src/ui/emoji_effect.rs` the timers that decide whether an effect lives now read `cx.background_executor().now()`: `Active::queued` (the 2 s / 30 s sticker wait), `MessageEffectPlay::missing` (the 400 ms badge grace). `push_premium` and `push_message` take the `now` instant from their callers, which hold a `cx`. The comparisons are `saturating_duration_since`, in the helpers `sticker_wait_over` and `badge_within_grace`. In the app the executor clock is the real clock, so behavior is unchanged; in a GPUI test it only moves when the test advances it.

Left on the real clock on purpose: `LayeredClip::started` and the `shown` cell (`clip.used`), which the sticker cache compares with real `Instant`s, and the seed's nanosecond reading. An executor instant is never compared with a real one.

The overlay still calls `request_animation_tick_for(None, TICK_FPS, cx)` on every frame while any effect is active, so the timers keep being evaluated in the app.

## Verified
Unit tests for both boundaries (just before and at the grace and the wait) and a UI test that advances the executor clock past the grace with the badge in view. 30 runs of the badge and effect tests with 32 and with 128 busy loops: 30/30 before and after. The flake was never reproduced for this file, so this is a hardening, not a fix of an observed failure.
