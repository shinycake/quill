# Resolve native screen capture metadata

DesktopCapturerModule parses the input as JSON and requires an id. The adapter instead passed NULL and incorrectly documented this as selecting the default display. Resolve the first actual enumerated screen's metadata and keep its CString alive while issuing sources. The shared source resolver retains macOS system-default audio behavior and explicit device selections. Both 1:1 and presentation capture use it; external peer screen playback still has no capture input.

The opt-in native regression checks that the enumerated screen contains the required source ID and that the video description receives those bytes. It does not construct a desktop capturer or record any screen content. Native default-audio, key-exchange and local transport checks remain in place. A live screen-sharing session is still unverified; group-call completion stays unchecked.

Contract: [DesktopCapturerModule v3.0.0](https://github.com/pytgcalls/ntgcalls/blob/v3.0.0/ntgcalls/src/media/devices/desktop_capturer_module.cpp).
