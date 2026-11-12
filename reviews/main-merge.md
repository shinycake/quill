# Merge of origin/main (91a4b02fd) into the follow-up

Merge commit a7ac0c33c, parents 0a22b2d88 (the follow-up with 47 of 47 in) and 91a4b02fd. A merge, no force. Main had gained the send box dialog (#1450) and the viewer's screen cover and draggable controller (#1451) since the sweep branched.

## Conflicts and how each went

| File | Resolution |
| --- | --- |
| `scripts/ui-statics.txt` | Main's `screen_cover.rs X11_LAST` line added. The two statics on our side of the block (`dialogs/contacts.rs NEXT`, `viewer_parts.rs TOKENS`) no longer exist in the sweep tree, so their lines stay out. |
| `src/ui/conversation/conversation_part1.rs` | Main's removal of the in-chat send card kept (the box is a dialog now); the sweep's `forward_result` shape kept. |
| `src/ui/drop_zones.rs` | The sweep's `FileDrag` stage kept; main's rule added to the `Probed` arm: no zones while the send box is shown. |
| `src/ui/media_viewer/actions.rs` | Sweep's pointer stage kept; main's rule added: a held controller counts as the pointer on the controls. |
| `src/ui/media_viewer/mod.rs` | Both module lists. |
| `src/ui/media_viewer/navigation.rs` | Sweep's `viewer.closed()` kept. `closed()` now also ends a held controller drag (below). |
| `src/ui/send_box_options.rs` | Sweep's per-path reads kept (`TrayReads::photo_sizes`). Main's new rule (a picture that goes as a file, but is not file-only, still counts for the HD offer) is one helper, `sized_picture`, used by the reader, by the read that starts, and by `tray_has_large_photo`. |
| `src/ui/send_box_state.rs` | Main's `RefCell` caches dropped; the sweep's `forget` on each `Reads` kept. |
| `src/ui/send_card/header.rs` | Deleted with main. The sweep's one change there (`media_grouping.is_remembered()`) moved to main's `send_card/options.rs`. |
| `src/ui/send_card/mod.rs` | Main's modules plus the sweep's `file_size`. |
| `src/ui/send_card/tiles.rs` | Main's tile actions kept; the file row's size stays the sweep's off-thread read (`reads.file_sizes`), not `std::fs::metadata` in render. |
| `src/ui/send_card/video_preview.rs` | Main's side. |

Auto-merged but edited: `src/ui/viewer_state.rs`. `ViewerUi::new_item` and `closed` destructure every field, so main's new `controller` field had to be placed: kept across items, and on close `controller.release()` ends a drag that was still held while the resting spot stays for the next open.

## Guards on main's new code

Run without `--prune` on the merged tree: implicit-state 7/15, request-match 143/6, owned-timers 45/9, refcell-borrows 85/0, stage-enums 178/47. Same counts as before the merge. No new hit, no baseline line added or removed. Under `scripts/` the merge changes two lists only, both from main: the `SendBox` line in `session-dialogs.txt` and `X11_LAST` in `ui-statics.txt`.

## Review of main's new state (same eyes as the sweep)

Read whole: `send_card/{dialog,options,tiles,file_size}.rs`, `media_viewer/{screen_cover,controller_drag}.rs`, `src/media_viewer/placement.rs`, `conversation/composer_field_view.rs`. Read as diffs: `send_card/{layout,demo,tests}.rs`, `media_viewer/{caption,chrome,controller,layer,geometry}.rs`, `video_player/controls/mod.rs`, `window_actions.rs`, `window_control.rs`, `composer_sends.rs`, `media_panel/{mod,mode}.rs`, `send_button_ui.rs`, `shell.rs`, `agent_driver_surface.rs`.

- The send box has no open flag: it shows while the chat's tray holds files (`send_box_shown`). Nothing to keep in step.
- `Choices`, `TileAction`, `TileControls`, `Quality`: read per render from the tray; enums where there is a choice.
- `ControllerPlace { spot: Home | Moved, drag: Idle | Held }`: `Held` ends on mouse-up, on a move with no button down, and (added in the merge) when the viewer closes.
- `DockedCaption::{Folded, Open}`: an enum, reset with the item.
- `VideoController`'s new `grabbed`/`bare` bools and `press`/`snap_back` options are builder props of a `RenderOnce` element, not kept state.
- No new `unwrap`/`expect`/indexing outside tests; `x11_frame_and_work_area` uses checked arithmetic and `get(range)`.

Fixed in the merge:

1. Held controller drag outlived the viewer (above).
2. Three of main's send box tests (`pictures_sent_separately_stand_side_by_side`, `a_short_window_shrinks_the_previews_and_keeps_every_option_in_view`, `quality_is_one_choice_of_three_and_files_keep_the_way_back`) filled the tray and expected the picture's size in the same step, which held only while the size was read on the UI thread. They now start the read the way a pick does (`check_tray_videos`) and let it land with `run_until_parked`. The quality test also asserts that HD is not on offer before the read lands.

Noted, not changed (S4):

- `screen_cover.rs` `X11_LAST` caches the last X11 work-area answer keyed by window id and window bounds. A panel that appears or hides while the window neither moves nor resizes leaves the viewer's chrome on the old work area until the next move or resize. It is a cache of a server read, not a stage; a fix would listen for `_NET_WORKAREA` changes on the root window.
- The docked caption's Folded/Open choice is kept when the controller is put back home and dragged out again in the same item. Harmless.

## Gate

`GATE OK core=8610 0 ui=3782 0` on a7ac0c33c (before the merge: core=8595, ui=3755). Pushed to `claude/sweep-r2-followup` as a fast-forward from 0a22b2d88.
