# Proxy extras (`parity:data-proxy-extras`)

## What tdesktop does

- `boxes/connection_box.cpp`: ProxiesBox has radios Disable / Use system proxy settings / Use custom proxy, "Use proxy for calls", "Auto-switch proxies" with a timeout slider, and a per-proxy menu with Share (link) and Show QR code (`ShowProxyQrBox`, encodes the `tg://` link).
- `window/window_connecting_widget.cpp`: the connecting strip carries a proxy shield icon (`ProxyIcon`) while a proxy is enabled.
- `settings_advanced.cpp`: a "Connection type" button showing the mode and transport, opening the proxy box.

## Already in Quill before this change

Disable / custom radios, calls toggle, auto-switch with timeout (`proxy.rs` `rotation_step`, `proxy_prefs.json`), copy link, share list, sidebar shield, strip "Proxy settings" link.

## What changed

- Per-row "Show QR code" button and a QR box (`src/ui/proxy_qr.rs`) that reuses `render_qr_image`; it encodes the `tg://` link; "Copy link" button.
- Shield icon in the connection strip while a proxy is enabled.
- Settings row "Proxy" is now "Connection type: <value>" (`proxy_extras::connection_type_label`). TDLib does not expose the transport (TCP/HTTP), so the value is Default, or the proxy kind and endpoint.
- Pure helpers and tests in `src/proxy_extras.rs` (label, QR link, prefs persistence round trip).

## Skipped: "Use system proxy settings"

tdesktop gets this from Qt's `QNetworkProxyFactory`. TDLib only takes explicit proxies, so Quill would have to read the OS setting itself. There is no single reliable way on macOS, Linux and Windows (scutil/CFNetwork, registry plus PAC, GNOME/KDE/env variables). Not implemented rather than guessed; the parity item stays unchecked (no fragment).

## Verified

Unit tests; gate; temporary demo capture of the QR box (not committed).
