# Upload a custom notification sound

## What tdesktop does

`boxes/ringtones_box.cpp` puts an "Upload Sound" button under the saved sounds. It opens the
system file dialog filtered to "Audio files (*.mp3)" titled "Choose a tone". The file is read and
refused at once when it is bigger than the size limit ("Sorry, but your file is too big. The
maximum size for ringtones is {size}."). Otherwise it uploads; the server's
`RINGTONE_DURATION_TOO_LONG`, `RINGTONE_SIZE_TOO_BIG` and `RINGTONE_MIME_INVALID` answers show
inform boxes (duration text, size text, "Sorry, no way to use this file."). When the upload
finishes the new sound goes to the top of the list and is the chosen one. A right click on a saved
sound offers "Delete" (`removeSavedRingtone`). There is no drag and drop onto the box, and the
duration is not checked on the client.

## What changed

- `src/tone_upload.rs`: the file checks (MP3 extension, size against `notification_sound_size_max`),
  the mapping of server errors, tdesktop's copy.
- `addSavedNotificationSound` with `inputFileLocal` (`upload_notification_tone`), and
  `removeSavedNotificationSound` (`remove_notification_tone`) in `src/connect/settings/tone_upload.rs`.
- The reducer adds the answered sound to the front of the saved list and reports the outcome once
  (`ToneUpload`); a failure carries the server error class.
- The sound picker (chat, story, defaults, reactions) gets the "Upload Sound" button (native file
  dialog through `prompt_for_paths`), a row with a spinner for the file in flight, and a context menu
  "Delete" on saved sounds. The new sound is applied to the picker that started the upload.

## Deviations

- The dialog cannot filter by type in GPUI; a non-MP3 file is refused after the pick with tdesktop's
  invalid-file copy.
- Errors are toasts (Quill has no inform boxes), as elsewhere in Quill.
- The duration limit is enforced by the server only, as in tdesktop; the limit text comes from
  `notification_sound_duration_max`.
- No drag and drop: tdesktop has none.
- Quill selects the new sound immediately (its picker applies on click) where tdesktop selects it
  in the box until Save.

## Verified

Unit tests (checks, error mapping), driver tests with the recording sender (request shape, limits,
answer, server refusal, delete), GPUI tests (button, pending row, refused file, new sound selected),
and light and dark captures of the `ready-notify-upload-sound` demo. Not verified: a real upload
against Telegram, the real system file dialog.
