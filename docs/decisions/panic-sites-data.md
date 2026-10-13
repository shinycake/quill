# Panic sites in data paths and RefCell guards

Steps 3 and 4 of the no-crash plan (`docs/decisions/no-crash-audit.md` §5),
covering the sites listed in §3.2, §3.4 and §6.2.

## What tdesktop does

Nothing to match: this is internal robustness work with no visible
behaviour. tdesktop's own PNG and media paths check lengths before reading
(Qt's image loaders reject short chunks), which is the behaviour Quill now
has too.

## What changed

Indexing, slicing and arithmetic (§3.4, §6.2):

- **P0 `app_theme/png.rs`**: the `IHDR` body is destructured with a slice
  pattern, so a chunk shorter than ten bytes returns `None`. The chunk walker
  uses `get` and checked adds, so a chunk length near `u32::MAX` cannot wrap.
- **P1 `composer_doc::block_range`**: both selection edges snap to char
  boundaries (`floor_char_boundary` / `ceil_char_boundary`) before slicing.
- **P1 `ui/vanish.rs`**: `width() - 1` / `height() - 1` became
  `saturating_sub(1)` and the sample goes through `get_pixel_checked`, so a
  zero-sized picture falls back to the fill colour.
- **P2 composer_doc serializer (9 sites)**: span and emoji ranges that are off
  a char boundary are dropped; every `self.inline[ix]` and `text[a..b]` is a
  `get`.
- **P2 `code_language::with_language`**: a fence found in an older draft
  returns the new `LanguageError::Stale` ("The code block changed. Try
  again.") instead of slicing out of range.
- **P2 `ui/emoji_effect.rs`**: the index held across `effect_clip` is looked up
  with `get` / `get_mut`.
- **P2 `video_decode::rotate_bgra`**: returns an empty 0x0 picture when the
  buffer is shorter than `stride * (height - 1) + width * 4`.
- **P2 stat charts**: `height_limits` clamps `start` to `end`; the pie total in
  `ui/stat_chart/elements.rs` uses `get(first..=last)`.
- **P2 call rating**: `RatingDetail::problems` is `[bool; CALL_PROBLEMS.len()]`,
  and every read and toggle uses `get`.

RefCell guards (§3.2), behaviour unchanged:

- `ui/story_more.rs`: a `with_story_video` helper borrows the player for one
  call with `try_borrow_mut`; pause/resume and the frame fetch use it, so no
  guard outlives a native call and the frame-clock request runs unborrowed.
- `ui/video_editor_tools.rs`: the player guard is dropped before
  `request_animation_tick`; a busy cell skips one frame and shows the still.
- `ui/premium_ui/promo.rs`: the preview frame uses `try_borrow_mut`.
- `ui/inline_video.rs`: the frame fetch uses `try_borrow_mut`, and the tick
  decision is copied out before `request_media_tick` /
  `request_animation_tick`.
- `ui/conversation/history_message_list.rs`: the canvas takes the probe rows
  out of the cell (`try_borrow_mut` + `mem::take`), works on them unborrowed,
  and hands the emptied buffer back to keep its allocation.

## Skipped

None of these sites sit in files owned by in-flight parity work (composer send
menu, TON wallet rendering, channel/bot revenue screens), so nothing was
deferred.

## How verified

- New unit tests: truncated, empty and huge `IHDR` chunks; `block_range` with
  both edges inside multi-byte characters and past the end; markup with spans
  and emoji off char boundaries; a stale code fence (out of range and inside a
  character); `height_limits` with a start past the end; `rotate_bgra` with a
  short buffer, zero height and a tight last row; vanish grains from 0x0, 0x5
  and 5x0 pictures.
- `quill-tools/gate.sh` (fmt, clippy, tests, size and hotspot checks).

## Unverified

The RefCell changes only alter what happens when a cell is already borrowed,
which no current path does; there is no test that forces re-entry.
