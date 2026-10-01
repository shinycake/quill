# Sticker picker tabs

The existing composer picker now exposes Installed, Recent, Favorites, and Trending. Recent and Favorites share the existing sticker grid and sandboxed thumbnail download path. Favorite/Unfavorite uses the actual TDLib methods and refetches favorites after confirmed success; an older pending fetch is invalidated so it cannot undo a confirmed change. Clear Recent waits for TDLib's `ok` before clearing and invalidates an older pending fetch. Errors remain visible with tab retry; duplicate requests are suppressed.

Trending fetches regular sets, marks them viewed, opens the same set preview grid, and offers additional pages using the server total. Switching back to Installed restores an installed set, including when a late installed-list answer arrives during discovery. Reopening the picker refreshes its selected tab.

The installed-set and saved-GIF empty states already existed in the picker; this slice verifies them and declares their previously unchecked checklist entries. This slice does not declare sticker-set install/remove or animated/video playback complete.

Validation: focused sticker driver/reducer/request tests, no-default clippy, and UI compilation. Live Telegram account behavior remains unverified; fixtures exercise the real JSON dispatcher and TDLib request shapes from the pinned schema.
