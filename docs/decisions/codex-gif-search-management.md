# GIF search and saved collection

The picker offers Saved, Search and Trending (empty query), pagination, Save GIF and confirmed Remove saved controls. TDLib's animation_search_bot_username option resolves the bot through searchPublicChat; no provider or bot name is hardcoded. updateAnimationSearchParameters supplies visible provider attribution and clickable emoji query shortcuts. Search uses the existing getInlineQueryResults parser and saved controls use the existing add/removeSavedAnimation builders.

New queries discard earlier tracked resolution/pages. Provider bot changes invalidate old results and resolution. Pages deduplicate files and stop on an empty or repeated cursor. Search and saved errors are separate, so a saved-list reply cannot hide a failed search. Search thumbnails use the same sandboxed file/download path as saved animations.

Confirmed mutations cancel older saved-list fetches and reload the server collection. Errors preserve saved entries. The saved cache tracks successful empty lists and failed fetches, preventing automatic refetch loops; explicit reopen/Saved retries. updateSavedAnimations invalidates older fetches and marks closed panels stale for reopening. The picker moves into its own module and remains height-bounded. Search input clears on account switch.

Validation: focused GIF parser/state/driver tests cover current-query resolution, old replies, paging duplicates/repeated cursors, thumbnail requests without a chat, mutation serialization/confirmation, failed deletion, empty/error cache stability, provider changes and closed-panel invalidation. Core clippy and UI compilation pass. Live Telegram behavior and GIF loop playback are not verified in this slice.
