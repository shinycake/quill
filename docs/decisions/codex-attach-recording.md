# Real file picking, voice and video message recording on macOS

**Attach.** The paperclip menu's Photo/Video/File items attached
fixtures from `docs/screenshots` (the old live-testing shortcut). They now
open the system "Choose Files" dialog, as tdesktop does:
- "Photo or video": photos and videos join the album; any other file
  goes as a file, through the same path as drag-and-drop.
- "File": everything is sent without compression (`set_send_as_files`).
- `QUILL_ATTACH_PHOTO` / `QUILL_ATTACH_FILE` still skip the dialog for
  live testing, and the offline demo keeps its fixtures.
- "Video message" starts recording a round video. tdesktop records from
  the record button; the menu item just makes it easier to find.

**Why recording never worked on a Mac.** Both captures only knew Linux
devices: `ffmpeg -f pulse` for the microphone, `-f v4l2 /dev/video0` for
the camera. On macOS the voice capture produced an empty file and the
video capture failed with "no camera found". `media_tools::capture_input`
now picks the platform's devices: AVFoundation's default camera and
microphone on macOS, V4L2 + PulseAudio on Linux.

**Permission first** (tdesktop `Platform::GetPermissionStatus`). Before
capturing, Quill checks `AVCaptureDevice` authorization and asks
for any device not decided yet. The system prompt then names Quill. A
refusal leaves a note pointing at Privacy & Security instead of an
ffmpeg error. If ffmpeg still stops early, the note shows its reason
from the log, minus the device-format noise.

**Voice** (tdesktop `media_audio_capture`). ffmpeg encodes Opus OGG
(48 kHz mono) and also streams 8 kHz PCM back to Quill:
- One peak per 10 ms feeds the live bars.
- The sent waveform is tdesktop's `CollectWaveform`: 100 bars, scaled
  against 1.8× their mean, with a 2500 floor. Before, it was guessed from
  how fast the file grew.

**Video messages** (tdesktop `RoundVideoRecorder`). ffmpeg center-crops
the camera to a square and mirrors it, as tdesktop sends it. One branch
encodes H.264 + AAC (the round size from the HQ setting, max 60 s). The
other streams 360² BGRA frames for the live preview. AVFoundation reports
no frame rate (a 1 MHz time base), and the MP4 took that as a constant
1 000 000 fps: nothing was ever encoded and the stop signal hung. `fps=30`
in the graph fixes it, and the test now checks the 30/1 rate. Round
video messages now carry sound; they were silent (`-an`) before.

While recording, a 300 pt camera circle floats over the chat with a
shadow and a ring filling toward 60 s, as in tdesktop. Replaced preview
images are dropped from the sprite atlas on the next paint. At the limit
the clip is sent, as tdesktop does. Finishing runs off the main thread.

**Playback of what was just sent.** A sent recording keeps its local
path, and the temp folder failed the display sandbox ("voice file is
outside the account files"). Recordings now go to the media cache's
`captures` folder (0700). That folder is a display root and is swept with
the cache.

**Round video mask.** A video message's corners were masked in pure black
over the gray history. The mask now uses the history's color (the
wallpaper or the theme background).

Verified live in Saved Messages:
- the native picker opens from the attach menu;
- a voice message records, sends and plays (Opus, 48 kHz mono);
- a video message shows the live circle, then sends (H.264 280² 30 fps
  + AAC) and plays round, blended into the history.
