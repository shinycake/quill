# Viewer extras

## What Telegram Desktop does

- The speed button in the playback controls and the player bar opens a menu
  with a slider from 0.5x to 2.5x and six named presets: Slow 0.5, Normal 1,
  Medium 1.2, Fast 1.5, Very fast 1.7, Super fast 2
  (`media/player/media_player_dropdown.cpp`, `kSpeedMin`/`kSpeedMax` in
  `media/media_common.h`).
- Saving from the viewer shows a toast, "Image was saved to your Downloads
  folder" (or "Video file was ..."), where Downloads is a link that reveals
  the file (`showSaveMsgToast` in `media_view_overlay_widget.cpp`).
- The sender name in the header opens that peer's profile (`Over::Name`).
- The context menu says "View all photos" or "View all files" and opens the
  chat's shared media.
- The OS transport controls drive whatever is playing
  (`media/system_media_controls_manager.cpp`).

## What changed

- `src/viewer_extras.rs` holds the pure logic: speed range, rounding and
  labels, the preset list, the saved-toast wording, the mapping from OS
  commands to viewer actions, the Now Playing info for a viewer video, the
  sender-name target, and the "View all" label. It has unit tests.
- Speed dial: the speed button in the viewer and the player bar is now a
  popover with a slider (0.5x to 2.5x in tenths) above the six presets. The
  rows' speed links still cycle through the old set.
- Saved toast: saving from the viewer shows a toast under the top bar with a
  "Downloads" button that reveals the file. It names the real folder when the
  file did not go to Downloads. It hides after four seconds.
- Sender header: the name opens the sender's profile (user, group or channel;
  the chat itself when the message has no sender).
- View all: the entry reads "View all photos" or "View all videos".
- OS media keys: while a viewer video is open and no audio or voice message is
  active, the viewer's tick takes commands from the existing macOS, MPRIS and
  SMTC backends and publishes the video to Now Playing (title "Video from
  <sender>", subtitle the chat). Play, pause, toggle, stop and seek are
  handled. Next and previous are not offered for a single clip.
- Caption: the viewer already renders captions through the message rich-text
  path, so bold, links and custom emoji show. The new demo exercises it.

## Not done

- Video rotate, Share at time and the quality picker. Share at time needs
  `inputMessageForwarded` with a new start timestamp, which the forward picker
  does not carry yet. Rotating video needs a per-frame transform on both the
  native macOS surface and decoded frames. Copy Frame was already there.
- "Disappears in" countdown: the viewer does not open self-destructing media
  at all, so there is nothing to count down yet.

## How it was verified

- `gate.sh`: GATE OK (core 2611, ui 210).
- `ready-viewer-extras` demo capture: a playing video with a bold and linked
  caption, the sender header, the saved toast and the open speed dial.
  `QUILL_DEMO_NO_DIAL=1` renders it without the dial.
- Not verified on Linux and Windows hardware: media keys there go through the
  existing backends, which this change only feeds.
