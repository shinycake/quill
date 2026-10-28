# Notification click: the window stops repainting on macOS

Reported 2026-10-28 on v2026.1027.3908: clicking a Quill notification brings the app forward, but the window
is frozen. No hang or crash report exists for it, and the run ended with a clean quit, so the main thread was
still handling input. The unified log agrees: after the chat was opened, Quill kept working (it posted a new
notification for that same chat 17 s later, which it only does when it thinks the window is inactive or the
chat is not shown) while nothing new reached the screen.

## Cause

The idle display link (`docs/decisions/codex-idle-cpu.md`) stops a window's `CVDisplayLink` after 250 ms with
nothing to draw, and GPUI's frame waker asks for one step to start it again. The waker only did that when the
link had stopped *for idling* (`FrameIdle::stopped`). Every other stop cleared that flag and left the restart
to `start_display_link`, which gives up when the window is not visible yet or AppKit reports no screen for it.
A window coming back from hidden or minimized (what a notification click does) goes through several of those
stops and restarts. If the last restart gives up and no later occlusion change retries it, the window is
visible with no display link and a waker that does nothing. From then on GPUI marks the window dirty on every
change, nobody draws it, and the window looks frozen until something outside Quill changes its occlusion or
key state.

## Fix

`FrameIdle` now records whether the link runs (`running`) and whether AppKit last reported the window visible
(`visible`). The waker asks for a step whenever a visible window's link is not running, whatever stopped it.
That step draws and calls `start_display_link`, so a failed restart is retried on the next change. Hidden
windows still never draw (their `visible` is false), and a running link is still never disturbed, so idle CPU
is unchanged.

## Verified

A debug build with temporary, uncommitted hooks: hide the app, replay the notification click's
response handling for a chat, then make the next display link start fail once as if AppKit reported no screen.

- Before: the chat opened in state, but the window never drew again (no render in the next 5 s, including after
  a forced `cx.notify()`).
- After: the next change drew at once and restarted the link; renders continued normally.

The plain hidden and minimized replays (no forced failure) drew normally before and after.
