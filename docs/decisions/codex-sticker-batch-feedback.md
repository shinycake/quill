# Sticker batch counts and feedback

The picker shows installed counts, offers Install displayed sets for available search/trending/archived sets, and confirms Remove all installed. The driver validates the whole input before dispatch, deduplicates IDs and rejects overlapping batch/mutation/reorder work. Each set has its own correlated TDLib request; the batch is not atomic.

The picker retains confirmed, pending and failed counts. A successful sibling cannot erase earlier failures, duplicate replies cannot double-count outcomes, and starting another batch resets the previous receipt only after overlap validation. Confirmed changes use the existing catalog reload path. Immediate send failures are counted without marking those sets complete.

Validation: a focused driver regression covers invalid IDs, duplicate inputs, overlap rejection, mixed confirmed/error results, duplicate acknowledgments and a new removal batch. The full focused sticker group (27 tests), core clippy and UI compilation pass. Live Telegram behavior is not yet verified.
