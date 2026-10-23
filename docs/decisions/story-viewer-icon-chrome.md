# Story viewer icon chrome

## What tdesktop does
`media/stories` draws the viewer as an icon overlay: the header holds the
userpic with its privacy badge, name, age, then round icon buttons (sound for
video, pause) and a three-dots menu; Save, Copy Link, Edit, Privacy, Stealth,
Report, Hide, Delete and the rest live in that menu, gated by the story's
rights. Under someone else's story sit the reply field with the heart and
share buttons; under your own, the recent-views row (eye and count) that
opens the viewers list, with share.

## What changed
- `story_viewer/icon_chrome.rs`: header icon buttons (sound, pause, menu,
  close), the menu (kit `DropdownMenu`, entries gated exactly as the old text
  buttons were), and the bottom bar (reply pill, heart, share; or views,
  reaction and forward counts with share). Right-click on the heart opens the
  reaction picker, which tdesktop shows on hover; the menu also has React...
- The wrapping row of text buttons and the counters line are gone; every
  action kept its handler.
- The viewers list loads the next page when scrolled within 80 px of its end
  (and each tick, so a short page chains), with a spinner row instead of
  Load more. Live only.
- `QUILL_DEMO_STORY=own|own-viewers|own-viewers-loading` for `ready-story-viewer`.

## Deviations
- The reply field is a pill that opens the existing reply row rather than an
  always-live field (focus drives the story pause here). The placeholder is
  "Reply to story" (no new language key).
- Recent views show the eye count, not the viewers' avatars.
- Prev / Next text buttons stay.

## Verified
`/verify` captures of someone else's and your own story, the menu, and the
viewers list with the loading row, light and dark; GPUI test for the menu gates.
