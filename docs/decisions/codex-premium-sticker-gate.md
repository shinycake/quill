# Premium sticker sending

TDLib's regular sticker full type says a non-null premium animation makes the sticker Premium-only (pinned schema line 426). The parser retains that requirement separately from messageSticker.is_premium, which controls animation playback. The picker and suggestion cells label locked stickers Premium. Their shared send handler shows “Sending this sticker requires Telegram Premium” and preserves the draft/reply/picker when blocked.

The shared driver independently checks known picker/search/favorite/recent/suggestion and chat/topic-history metadata against the current account's Premium entitlement before dispatch. Missing account entitlement keeps the local gate closed for a known Premium sticker; unknown sticker metadata is still enforced by TDLib. The existing message playback flag is unchanged.

Validation: one focused regression covers full-type parsing, a regular sticker, non-Premium rejection, updated own-user entitlement, and history metadata after picker caches clear. The focused sticker tests, core clippy and UI compilation pass. Live Telegram behavior is not yet verified.
