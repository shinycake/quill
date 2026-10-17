# Per-update and per-render scans

Round 2 of the quality sweep listed work the app repeated for every
envelope or every frame. This note records what changed and what was left
alone. No behavior changes are intended.

## Poll batches

`ConnectDriver::begin_batch` / `end_batch` bracket one drain of the receive
queue (`ui/notifications/mod.rs`, `ui/background_accounts/mod.rs`). Callers
that ingest without them behave as before.

- Chat-list order. `rebuild_main_order` now sets a dirty flag
  (`Session::chat_order_sync`) and sorts at once unless a batch is open;
  `end_update_batch` sorts once if anything moved. `main_order` and
  `archive_order` stay plain data derived from the chats, so the result is
  the same sort over the same chats. Test: a batch of position updates ends
  with the order that sorting after every update gives.
- Open-chat custom emoji. After each envelope the driver scanned the open
  chat, the chat list and the shared media for custom emoji ids twice (once
  to resolve, once to queue downloads). It now scans once per run and, in a
  batch, once at `end_batch`. The resolve request names every id the batch
  brought (test in `connect/tests/update_batch.rs`).

## Custom emoji cache

`custom_emoji_stickers` is still a `Vec` (the pickers and chips index into
it), so lookups stay linear where they are single and use a `HashSet` or
`HashMap` built per call where they were repeated per id
(`resolved_custom_emoji_ids`, the status panel). The merge is one pass
instead of a `retain` per sticker, and the Vec is capped at
`CUSTOM_EMOJI_CACHE_CAP` (4000), dropping the oldest. A dropped id loses
its attempted mark, so it resolves again when it shows.

## Borrowing instead of cloning

`media.files` / `media.downloads` are borrowed in the sponsored footer, the
GIF picker, the story viewer overlay and the viewer's save/copy/rotate
actions; the emoji status and pack panels borrow `EmojiPanel`. The payment
form is cloned only on the frame that prefills the dialog.

Not changed: the media viewer overlay still clones `media.files` per
render. Its render calls `&mut self` methods between uses of the map, so a
borrow does not compile without restructuring the whole function.

## Sidebar rows

- Folder chip text is upper-cased once per list render (`tag_names`), not
  once per chat per folder; row heights ask `row_has_tags` and no longer
  build the chips.
- The rows of a list share one `FolderTagContext`, stored beside the item
  list it was built with, instead of cloning the folder names per row.
- The pin-drag check asks `has_two_pinned_chats` (stops at the second
  pinned chat) instead of sorting all pinned ids per pinned row, and the
  "has archived chats" checks stop at the first match.
- A forum row's topic line sorts references instead of cloning every topic.

Not changed: `ordered_folder_chats` sorts per list render when a folder tab
is open. Nothing in the state says when a folder's order changed, so a
cache would need a hand-kept revision; it stays a plain derivation.

## Frame upkeep

The self-destruct and live-location ticks scanned the open chat's messages
every frame before noticing their timer already ran. They check the running
timer first and scan only to start one.

## Off the main thread

- Rotate/flip in the photo viewer decodes on a background thread. The new
  orientation and the decoded image apply together when it lands
  (`finish_viewer_reorient`), so the picture never shows a half-applied
  turn; a press while a decode runs builds on the pending target, and a
  step, close or newer press drops a late result (`reorient_gen`).
- Copy photo decodes, orients and PNG-encodes on a background thread, then
  writes the clipboard.
- A notification avatar that is not cached yet is made on a background
  thread and the notification is shown when it is ready, without the icon
  if that fails. A cached icon still shows at once.

Not moved: "Copy frame" for video (the frame is already decoded, so only
the PNG encode would move).

## Verified

Unit and driver tests above, the gate, and a demo-capture of the chat list
and emoji panels (see the PR).

## Review follow-ups

- Eviction rule. The custom emoji cache still drops the oldest entries past
  `CUSTOM_EMOJI_CACHE_CAP`, but skips every id `open_chat_custom_emoji_ids`
  reports (open chat text and reactions, chat list previews and badges,
  statuses, shared media captions, the open reaction picker). The protected
  set is derived from state at each overflow, so nothing is kept in sync by
  hand. If more than the cap is visible at once the cache holds them all.
- The resolved-id set is built once per resolve call and passed down
  (`unresolved_custom_emoji_ids_in`).
- Notification avatars: one decode per uncached photo. Later notifications
  for the same photo queue behind it and show in arrival order when it
  lands. The unused `AppContext` import is removed.
