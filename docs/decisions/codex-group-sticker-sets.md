# Group sticker and custom emoji packs

Eligible supergroups expose installed regular/custom emoji pack choices in the existing management panel, with current selection and removal. Catalog loading reuses the shared account caches without opening either picker, deduplicates pending fetches, and remains retryable. Mutation buttons wait for requests; displayed selection comes only from supergroup full-info updates. Request refusals now reach the shared chat-action error notification instead of being swallowed. Raw server error text is not retained.

The existing driver regression now covers both catalog request shapes, capability refusal, deduplication, closed picker state, non-optimistic mutation, safe error notification, and retry/removal. Full core/replay/integration tests, strict core clippy, and native Mac UI compilation pass. Actual group changes against a live account remain unverified. Users can install other packs in the existing pack settings before assigning them here.
