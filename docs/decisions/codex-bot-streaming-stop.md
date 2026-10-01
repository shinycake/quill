# Streaming bot replies and Stop

The pinned TDLib 1.8.67 schema supports updatePendingMessage and stopPendingMessage. The reducer tracks one ephemeral reply per chat/forum topic, replaces successive drafts and honors pending_text_message_period (TDLib's native default is 30 seconds). Incoming final messages clear only that thread's pending reply. Expiry runs both on reducer updates and the existing UI polling loop; pending replies never become persisted history or notifications.

The visible pending reply uses existing text/entity and rich-block rendering, including spoiler handling. Drafts have no server message ID: callback buttons/full-message fetch controls are excluded until a real message arrives. Spoiler keys occupy a separate namespace from real history. The Stop button follows can_stop, disables while the exact request is pending, and allows retry after an error. It sends the full int64 draft identifier and a MessageTopicForum or null topic, matching the pinned API.

No optimistic deletion: only a correlated OK or matching stop update removes the draft, or keeps it as stopped when keep_on_stop is true. A late acknowledgment for an older draft cannot stop its replacement. Further queued updates for a confirmed stopped draft cannot re-enable Stop. Non-stoppable replies are still labeled generating, not stopped.

Validation: one integration regression covers int64 fidelity, topic/null serialization, request dedup, failure/retry, late-ACK isolation, keep/remove behavior, final-message replacement, matching stop updates, expiry, malformed topic IDs and authorization/capability guards. Core clippy and UI compilation pass. Live bot streaming remains unverified without an authenticated account.
