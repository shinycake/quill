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
when that cannot run, offers "Open browser" with a "waiting" status. TDLib 1.8.69 moved the
transport into TDLib (`WebProxyManager`): the app only hosts the relay page in a hidden web view
with a bridge object, and the schema forbids showing or opening that URL. So "Open browser" and
the "waiting" status have no equivalent through TDLib and are not built. tdesktop does not test web
proxies either, so the row's check button and the link box's status row stay hidden for them.

## Carrier hosting
### The host: the web view helper, on every platform
Two candidates:

- the kit 0.8 `gpui-webview` crate, in the GPUI process. It covers macOS, Windows and Linux X11
  only. On Wayland it has nothing, and WebKitGTK needs a GTK main loop the GPUI process does not
  run (the reason the mini-app view lives in a helper, `codex-miniapp-webview.md`).
- the wry view of the `quill-webview` helper process, with a binary relay over its pipe.

The helper won. Wayland must work and no feature may be limited to some platforms, so
`gpui-webview` would have needed the helper next to it anyway; one path for all three is less code
and one set of tests. It also keeps a web page's renderer out of the process that holds the TDLib
database and keys, and a crash there cannot take Quill down.

### How it runs
- **One process per carrier.** `quill-webview --carrier` makes a hidden window (never shown, no
  Dock icon, no focus) with one incognito wry view. The schema asks for a web view per carrier;
  a process each also makes teardown a kill and isolates failures. The URL, origin, path prefix
  and bridge name go over stdin as the first line, never on the command line, so the capability in
  the URL is not visible in `ps`; nothing logs it, and `CarrierUrl`'s `Debug` prints a placeholder.
- **The bridge** (`crates/webview-host/src/carrier_bridge.js`) is injected at document start and
  defines the read-only, frozen `TelegramWebProxy` (the name TDLib sends, checked to be a plain
  identifier) with exactly `postMessage` and `onmessage`, only when `window === window.top` and
  `location.origin` is exactly the carrier's origin. `postMessage` passes an `ArrayBuffer` or
  typed array as base64 and a string as text, and throws `TypeError` for anything else. Every
  message to the helper starts with a random per-carrier token kept inside the script's closure,
  and the helper also checks the sending frame's URL against the origin where the platform reports
  it (WebKitGTK reports the main frame for every frame, so the token is what counts there).
- **Scope.** The navigation handler allows only the carrier's origin under its path prefix (no
  dot segments or encoded slashes). Any other navigation is refused and fails the carrier as
  `LoadFailed`. Loads and requests are held to the prefix by a Content-Security-Policy the bridge
  script puts in `<head>` before the page's own markup (`default-src`, `connect-src` with the
  `wss:` form, no frames, no forms, no `<base>`). New windows, downloads and permission requests
  are refused.
- **Relay.** Binary data stays base64 end to end, because that is what TDLib's JSON `bytes` already
  are: `updateWebProxyCarrierData` goes to the helper as is, the page gets an `ArrayBuffer`, and
  what the page posts goes back in `processWebProxyCarrierMessage` without Quill decoding it. The
  driver relays data before the reducer runs (bytes are not state) and only for carriers the table
  says are live.
- **Backpressure and limits.** The helper hands the page one delivery at a time and waits for
  the page's acknowledgement (tdesktop does the same). It queues at most 1,024 deliveries or
  16 MiB, a delivery may be at most 1 MiB plus slack (TDLib's own cap), and a page message at most
  2 MiB binary or 64 KiB text (TDLib's text cap). Quill's side queues at most 1,024 commands or
  16 MiB per carrier. Going over any of these, or a malformed line, fails the carrier as
  `Terminated`; nothing ever blocks the UI thread.
- **Crash and hang.** WebKit's content process dying (macOS) fails the carrier at once. The
  helper probes the page every 3 s and fails it after 10 s without an answer or 10 s on one
  unacknowledged delivery. Quill fails a helper that sends no `alive` line for 15 s and kills it.
  A helper that exits before it says `ready` (no WebKitGTK, no WebView2 runtime) is reported as
  `Unsupported`, so TDLib stops asking until the proxy is enabled again; after `ready` an exit is
  `Terminated` and TDLib retries with its own backoff (2 s doubling to 30 s).
- **Teardown.** `updateWebProxyCarrierClosed` closes the view: if the page posted anything it first
  gets `{"t":"close"}` and the view goes 200 ms later, otherwise at once; messages after the close
  are dropped. Logging out, a closed client and quitting (`begin_close`) shut every carrier down.
  The writer thread owns the child and kills it whenever its queue goes away, and the helper exits
  by itself when Quill's end of the pipe closes, so no view outlives Quill.
- **Temporary carriers** (a `pingProxy` or `testProxy` of a web proxy) are refused with
  `Refused`, as the schema allows. Quill never pings or tests web proxies (tdesktop doesn't
  either) and auto-switch skips them, so these would only start a web view for nothing.

### State
`ProxyState::carriers` (`src/proxy_carrier/mod.rs`) is the only record of carriers: one entry
per carrier TDLib opened with its phase (opening, relaying, failed) and the work the driver still
has to do (open, report, close). The reducer applies `updateWebProxyCarrierOpened` / `Closed` and
the host's failures to it; the driver (`src/connect/proxy_carrier.rs`) does the queued work through
the `CarrierHost` trait and relays data. The live host is `src/proxy_carrier/helper.rs`; the
tests use a fake one. A failed carrier marks its proxy's row "not available" until TDLib opens the
next one, which shows "connecting" again.

### Enabling a web proxy
A client whose host can run carriers (the helper is next to Quill's executable, as in every
package) enables web proxies like any other: the driver refusal, the "use custom proxy" radio
skip and the row note apply only to a build without the helper. There, the driver still refuses
`enableProxy`, saves an added or edited web proxy disabled, and the row says "Web proxies aren't
supported in this version yet".

### Cost
An idle carrier on macOS (Apple M-series, macOS 27, debug helper build, local relay page):
physical footprint 24 MB for the helper plus 13 MB WebContent, 13 MB GPU and 6 MB Networking
processes, 55 MB in all (`footprint`). Quill itself adds two threads per carrier and no memory
beyond the queues. Each signed-in account's TDLib asks for its own carrier, so the cost is per
account using the web proxy. A 3-byte round trip from Quill's host through the helper, the page
and back (`src/proxy_carrier/helper_live_tests.rs`, 20 rounds, polled every 200 µs) takes
0.54 ms at the median and 66 ms at worst, which was the first, cold round. In the app, data
crosses the UI thread once each way (the helper's reader thread wakes the poll loop). If relay
latency ever becomes a problem, a fast path on the receive thread would remove that hop.

## How verified
Carrier hosting:

- Pure tests of the bridge-origin and navigation rules, the bridge-message format, base64 and size
  limits, and the one-at-a-time delivery queue (`crates/webview-protocol/src/carrier.rs`,
  `crates/webview-host/src/carrier.rs`), and of the carrier table (`src/proxy_carrier/tests.rs`).
- Driver tests with a fake host (`src/connect/tests/proxy_carrier.rs`): open, page → TDLib text
  and binary, TDLib → page data in order, close with and without `{"t":"close"}`, failures
  reported once with TDLib's error shapes, refused starts, temporary carriers, quit and logout
  while open, and the URL kept out of `Debug` output.
- The live host against stand-in helpers (`src/proxy_carrier/helper.rs`): a missing program and
  one that exits before `ready` are `Unsupported`; a closed carrier reports nothing.
- The real helper with WKWebView on macOS against a local relay page
  (`crates/webview-host/tests/carrier_live.rs`, ignored by default, run with
  `cargo test -p quill-webview --test carrier_live -- --ignored --test-threads=1`): the bridge
  exists, is frozen, has exactly its two members and rejects other values; bytes go both ways in
  order; a fetch outside the path prefix is blocked by the policy and never reaches the server; a
  navigation outside the scope fails as `LoadFailed`; a page that hangs in `onmessage` fails as
  `Terminated`; a plain-`http` origin is refused before loading. Debug builds accept a loopback
  `http://127.0.0.1` origin for this test when `QUILL_WEBVIEW_CARRIER_LOOPBACK` is set; release
  builds never do.
- Quill's live host against the real helper (`src/proxy_carrier/helper_live_tests.rs`, ignored by
  default): the page's hello reaches the host, and 20 deliveries come back reversed and in order.
  After the close, the host reports nothing more.
- Light and dark captures of the proxy list with the web proxy enabled (`ready-proxy-web-list`).
- Not run: Linux (WebKitGTK, X11 and Wayland) and Windows (WebView2) are built by CI, not run
  here. No public web proxy was found to try, so a real relay page and real traffic are untested.

Earlier:
Unit tests for the address rules, link secret, validation, `proxyTypeWeb` both ways and web
links (`src/proxy_web.rs`), a driver test for the carrier refusal
(`src/connect/tests/proxy.rs`), a UI test of the form with the WEB type
(`src/ui/proxy/web_ui_tests.rs`), and light and dark captures of the form and the list from
`ready-proxy-web-form` and `ready-proxy-web-list`.
