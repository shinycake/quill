# UX copy and search chips

Findings 1, 2, 4 and 10 of the UX sweep.

## What tdesktop does

- Forwarding shows a toast, `lng_share_message(s)_to_chat`: "Message forwarded to **{chat}**." No ids.
- An empty group or channel paints nothing (`EmptyPainter` only draws the "You created a group" panel when the user created the chat). There is no developer text.
- Search tabs and chip rows use language-pack keys (`lng_recent_chats`, `lng_all_photos`, `lng_search_filter_*`, `lng_search_tab_*`, `lng_media_type_*`). English strings use the real ellipsis character.

## What changed

- Forward banner: one line, text from the two tdesktop keys (bold markers dropped), failure text is `qll_forward_failed`. The ids line and `qll_misc_ids` are gone.
- Empty chat: the pane is blank, as in tdesktop. Not done: the "You created a group" panel for creators (Quill has no creator flag on `ChatSummary` for basic groups).
- Search: every tab, chip, scope, date and hint label goes through `tr()` (new `lang/en/search.strings`, tdesktop keys where they exist, `qll_search_*` otherwise). Row labels ("Search", "Chats", "Date") now have the height of an xsmall chip, so they share the chips' first line. Result rows get the chat row's 2 px transparent leading edge so avatars line up. The tab strip fades its clipped edges (left once scrolled, right while more follows; assumed before the first layout, then re-rendered once measured).
- Copy: 32 three-dot ellipses in `lang/en` are now "…". Hard-coded text labels (`OK`, `Play`, `How?`, `COLLECT`) in `src/ui` go through `tr()`. Symbol-only labels stay.
- `scripts/lang-baseline.txt` is unchanged (the removed strings were not counted by the checker).

## Verified

- Tests: `ForwardResult::success_label` (state and driver tests, replay test), a Portuguese-pack test for tab, chip, date and hint labels, a test that no English `.strings` file has `...`. Gate OK.
- Before/after demo captures (ready-forward, ready-search, ready-group-info-edit, light) in `/Volumes/Idan1TB/quill-work/scratch/quality-sweep/fix-ux-copy/`.

## Not verified

- Dark theme captures were not taken; the fade uses the theme background token.
- Other languages beyond the unit test (no real pack loaded in a window).
