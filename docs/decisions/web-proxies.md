# web-proxies: WEB as a proxy type (`parity:data-proxy-web`)

## tdesktop
`boxes/connection_box.cpp` and `mtproto/mtproto_proxy_data.cpp` add `ProxyData::Type::Web`:

- The add/edit box has a fourth radio, `WEB`. Choosing it swaps the host and port inputs for one
  "Web proxy server" field (placeholder `proxy.example.com`) and keeps the secret field. The
  warning under the radios is `lng_proxy_web_warning` for WEB and the sponsor warning for MTProto.
- The address is a host with an optional base path (`host/path`). A pasted `https://` URL is
  accepted. The host must be a domain with a dot, lower-cased and punycoded, never an IP or an
  all-numeric last label. The path is `/`-separated `[A-Za-z0-9][A-Za-z0-9_-]*` segments, at
  most 128 characters. The port is always 443. The secret follows the MTProto rules, except that
  a fake-TLS (`ee`) secret is unsupported.
- Links are `tg://webproxy?server=<host[/path]>&secret=<secret>` (and `t.me/webproxy`). With a base
  path the secret is `base64url(0x70 + secret)`, and an unmarked secret on a link with a path is
  rejected. `host` works as an alias of `server`.
- Rows show `WEB host/path` with no port. A web proxy is never pinged; its status is "not tested"
  until it is in use, then online, connecting, "retrying built-in web transport…" or not
  available. The row menu has "Open browser", and the apply box leaves out the port, the status
  check and the top-menu share items.

## What changed in Quill
- `ProxyKind::Web`, `ProxyDraft::path`, `ProxyDraft::web`, the web checks in `validated`, the
  `proxyTypeWeb` JSON both ways and the `webproxy` link parse and share live in `src/proxy.rs`.
  The address, link-secret and row-state rules are in the new `src/proxy_web.rs`.
- The editor, the link confirmation box and the list row (`src/ui/proxy/`) follow the tdesktop
  layout above. `proxy_row.rs` was split out of `mod.rs` to stay under the size limit.
- New strings use tdesktop's keys (`lng_proxy_web_*`, `lng_proxy_online`, `lng_proxy_connecting`,
  `lng_proxy_unavailable`) and one Quill key for the form's validation line.
- The `idna` crate (already in the lockfile through `url`) does the punycode step that
  `QUrl::toAce` does in tdesktop.

## Where Quill differs
tdesktop carries the traffic itself: its built-in web transport talks to a hidden web view and,
when that cannot run, offers "Open browser" with a "waiting" status. TDLib 1.8.69 moved this
behind `updateWebProxyCarrierOpened`: the app must host the relay page in a hidden web view with
a bridge object, and the schema forbids showing or opening that URL. Quill has no such web view in
the main process (the mini-app web view is a separate helper process), so:

- Quill answers every carrier request with `reportWebProxyCarrierError` (`Unsupported`, or
  `Refused` for the temporary carrier of a ping or test) and the row of the enabled web proxy says
  "not available". Without that answer TDLib would wait for a page that never comes.
- "Open browser" and the "waiting" status have no equivalent through TDLib and are not built.
- Testing: tdesktop does not test web proxies, so Quill hides the row's check button and the link
  box's status row for them.

Hosting the carrier (a hidden web view per carrier, the bridge object and binary relay through the
helper process) is a separate feature. Until it lands, a web proxy can be added, edited, shared
and enabled, but it will not carry traffic.

## How verified
Unit tests for the address rules, link secret, validation, `proxyTypeWeb` both ways and web
links (`src/proxy_web.rs`), a driver test for the carrier refusal
(`src/connect/tests/proxy.rs`), a UI test of the form with the WEB type
(`src/ui/proxy/web_ui_tests.rs`), and light and dark captures of the form and the list from
`ready-proxy-web-form` and `ready-proxy-web-list`.
