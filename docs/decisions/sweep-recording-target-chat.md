# Recordings go to the chat they were sent from

## What tdesktop does

`VoiceRecordBar` hands the finished recording to the history it was started
in (`_send` is bound to that controller's `Api::SendAction`), so opening
another chat while the file is written never redirects it.

## What changed

- `SendTarget` (`src/ui/recording_send.rs`) fixes the chat, the reply target
  and the caption when Send is tapped. Slow mode is checked against that
  chat. The background job carries it, and the send goes through
  `send_recorded_video_note_to` / `send_voice_note_to` (the old methods call
  them with the open chat). The composer, reply and draft are only cleared
  when the target chat is still the one on screen.
- Voice: `VoiceCapture::close_microphone` releases the microphone on the UI
  thread (the cpal stream may not leave it); joining the encoder
  (`VoiceFinish::complete`) runs on `background_spawn`, for the composer and
  the story reply.
- The round video start (`VideoNoteCapture::start`: encoder probe, ffmpeg
  launch) runs on `background_spawn`. `RecordingUi::starting` stops a second
  start meanwhile; the capture is held by the window that asked, or dropped.
- Cleanup: `VideoNoteCapture::finish` removes the clip on every error, a
  dropped capture removes its clip, a failed send deletes the draft, and the
  story reply deletes the draft when its update target is gone.
- New toast: `qll_notes_voice_note_not_sent_window_closed`.

## How verified

Unit tests: `SendTarget` keeps its chat, and a driver test sends a video
note to chat 11 while chat 12 is open. Windows and Linux compile in CI.
Not exercised: real camera and microphone hardware.
