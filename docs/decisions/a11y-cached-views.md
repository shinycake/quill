# Cached views keep their part of the accessibility tree (2026-10-30)

## What was wrong

On Idan's Mac, Quill idled at 18% CPU with no chat open and 45% with a
playing clip on screen, where a fresh launch drew nothing while idle. A
slice is a cached child view (`src/ui/app_slice.rs`), and GPUI replays a
cached view's last frame unless something in it changed. While the
accessibility tree was built, Quill rendered every slice on every frame
instead, because GPUI builds the tree during prepaint and a replayed view
added no nodes: its part of the tree would have dropped out.

GPUI builds the tree from the moment any accessibility client first asks
macOS for it, and on macOS it never stops (`gpui-pre-macos` never calls
the deactivation callback). On the Mac:

- Window-level queries don't start it: `AXWindows` with position, size
  and title, `AXFocusedWindow`, `AXMainWindow`, app attributes, an
  `AXObserver` for move, resize and focus, and the menu bar.
  `AXFocusedUIElement` returns the window itself.
- Reading the window's `AXChildren` starts it, and so does any hit test.
- After one query, 330 s with no queries kept it on.
- A click with Tiles (a window manager) running started it: Tiles asks
  `AXUIElementCopyElementAtPosition` on clicks. Stopped in
  `accesskit_macos::subclass::hit_test` at a click, Tiles was the only
  process of 21 waiting on Quill.

So with an ordinary utility installed, the first click made every frame a
full render until Quill quit. A screen reader user paid the same.

## The change

GPUI replays a cached view's part of the tree with the rest of its frame
(`third_party/gpui-pre/QUILL-CHANGES.md`, "A cached view replays its part
of the accessibility tree"). The prepaint index records how many finished
nodes and focus claims the tree had; a reused range adds the same nodes
from the last frame's update, with their bounds, tags, focus handles and
action listeners, and makes the same focus claims. The agent driver's
hover and focus tags, which a reused frame also dropped, are replayed the
same way. Quill embeds its slices cached whether or not a tree is built.

Considered and not done:

- Answer hit tests without the tree unless VoiceOver or Switch Control
  runs. It would stop Tiles from starting the tree, but Hover Text and
  "speak item under the pointer" read what a hit test returns, every
  client that walks the tree would still start it, and a screen reader
  user would keep paying a full render per frame.
- Render uncached only for a screen reader. Every other client (Voice
  Control, Full Keyboard Access, automation tools) would read a tree with
  holes in it on every frame a slice replays.

## Tests

`src/ui/a11y_replay_tests.rs`: with the tree built, a frame in which only
the conversation slice renders leaves the tree, its focus, and the
focused and hovered tags as they were, and the replayed composer still
takes a value set by assistive technology. Both fail with the replay
taken out of `reuse_prepaint`, and the tags check fails without the
hover-tag replay.
