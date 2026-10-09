# Perf / memory / correctness bundle (Q3-Q7, Q10)

Small, evidence-based fixes from the quality track. These are internal
quality fixes; the tdesktop sources were not re-read for this bundle, so no
tdesktop behavior is claimed beyond what the earlier decision docs cite.

## Q3 Path verdict memo hard cap (`src/local_path.rs`)

`sandboxed_display_path` memoized verdicts for 1 s and "capped" the map at
8192 by dropping expired entries only, so a burst of fresh distinct paths
grew it without bound. `make_room` now drops expired entries first and then
evicts oldest-first until one slot is free, so the map never exceeds
`VERDICT_CAP` (8192). Test: `verdict_cache_hard_cap_evicts_oldest_first`
(8192 fresh entries, oldest evicted, size stays at the cap).

## Q4 Spell dictionaries warm off-thread (`src/spell_dict.rs`)

Finding: the render path (`spellcheck_underlines`) only paints cached
`spell_misspellings`, computed on the checker's background thread, so render
itself never parsed a dictionary. The parse was still lazy in
`OnceLock::get_or_init`, so the first background check, and the first
suggestions request (right-click menu, UI thread), waited for it.
`hunspell_engine` now spawns `quill-spell-warm`, which calls the new
`SpellBackend::warm` (parses every selected dictionary). Until it finishes a
check simply waits on the same `OnceLock`; no underlines are painted before
results exist. Test: `warm_parses_every_dictionary_once`.

Measurement (release, M1 Pro, `large_dictionary_parse_time`, ignored timing
probe, synthetic 400k-word dictionary with one suffix rule): 63 ms to parse.
Real large dictionaries will be a few times that; the work now happens at
startup instead of at the first suggestion. Not measured against a real
`.dic` (none installed on the dev machine).

## Q5 History photos at display size (already shipped)

`codex-sized-history-media.md` is the follow-up the item points to: history
photos, posters, album tiles, link previews and stickers already decode at
`scale * displayed edge` through `image_budget::sized_media`, with the byte
budget and idle trim, and the viewer keeps full size. Verified in the code
(`message_media.rs`, `history.rs`, `bubble_header.rs`, `service_row.rs`,
`message_text.rs` all use `sized_media`). No change. Existing numbers from
that doc (200 x 2560x1920 photos, peak footprint 340 -> 292 MB, idle
217 -> 135 MB) still apply.

## Q6 Picker emoji animate only on screen (`media_panel.rs`, `sticker_playback.rs`)

The panel list is virtualized and the frame clock stops when nothing asked
for a tick, so a closed panel already costs nothing. The gap was list
overdraw (400 px): the rows built above and below the viewport also
requested ticks. `custom_emoji_cell` now takes `on_screen`
(`row_on_screen(item_is_above_viewport, item_is_below_viewport)`; unknown
counts as visible); an overdraw row shows the clip's first frame via
`custom_emoji_image_parked` and does not call `request_animation_tick`.
Clips still decode ahead, so scrolling in is seamless. Test:
`overdraw_rows_do_not_count_as_on_screen`. Live idle-wake behavior was not
measured (needs a premium account with custom emoji sets).

## Q7 Server name colors (`src/telegram/name_accent.rs`)

`updateAccentColors` is parsed (`accentColor`: id, built-in id, light and
dark RGB lists) into `Session::name_accent_colors` and a process-wide table
(the palette is account independent). `chat_theme::peer_name_color` uses the
server colors for ids 7+ (first color of the theme list, dark falls back to
light), then the entry's built-in id, then the old modulo fold. Ids 0-6 keep
the tuned built-in values. Tests: parser, resolution order, and the reducer
(`update_accent_colors_stores_name_palette`).

## Q10 Group call ssrc routing cleared on call end (`engine/types.rs`, `ntgcalls.rs`)

`group_video_ssrc_to_user` was pruned on endpoint removal and at the end of
`leave_group_call`, but the two `?`-style early returns for a failed
`ntg_stop_presentation` / `ntg_stop` skipped the cleanup, stranding the
chat's routing. `CallbackShared::forget_group` now runs right after the call
is removed, before any native stop. Test:
`forget_group_clears_only_that_chats_routing`.

## Verified

Unit tests above; `gate.sh` (fmt, clippy, tests). Nothing run against a live
account.
