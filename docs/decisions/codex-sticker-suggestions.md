# Composer sticker suggestions

Composer changes call the existing trailing-emoji suggestion driver. A horizontal row above the composer reuses the picker's thumbnail/send cells. The picker exposes persisted Installed + recommended / Only installed / None modes. Changing mode cancels older suggestions and re-evaluates the current draft; non-emoji drafts and None clear the row.

Only installed waits for the authoritative installed list before searching and filters returned sets against that catalog. Changes while waiting keep the latest emoji, and old search replies cannot replace newer results. Suggested thumbnail downloads run even when the picker is closed. Picker and suggestion cells share the same sandboxed paths and send request.

Validation: one driver regression covers catalog-first ordering, latest-emoji selection, repeated-query deduplication, stale replies, installed filtering, closed-picker thumbnail downloads and mode-off cleanup. Existing suggestion and sticker tests, core clippy and UI compilation pass. Live Telegram interaction is not yet verified.
