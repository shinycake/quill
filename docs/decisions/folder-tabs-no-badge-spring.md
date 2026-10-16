# Folder tabs: a badge change no longer springs the indicator

## What tdesktop does

`DiscreteSlider::setSelectedSection` (lib_ui `ui/widgets/discrete_sliders.cpp`)
animates the active-section bar from the old section to the new one. The unread
counter on a folder tab (`ChatsFiltersTabs::setUnreadCount`, `window/window_filters_menu` /
`dialogs/ui/chats_filters_tabs`) only recomputes the section widths and
repaints: the bar jumps to the new width with no animation.

## Problem

The folder strip uses the kit `TabBar` (underline variant). In gpui-component
0.7.3 `render_indicator` / `update_anim_params` retarget a 280 ms `spring_move`
spring whenever the selected tab's left or width differs from the last target,
not only when the selected index changes. A badge appearing or disappearing
changes the tab width, so the indicator sprang, and every spring frame
re-rendered the sidebar slice. With `QUILL_DEMO_UPDATE_STREAM=10` (unread totals
change in each eight-update cycle) the sidebar redrew at the display rate in
roughly 300 ms bursts.

## Change

`third_party/gpui-component` (pristine 0.7.3 in the previous commit, patch in
this one, recorded in `QUILL-CHANGES.md`, wired through `[patch.crates-io]`).
`TabBar`'s animation state gains a jump counter that is part of the spring key.
A selection change keeps the key and springs as before. A bounds-only change
bumps the counter; a fresh spring adopts its first value immediately, so the
indicator jumps to the new bounds. The first measurement does not count as a
jump. The tab epoch (text color fade) is untouched, and the `TabBar` is not
re-keyed, so scroll position and tab transitions are kept.

The kit has no option to disable the spring for bounds changes, hence the patch.
`THIRD_PARTY_LICENSES.md` is unchanged: same crate, same version, same license.

## Verification

- `src/ui/folder_tabs_tests.rs` (GPUI UI tests): the indicator width equals the
  resized tab's width a few frames after a badge-sized suffix grows (fails
  without the patch: 45.5 px against 34 px), and the indicator still travels
  over many frames when the selection moves.
- Release before/after on the stream10 demo: see below.

Measured on this Mac (M1 Pro, release, `--screenshot-demo ready-chats`,
`QUILL_ASSUME_ACTIVE=1 QUILL_TRACE_TICKS=1 QUILL_DEMO_UPDATE_STREAM=10`, 20 s
settle, mean of 20 one-second `top` samples, builds alternated twice). The load
average was 25-63 from other builds, so absolute CPU is noisy:

| | sidebar renders/s | animation layer renders/s | CPU |
| --- | --- | --- | --- |
| Before | 40 in the spring bursts (3 between them) | up to 84 | 4.95%, 4.84% |
| After | 3 steady | 6-22 (12 fps typing tick) | 1.63%, 1.64% |

The remaining 3 sidebar renders a second are the stream's own updates.
