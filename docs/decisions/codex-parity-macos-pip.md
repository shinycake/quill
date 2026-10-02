# Native macOS video Picture-in-Picture

The video viewer opens a native floating window on macOS. It shares decoded frames and the playback clock with the main viewer, supports Pause/Play and Return to viewer, and closes when playback is stopped or the account changes. Window creation is deferred until the owner entity is no longer leased, because GPUI renders new windows synchronously. No additional player or frame extraction is created for PiP. The native panel stays visible when the app deactivates and participates in Spaces, including full-screen auxiliary placement. This uses the already installed AppKit and raw-window-handle crates.

The native macOS smoke check verifies WindowServer floating level 3, visibility while another app is active, changing video pixels, paused pixels staying identical, resume and return to the original viewer. Demo frames use a negative cache ID so they cannot overwrite live TDLib file caches. UI debug/release compilation and formatting passed.

Platform scope: macOS implements this item. The control is unavailable on other hosts: GPUI Floating has no guaranteed always-on-top behavior on X11/Wayland in the pinned backend. Linux’s existing blocker remains. This is a native macOS completion, consistent with the checklist’s other host-scoped features.
