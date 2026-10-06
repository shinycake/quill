# Video player panel and Space to play/pause

Owner: "in the video player some UI is obscured, and space bar should
play/pause". The volume slider's thumb overhung its 80 px box and sat on
the "100%" label; the transport was a row of text buttons ("▶ Play",
"Mute") next to the photo zoom controls.

Telegram Desktop's player (media_view playback controls): one rounded
panel, with the seek bar across it and elapsed / remaining time at its
ends; below it play/pause, a volume icon with its slider, then speed and
picture-in-picture. Space toggles playback.

Decision:
- Videos get that panel, with icon buttons (Play/Pause, Volume2/VolumeX,
  PictureInPicture2). Sliders have horizontal padding so the thumbs stay
  inside. The zoom controls stay for photos only.
- Space plays/pauses through the keystroke interceptor, ahead of the
  composer, so it never types a space into a draft while the viewer is
  open.

Verified live: the panel lays out cleanly; Space pauses (time holds) and
resumes.
