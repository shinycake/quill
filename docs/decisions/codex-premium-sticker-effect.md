# Premium sticker effect

## What tdesktop does

`HistoryView::Sticker::hasPremiumEffect` is true when the sticker document is a premium sticker (`isPremiumSticker`: a regular sticker whose full type carries a premium animation). That is not "the viewer lacks Premium".

When the sticker view is created it loads that animation (`loadVideoThumbnail`). Once the sticker player is ready, `checkPremiumEffectStart` calls `elementStartPremium` / `EmojiInteractions::playPremiumEffect`, which plays the effect once around the sticker. The effect is `kPremiumMultiplier` (1.49) times the sticker, shifted toward the outer edge by `kPremiumShift` (21/240 of the sticker width), and mirrored for a message that is not right-aligned. A later visit to the chat builds the list again, so the automatic play happens again.

A click is `premiumStickerClicked`: it clears the played flag and repaints, which starts the effect again. If the effect is already in `_plays`, `playPremiumEffect` returns false and `elementStartPremium` shows `showPremiumStickerTooltip` (`lng_sticker_premium_text`: "This set contains Premium stickers like this one."). The click handler is set only when the sticker belongs to a set. While `PowerSaving::kStickersChat` is on, `elementStartPremium` is not called, so the effect is not played and the hint is not shown. Playback is not gated on the viewer having Premium. Sending a premium sticker without Premium is a separate path.

## What changed

Claude parsed `sticker.full_type.premium_animation` into `StickerItem` / `StickerContent` and listed that file for download, extended `emoji_effect` with a premium placement path (1.49×, outer-edge shift, frames flipped for a message on the left), and started the effect from the history overlay once `layered_clip` had a decoded TGS. Click replay, the hint, and the once-per-chat-visit set were already in `premium_sticker_effect`.

omp, after the session limit:

- Moved the history-row click and position tracker into `premium_sticker_effect::wrap`, so `session_history_row.rs` stays under 1000 counted lines. A premium sticker still replays; a plain sticker still opens its set.
- Reverted the one-off `premium_animation` injection in the `HistoryAnim::Stickers` demo fixture. It is `null` again.
- The overlay is built before paint, so the first paint of a premium sticker asks for one more frame. Autoplay then calls `layered_clip`, which is the existing animated-sticker path: `ensure_sticker_playback` downloads the TGS (`download_file`, priority 1) when it is not local, unless data saver is on, and returns a clip only after decode. The effect is marked played only then. The open-chat thumbnail pass still downloads the sticker picture, not this extra file; it is not a display file.
- While stickers in chats are paused, the file is still requested, but the effect is not started and a click does neither replay nor hint.

## How verified

Unit tests: premium-animation parse and file list, autoplay once per chat visit, click replay versus hint, effect rect on the left and the right. `gate.sh` printed `GATE OK core=3436 0 ui=291 0`. `scripts/check-file-size.sh` printed file sizes OK (`session_history_row.rs` is 984 counted lines). `scripts/check-hotspots.sh origin/main` printed hotspots OK.

A temporary `ready-chats` capture with `QUILL_DEMO_HISTORY_ANIM=stickers` and a local `premium_animation` (not committed) showed the two history stickers. The only local TGS is a 273-byte circle, so the frame does not show a separable premium effect. The fixture was put back to `premium_animation: null`.

## What was not checked live

No live Telegram session: a real `premium_animation` download, the effect around a sticker in an open chat, a click replay, or the hint. 

## Sticker flip (codex/premium-sticker-flip)

tdesktop's `Sticker::mirrorHorizontal()` is true when the sticker has a Premium effect and its message is not right-aligned (`hasRightLayout`: outgoing and not wide chat mode). The paint path then flips the sticker bitmap, so the sticker faces the same way as the mirrored effect.

Quill now does the same in `src/ui/sticker_mirror.rs`: `mirrors(has_premium_effect, right_aligned)` is the condition (Quill's right-aligned is `is_outgoing`, as the effect placement already uses; Quill has no wide chat mode). `render_history_row_body` passes the decoded sticker visual through `mirrored` for such messages. The decoded frames are flipped once with `emoji_effect::flip_frames` (the same row flip the effect uses) and cached by the identity of the decoded frame set (16 sets, least recently used out, evicted frames retired to the atlas sweeper), so a re-decode flips again and nothing is done per frame afterwards. The non-layered single-image path is cached per image the same way.

Verified: unit tests for the condition, the flip, and reuse of the flipped image; `gate.sh` GATE OK, hotspots and file size checks OK. Not verified: a visual capture (the only local TGS is a trivial circle) and a live Premium sticker.
