# Proxy and connection settings (gap-audit batch 9)

Quill now has tdesktop's proxy box, backed entirely by TDLib's own proxy list.

**What shipped**
- `src/proxy.rs` (pure): proxy model, tdesktop's MTProto secret validation (hex or base64url, normalised to hex for TDLib), `tg://proxy|socks` and `t.me/proxy|socks` link parsing, share-link building, ping status, and the auto-switch state machine.
- Driver (`connect/proxy.rs`) and reducer (`state/session_proxy.rs`): `getProxies`, `addProxy`, `editProxy`, `enableProxy`, `disableProxy`, `removeProxy`, `pingProxy`, `setOption("prefer_ipv6")`. These work before sign-in (gate is "not shutting down", not "auth Ready"). Mutations never edit the list optimistically: the ack marks it stale and `getProxies` is refetched.
- UI (`ui/proxy.rs`): Settings › Proxy opens the list (Disable / Use custom radios, IPv6, calls, auto-switch with 5/10/15/30/60 s, per-row check / copy link / edit / delete, Add, Add from clipboard, Share Proxy List). Separate add/edit box and a "Proxy Server — Connect Proxy" confirmation for links, with the one-time "exposes your IP" warning before Check Status.
- Launch / OS links to a proxy are parsed locally in `pump_deep_link` before the "auth Ready" gate, so a blocked user can get in through a link. Invalid links show tdesktop's three messages in the existing link info dialog.
- Connection strip: the existing "Connecting…/Connecting to proxy…/Updating…/Waiting for network…" labels were already present; added a "Proxy settings" link while connecting and a shield (checked when connected) in the sidebar header while a proxy is enabled.
- "Use proxy for calls" stays the `CallPrefs` flag; the proxy box shows it only for an enabled SOCKS5 proxy and `ProxyState::call_proxy` hands the proxy to `calls::proxy`.

**Deliberate differences / left out**
- Schema 1.8.67 has no `getProxyLink`; links are built locally exactly as tdesktop does (`tg://` or `https://t.me/`, HTTP proxies not shareable).
- No "use system proxy settings": TDLib has no such mode.
- No share QR (the `qrcode` crate is UI-only; follow-up) and no web proxies (tdesktop-only).
- Delete is immediate (no "Restore" menu).
- Clicking a `tg://proxy` link inside a message still goes through the OS handler; only launch / forwarded links use the in-app flow.
- Auto-switch preferences live in `proxy_prefs.json`; the proxy list itself is TDLib's.
- The IP warning acknowledgement is per run (tdesktop persists it).
- Rows auto-ping only on demand (not on open) to avoid contacting every proxy unprompted.

**Tests**: recorded-JSON driver tests (`connect/tests/proxy.rs`), request-builder tests, link / secret / rotation unit tests. Demo capture: `--screenshot-demo ready-proxy` with `QUILL_DEMO_PROXY=list|edit|link|link-bad`.
