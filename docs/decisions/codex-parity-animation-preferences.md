# GIF autoplay preference

Settings → Media settings exposes the account's persisted Autoplay GIFs switch, defaulting on for existing preference files. The virtualized history renderer defers playback of downloaded GIFs to the existing cancellable decoder. Data saver, spoilers, secret media, recording, other media players and open media/story viewers suppress automatic starts. A per-chat attempted set prevents a paused or failed clip from restarting on every render; explicit Play remains available. Switching chats clears that set. Turning autoplay off stops playback immediately.

The existing player supports one visible GIF at a time. Autoplay does not request additional downloads and follows the existing auto-download policy. Concurrent GIF players remain an implementation limit.

Validation: core tests, strict core clippy and macOS UI build. The native GIF smoke demo now starts through autoplay rather than an injected Play action. Owned-window pixel captures prove it loops; an AX Pause action followed by captures proves it remains paused across renders. The media preference regression verifies an off value survives persistence and older preference files default on. Live Telegram traffic was not used.
