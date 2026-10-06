# Composer arrow keys

The composer's Left/Right arrows didn't move the caret, and typing "0"
could be swallowed. The media viewer's actions (Previous/Next on
left/right, zoom reset on 0, zoom in/out) are bound app-wide, and their
handlers did nothing while the viewer was closed. But a GPUI action
handler stops propagation unless it calls `cx.propagate()`, so the
keystroke never reached the focused text input. The handlers now
propagate when the viewer isn't open.

Verified live: in the composer, Left moves the caret and "0" types.
