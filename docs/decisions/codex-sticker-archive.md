# Archived regular sticker sets

Installed rows offer Archive. The Archived tab shows paged regular sets and Restore controls, and the settings entries open the same picker in a kit dialog so it is available without a selected chat. Reusing the picker keeps titled previews, loading/errors and install/remove behavior consistent. Confirmation dialogs have priority over the settings view. Closing or switching account clears the settings state.

Archived requests use `getArchivedStickerSets` with a string-encoded last-set-ID cursor. Pages deduplicate IDs, stop on exhaustion or an unchanged cursor, and refresh from zero. Archive/restore flags use `changeStickerSet`; failed changes retain the catalog. Confirmed changes invalidate installed and archived catalogs and any earlier archived fetch, then reload the open view. Archived previews are cleared on mutations and their pending requests discarded so late replies cannot revive a restored set.

Validation: focused regressions cover request flags/type, cursor paging, duplicate pages, exhaustion, failed restore, pre-mutation fetch rejection and confirmed archive/restore. Core clippy and UI compilation pass; live Telegram behavior remains unverified.
