# Preserve the peer's P2P call negotiation

TDLib's callStateReady.protocol describes the other participant's supported protocols, not Quill's capabilities. Previously the envelope discarded this field and the driver connected using its own full version list. Preserve the peer's library_versions and pass them to ntgcalls, which selects a common signaling version. Preserve and pass custom_parameters too, as required by the pinned TDLib schema; empty parameters remain NULL for the native API. These values remain out of diagnostics.

Enable external playback receivers for both peer camera and screen video before connecting. NULL playback descriptions disable these receivers, so the existing decoded-frame callback never fired for remote video. The native source check covers configuring these receivers without opening capture devices.

Before native key exchange, require exactly 256 bytes: ntgcalls' P2P connect copies a fixed 256-byte Telegram auth key. Reject short and oversized keys before handing them to the native implementation.

The existing ready-call driver regression now uses a peer advertising only 13.0.0 while Quill advertises four versions, and checks that the peer's version and custom JSON reach the engine. The native opt-in source regression also rejects 0-, 1-, 255- and 257-byte keys without connecting or capturing. No end-to-end call or new parity completion is claimed.

Contract: schema/td_api.tl:7051–7068 and [native P2P connection](https://github.com/pytgcalls/ntgcalls/blob/v3.0.0/ntgcalls/src/instances/p2p_call.cpp).
