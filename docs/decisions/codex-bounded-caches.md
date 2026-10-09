# Bounded caches and linear paged merges

Quality pass; no tdesktop behavior change (tdesktop bounds its caches by
size in `Data::Session`/`Images`, which is the model followed here).

Landed:
- `group_video_images` is pruned to the live group call on every insert and
  from `sync_group_call_window` (so it empties when the call ends); evicted
  tiles go to `image_budget::retire_all`.
- New `src/ui/lru.rs`: a count-capped LRU. Used by the call backdrop (8),
  `circle_mask` and `corner_mask` (32 each) and `blurred_preview` (256). The
  last two used to drop the whole map (or never evict); they now evict one
  least-recently-used entry and retire its GPU image.
- Chat event log and inline query paging dedupe with a `HashSet`
  (`src/state/paging.rs`), preserving order, instead of a scan per item.

Skipped (already fixed on main): `local_path.rs` verdict cache already has
`VERDICT_CAP` with oldest-first eviction and a test. The call backdrop was
already bounded (drained at 8); it only moved to the shared LRU.

Verified: unit tests for the LRU cap, dead-call tile removal and paging
dedupe; gate.sh.
