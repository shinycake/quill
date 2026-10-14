# Video avatars in the full video editor

The video avatar had its own panel inside the edit-profile dialog, with
five kit sliders for trim, cover, zoom and position. That was the last
place where the old slider editor survived. It is gone now.

The flow follows tdesktop's `PrepareProfileVideo`. "Video…" goes straight
to the file picker, and the picked clip opens the same full-window editor
that sending media uses, with a new `EditorTarget::Avatar`. That target
behaves like the sticker one:

- The crop is always a square.
- The range runs from 1 s to 9.6 s (`kProfileVideoMin/MaxDuration`).
- There are no quality dots and no GIF toggle.
- A clip that is too short, or more than ten times wider than tall (or
  the reverse), closes the editor with the reason.

"Done" keeps the editor up with a "Preparing the video…" note while
ffmpeg encodes. The encode turns and mirrors the clip, cuts the square
and writes 800 x 800 at 30 fps with no sound, at 1200 kbit/s. Then
`setProfilePhoto` goes out with the cover as the main frame. A failure
stays on the editor as its note.

The size label shows the avatar's own estimate. That is the bitrate over
the kept range, plus container overhead. Stickers show no size, because
their encoder steps the quality down until the file fits the set's byte
cap, whatever the clip.
