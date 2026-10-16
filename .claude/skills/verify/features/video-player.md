# Video player (media viewer)

The full-window viewer that opens when a user clicks a video in a chat: the
frame, the bottom controller (play, scrubber, time, volume, speed, PiP, full
screen), the speed popover and picture in picture.

## Sub-features

- Controller: play/pause, scrubber with hover thumb and time bubble, speed label.
- Speed popover (0.5x to 2x).
- Picture in picture window.
- Inline players in history (video tile autoplay, round video notes) live in
  [chat-history.md](chat-history.md).

## How to get to it (user POV)

Open a chat with a video, click the video. The controller shows while the
pointer moves over the viewer and hides after a pause.

## Driving it with --screenshot-demo

| Kind | State on screen |
| --- | --- |
| `ready-video-controls` | viewer open, 1.5x, pointer over the scrubber at 42% |
| `ready-video-speed` | viewer open with the speed popover |
| `ready-video-pip` | in-viewer playback for the PiP path; add `QUILL_DEMO_CAPTURE_WINDOWS=1` |

Proven click path (1200x800 window): open the speed popover from the
controller with
`QUILL_DEMO_CLICK="m:734,760;p:200;734,760;p:500;m:734,760"` on
`ready-video-controls`.

Setup lives in src/ui/demos/payments.rs (`demo_video_playback`) over
src/ui/demos/video_fixtures.rs. End state that proves it: the frame is drawn
(not a black or empty rect), the controller row is complete with the speed
label reading the set speed, and the hover bubble sits over the scrubber.

## Gotchas

- A capture cannot enter OS full screen. Prove the full-screen button's state
  and layout with the window sized large (`QUILL_DEMO_WINDOW_SIZE=1600x1000`);
  real full-screen transitions need a manual check on each OS.
- The controller auto-hides. Keep a `m:x,y` hover step last in
  `QUILL_DEMO_CLICK` if you need it visible after clicks.
- Video decode differs per platform; a macOS capture does not prove the
  Linux or Windows decoder.
