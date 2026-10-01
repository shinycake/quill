# Sticker search and set management

The picker has a Search tab with an explicit query field, installed title/name matches, TDLib-discovered sets, and matching stickers. Sticker results page with server offsets and deduplicate by file ID. A new query cancels tracked older searches and previews; late answers cannot replace newer results. Selecting a found set opens its titled preview grid with Install/Remove and Back to results.

Install and Remove use `changeStickerSet`. Removal reuses the existing kit confirmation dialog. The tracked request carries set ID and intended state; only `ok` changes catalog flags and refetches the authoritative installed list. Contradictory changes to one set are serialized, independent sets can proceed, and late installed-list answers from before a mutation are discarded even if the picker has closed. Errors leave the previous installed state intact. Search text is cleared on account switch.

The sticker picker moves from composer_ui into its own module to keep further media-keyboard work within the repository's file-size guidance. Trending refresh now resets its page offset through the same request path as reopen, rather than appending a refresh as a later page.

Validation: focused driver/reducer/request tests cover stale queries, duplicate search results with advancing offsets, previews/back navigation, confirmation-only changes, error rollback, contradictory requests, and closed-picker fetch invalidation; UI compilation and core clippy pass. Live Telegram interaction is not yet verified.
