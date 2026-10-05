# codex/voice-rows — voice and audio rows without boxes

- Voice notes and audio files drop the bordered card and the text "Play" button. Each row is a
  44px accent action disc (play / pause / progress ring while downloading) followed by the
  content. Stacked on codex/document-rows, so it shares that row's geometry.
- Voice: the waveform doubles as the progress indicator (bars before the playhead are solid,
  the rest faded), with a duration line and an accent dot for unheard incoming notes. The slider
  and speed/mute controls appear only on the active row.
- Audio: album art, when present, is the play button (glyph on a scrim). Otherwise it uses the
  accent disc. Title plus "performer · duration", truncated.
- Inline controls (Transcribe, Retry, speed, Mute) are compact links via `inline_link` in
  `bubble_accent` (theme primary on incoming bubbles, white on outgoing), so they stay legible
  on the accent bubble. `waveform_row`, `seek_bar_element`, `transcription_row` and
  `row_playback_controls` take that color. `action_disc` is the shared disc helper.
