# Recreate native P2P transport on retry

The driver retried connect on the same native P2P object. ntgcalls rejects repeated key exchange and repeated connect; a native failure may also remove the transport while Quill's mapping remains. Recreate the transport with existing hangup/start helpers before retrying, restore the same TDLib call mapping, and retain the current device, camera and screen choices rather than reverting to initial parameters. Retain desired media configuration after failed attempts so the next retry recreates it too.

Initial connection and reconnect share mute application. If mute cannot be applied, stop the replacement transport and clear its queued state callbacks rather than leaving a live unmuted transport behind a muted UI. The driver retains its existing three-retry limit and TDLib signaling authority.

The actual macOS sidecar regression performs key exchange, then retries twice with an unsupported signaling version: that version fails before a network connection or capture can be created. Before the fix the retry failed at key exchange; afterward both attempts reach protocol negotiation on fresh transports. Existing driver regressions check repeated mute application and teardown if applying mute fails, including a queued stale Connecting callback. No live Telegram call or media recording is performed.

Native contract: [P2PCall v3.0.0](https://github.com/pytgcalls/ntgcalls/blob/v3.0.0/ntgcalls/src/instances/p2p_call.cpp). End-to-end call quality remains unverified; no parity completion is declared.
