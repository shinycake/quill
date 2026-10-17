# Shared media: row menu and multi-select

## What tdesktop does

`Info::Media::ListWidget::showContextMenu` builds the menu of a gallery row
in this order, each entry only when it applies: Go to message, Cancel
download (while the file downloads) or Show in folder (once it is on disk and
saving is allowed), Forward, Delete, Select (Deselect when ticked). Ticking a
row switches the section header to "N selected" (`lng_media_selected_*`,
worded by tab) with Forward, Delete and Clear. Forward and Delete appear only
when every ticked message allows it (`canForwardAll`, `canDeleteAll`), and a
click on a row or tile while rows are ticked ticks or unticks it instead of
opening it.

## What changed

- `src/state/shared_media_select.rs`: the selection (`SelectedMedia`, one
  entry per ticked message with the `messageProperties` answer recorded when
  it was ticked), the group rules (`can_forward_selection`,
  `can_delete_selection`, `can_revoke_selection`), the menu planner
  (`media_menu_items`, pure and unit tested) and `remove_messages`, which
  `updateDeleteMessages` calls so a deleted message leaves every tab and the
  selection.
- `src/ui/shared_media_select.rs`: the context menu builder shared by list
  rows and grid tiles, hover prefetch of the row's rules
  (`getMessageProperties` through the existing menu-actions slot), the round
  tick, and the header. Forward reuses the chat's forward picker and Delete
  reuses `confirm_delete_ids`, the box the chat selection bar uses; "also
  delete for everyone" is offered only when every ticked message allows it.
- The row and tile menus replace the single "Go To Message" entry. The
  selection clears when the tab, the chat or the gallery changes.
- Demo: `QUILL_DEMO_SHARED_MEDIA_SELECT=2` ticks the first two rows of the
  `ready-shared-media` demo for captures.

Unknown rules allow nothing: a row whose answer has not arrived offers no
Forward or Delete, so the header never offers a guess.

## How verified

- Unit tests: menu order and gating, group rules, deletion pruning.
- Window tests (`src/ui/shared_media_select_tests.rs`, fixtures only; no
  message is deleted or forwarded): the header appears on ticking, a click
  while ticking toggles instead of opening the viewer, Clear empties the
  selection.
- Captures of the selection header in light and dark. The right-click
  popup itself is covered by the planner tests; a live capture of it needs a
  real session.
