# Idle CPU: stop redrawing what nobody sees

Quill sat at 35–40% CPU with no chat open. Two causes, and one cost
that made each redraw expensive:

1. **Inline players outlived their chat.** `InlineVideos` swept its
   players only when the history rendered (`begin_render`). After you
   left a chat with an autoplaying clip for a screen without history (no
   chat, Contacts, Calls…), the AVPlayer kept decoding and kept the
   frame clock at 30 fps. Now `frame_start` runs at the top of every app
   frame and stops every player when the previous frame drew no history.
2. **Animated emoji played while the window was in the background.**
   One animated custom emoji in the chat list asked for 30 fps, and each
   tick rebuilt the whole app. tdesktop pauses GIFs, stickers and emoji
   whenever its window isn't active (`SessionController::isGifPausedAtLeastFor`
   → `!widget()->isActive()`). Quill now does the same:
   `window_active` is captured each frame, and sticker/emoji playback
   only asks for ticks while it's true.
3. **Per-row file-system work.** `media_display_roots` ran once per chat
   row per frame and rebuilt the roots each time, including
   `ensure_private_dir` and the cache base. It's now computed once per
   frame (`media_roots_frame`, cleared at frame start).

`QUILL_TRACE_TICKS=1` logs which call site keeps the frame clock running
(once a second per site), so the next idle-redraw hunt takes minutes.

Measured on this Mac with the release build, no chat open:

| | CPU |
| --- | --- |
| Before | 35–40% |
| After, window active (one emoji still animating) | 10–14% |
| After, window inactive | ~11%, with zero frame-clock ticks |

The remainder is outside the frame clock (TDLib threads, 1 s timers). The
bigger structural fix, small self-redrawing views for animated content
plus cached subtrees so a 20 px emoji doesn't rebuild the app, is a
separate change.

This change also adds project subagent definitions (`.claude/agents/`:
quill-builder on Sonnet, quill-scout on Haiku, quill-architect on Opus at
high effort) for orchestrated work.
