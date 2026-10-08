# Media viewer parity with Telegram Desktop

## tdesktop behavior (read-only reference)

`Telegram/SourceFiles/media/view/media_view_overlay_widget.cpp`, `media_view.style`:

- `mediaviewWaitHide: 1100` (style:279): controls hide after 1100 ms without mouse movement.
- `mediaviewShowDuration: 200` (style:281): overlay fade-in. `mediaviewFadeDuration: 150` (style:282): controls fade.
- `Key_H` / `Key_V` (cpp:7431-7450): toggle `_flip` horizontal/vertical, photos only.
- Zoom is around the pointer; Delete, Copy, Save As, Forward, Show in Chat live in the toolbar and context menu.

## What changed

- **Delete**: a Trash toolbar button and context-menu item, only when the message may be deleted. The gate (`viewer_delete_gate`) mirrors the message menu: TDLib `messageProperties` (`can_be_deleted_only_for_self` / `can_be_deleted_for_all_users`) fetched on open/step, with the same local fallback before they arrive. It reuses `open_delete_dialog` (the "Also delete for ..." checkbox). When the message leaves the history, `MediaViewer::retain` moves to the next item (or the new last), or closes when empty.
- **Cmd+C** copies the photo as a PNG clipboard image (rotation/flips applied; refused in protected chats). **Cmd+S** runs the existing Save action.
- **H / V** flip; state is `ViewerOrientation { turns, flip_h, flip_v }` (flip then rotate). After a quarter turn the stored flip axes swap so the keys always flip what is on screen. The re-oriented pixels are cached under an orientation code.
- **Right-click menu** on the picture (`ContextMenuExt::context_menu`): Forward (the existing share/forward picker), Delete, Save As, Copy (photos), Show in Chat, Rotate, Flip Horizontally/Vertically. Forward/Save/Copy are hidden for protected chats.
- **Fade-in**: overlay opacity animates 0 to 1 over 200 ms on open. Closing stays instant, since a fade-out would have to keep the closed viewer's state alive.
- **Auto-hide**: toolbar, arrows and caption/transport fade out over 150 ms after 1100 ms without mouse movement and return on movement. They stay up while the pointer rests on one. The cursor is not hidden: GPUI's `CursorStyle` has no hidden/none variant. Screenshot-capture runs (`QUILL_DEMO_CAPTURE`) skip the fades and auto-hide so captures show the settled viewer.
- **Zoom at the pointer**: `ViewerZoom::zoom_at` keeps the point under the pointer fixed; `wheel_zoom_factor` scales with scroll distance (one 40 px notch = the 1.15 step, clamped to 3 notches per event). Buttons and `=`/`-` still zoom around the center; double-click resets.
- Keys are rebindable actions (`viewer-flip-h`, `viewer-flip-v`, `viewer-copy`, `viewer-save`) with no key context, like the existing viewer keys: they act only while the viewer is open and propagate otherwise. If the composer has focus while the viewer is open, its own Cmd+C wins over the viewer's.

## Verification

- `gate.sh`: GATE OK (core 1451, ui 45 tests). New unit tests cover zoom-at-point, wheel factor, auto-hide decisions, orientation/flip state and pixels, delete gate and deletion landing.
- Visual: `--screenshot-demo ready-media-viewer` shows the toolbar with the Trash button. Not exercised interactively: the right-click menu, real clipboard paste, live delete flow, the fade animations and the auto-hide timer in a running window (auto-hide was observed once in a capture, where controls were absent after the 1.5 s wait).
- `cargo clippy --features ui --bin quill -- -D warnings` still reports pre-existing lints across `src/ui` (including older code in `media_viewer.rs`) with this toolchain; none are from the new code.
