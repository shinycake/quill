# Story Stealth Mode box

## What tdesktop does

`media/stories/media_stories_stealth.cpp` (tdesktop 7.3.3):

- **Box.** A round logo, the title "Stealth Mode", an about line, and two feature rows: "Hide Recent Views" (past 5 minutes) and "Hide Future Views" (next 25 minutes). The 5 and 25 minutes are hard-coded in the language strings. The about line for a free account is the "Subscribe to Telegram Premium" variant.
- **Button states.** Free account: "Unlock Stealth Mode" with a lock. Premium: "Enable Stealth Mode", or "Enable and open the story" when the box came from a story-strip entry. While cooling down: "Available in m:ss", dimmed, counting down every second.
- **Presses.**
  - Unlock opens the Premium preview at the stories feature and keeps the box open.
  - A press during the cooldown shows the "Please wait until **Stealth Mode** is ready..." toast and closes the box.
  - Enable sends `activateStoryStealthMode`. The box stays open until `updateStoryStealthMode` reports it active, then closes and shows the "Stealth Mode On" toast. The stories open if the box was started from the strip.
- **Already on.** An entry point skips the box and shows the "You are in Stealth Mode" toast with the time left (h:mm:ss / m:ss).
- **Story under the box.** The story is paused while the box is open.
- **Entry points.**
  - Story strip context menu: "View Anonymously" (`AddStealthModeMenu`). Shown for a user peer with unread stories and no live stream, when Premium is possible.
  - Story viewer menu: "Hide My View" (`lng_stealth_mode_menu_item`). Shown for a user's story that is not your own and not live, when Premium is possible.
  - Both show a lock icon when the account is not Premium and stealth is not active.
- **Toasts.** 4 seconds, max width 340.

## What changed

- `src/story_stealth.rs` is a pure core that decides everything:
  - button state (`StealthButton::of`) and press result (`press`);
  - time left (`active_left`, `time_left_text`) and the menu lock (`StealthEntry::of`);
  - toasts (`StealthToast`) and the periods (`StealthPeriods`).
  It has no UI.
- **Periods.** They come from the TDLib options `story_stealth_mode_past_period` and `story_stealth_mode_future_period` (session `stories.stealth_periods`, set in `UpdateOption`). While they match tdesktop's 5 and 25 minutes the tdesktop strings are used unchanged. Otherwise Quill-only strings (`qll_stealth_*`) give the server's values.
- **The box.** `src/ui/story_stealth_ui.rs` is a kit `Dialog` registered at priority 4030, so the Premium dialog (4000) covers it and it comes back afterwards.
  - A once-a-second tick runs while it is open. The tick task is dropped when the box closes.
  - The poll-loop drain closes the box when TDLib reports stealth active.
  - "Requested" is derived from the request registry (`has_purpose(ActivateStoryStealthMode)`).
  - A refused request frees the button, and tdesktop shows nothing either. The old `stealth_error` status string is gone.
- **Entry points.** The viewer ⋯ menu's old direct-send "Stealth" item is now "Hide My View", which opens the box. The story-tile menu gains "View Anonymously" (`StoryMenuAction::ViewAnonymously { locked }`). Both carry the lock icon rule.
- **Toasts.** Inside the story viewer they render over the story for 4 s, because status notes sit under the viewer overlay. Outside the viewer they become a plain status note.
- **Premium preview.** Unlock opens Premium at the "Upgraded stories" feature page (`show_premium_stories_preview`).
- **Demo.** `QUILL_DEMO_STORY=stealth|stealth-free|stealth-cooldown|stealth-on|stealth-tile-menu` drives the screenshot demos.

## Deviations

- The box follows the app theme. tdesktop uses the dark stories style inside the viewer.
- The button label is not upper-cased.
- The periods come from TDLib options rather than being hard-coded.
- A toast shown outside the story viewer loses its bold title, because it is a status note.
- Not done:
  - The reply-field placeholder countdown ("Stealth Mode active – m:ss", `media_stories_reply.cpp`). Setting a textarea placeholder needs a `Window`, and the tick does not have one. This is still a gap under `stories-stealth-mode`.
  - The profile stories-list context-menu entry point. Quill has no context menu there.
- `media_view_overlay_widget.cpp` `showPremiumDownloadPromo` is the save-story Premium promo, a separate feature, so it is not part of this change.

## Verification

- `gate.sh`: GATE OK.
- `cargo clippy --features demo-capture --all-targets --locked -p quill -- -D warnings` is clean. The hotspot, file-size, window-scope and lang checks pass.
- Unit tests: `src/story_stealth_tests.rs` (button and press states, time format, menu lock, option periods, toast text) and `stealth_periods_follow_the_tdlib_options`.
- Driver test: `driver_activate_story_stealth_mode_sends_and_dedupes` uses the recording sender to check that a second activation is deduped and that a refused one can be sent again.
- GPUI tests in `src/ui/story_stealth_ui_tests.rs` cover:
  - the menu entry and its lock;
  - Premium enable, then the drain closes the box and shows the toast, with the story paused;
  - free Unlock keeps the box, Premium covers it, and the box returns;
  - the cooldown label and toast;
  - the already-on toast;
  - the tile entry opening the stories once stealth is on.
- Screenshot demos captured in light and dark and checked by eye: Premium box, free box, cooldown, stealth-on toast, viewer menu, tile menu.
- Not verified: a real TDLib activation round trip and `updateStoryStealthMode`, the live Premium preview page, the countdown ticking in a running window, Linux and Windows.
