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
  Dock icon, never focused) with one wry view. The schema asks for a web view per carrier; a
  process each also makes teardown a kill and keeps failures apart. The URL, origin, path prefix
  and bridge name go over stdin as the first line, never on the command line, so the capability in
  the URL does not show in `ps`. Nothing logs it: `SecretUrl` (in the protocol crate, so both
  processes use it) prints a placeholder in `Debug`. On Windows the helper is a GUI-subsystem
  binary and Quill starts it with `CREATE_NO_WINDOW`, so no console window flashes up; the
  mini-app helper had the same bug and gets the same flag.
- **Each account's files stay in its own folder.** The helper gets
  `accounts/<key>/webview-carrier`, which Quill empties when the account's client starts and when
  it shuts down. WebView2 keeps its profile in `webview2/` there (without one it would write next
  to the executable, which is read-only in an installed app). The macOS content rules and the
  WebKitGTK filter are compiled into `rules/` and `filters/` and removed once loaded. All the
  carriers of one account share WebView2's browser process, because WebView2 shares it per data
  folder. They still get one renderer each.
- **Locked down before the page runs.** The page is loaded only after the platform lockdown has
  succeeded, and the helper says `ready` only then:
  - Script dialogs and printing: `alert`, `confirm`, `prompt`, `print` and `onbeforeunload` are
    replaced with non-writable no-ops in every frame before any page script runs. Natively,
    WebKitGTK's `script-dialog` and `print` signals are handled and dropped, and WebView2 has
    its default script dialogs (`AreDefaultScriptDialogsEnabled`), context menus, dev tools, status
    bar and browser accelerator keys turned off. A live test checks that all four calls return
    at once and that the page keeps running.
  - The safe mode tdesktop uses: `RTCPeerConnection` (and its prefixed and helper classes),
    `WebTransport` and `WebAssembly` are removed in every frame, and WebRTC is turned off in
    WebKitGTK's settings.
  - The path prefix is enforced natively, not only by the Content-Security-Policy. On macOS a
    `WKContentRuleList` blocks every load except the carrier's origin under its prefix (and its
    `wss:` form, `data:` and its own `blob:`). WebKitGTK gets the same rules as a
    `WebKitUserContentFilter`. WebView2 answers every request outside the scope with a 403 from a
    `WebResourceRequested` handler. A top-level page that ends up outside the scope (after a
    redirect, say) fails the carrier as `LoadFailed`.
  - What stays best effort: WebView2's request handler does not see a redirect that its network
    stack follows on its own, so a redirect out of the prefix is caught only by the CSP and the
    top-level check there. DNS prefetching of links in the page is not covered by any of the
    three platforms' filters.
- **The bridge** (`crates/webview-host/src/carrier/bridge.js`) defines the read-only, frozen
  `TelegramWebProxy` (the name TDLib sends, checked to be a plain identifier) with exactly
  `postMessage` and `onmessage`, only when `window === window.top` and `location.origin` is the
  carrier's origin. `postMessage` sends an `ArrayBuffer` or typed array as base64 and a string as
  text, and throws `TypeError` for anything else. The script takes one JSON configuration, so no
  value can turn into code. Messages to the helper start with a random per-carrier token kept in
  the script's closure. The helper calls back into the page through a hook object whose name and
  key are random per carrier, so the page cannot fake a delivery.
- **When a carrier counts as alive.** The page itself has to answer, not only the bridge. A load
  that fails, an HTTP error status (where the platform reports it), a page whose bridge script
  never started within 30 s or a page that never touches the bridge within 10 s of loading all
  fail the carrier as `LoadFailed` with the reason. WebKit does not report the status of a
  navigation, so on macOS and Linux a 404 shows up as "the relay page did not start" after the
  10 s, not at once. After that the helper asks the page every 3 s and fails it after 10 s without
  an answer.
- **Data waits for the page.** Data TDLib sends before the page has set `onmessage` is queued in
  the helper and delivered once it has. On Quill's side, an open and the data right behind it in
  the same poll batch reach the helper in that order. A delivery that was in flight when the page
  reloaded is sent again.
- **Relay.** Binary data stays base64 end to end, because that is what TDLib's JSON `bytes`
  already are. `updateWebProxyCarrierData` goes to the helper as is and the page gets an
  `ArrayBuffer`. What the page posts goes from the helper's reader thread straight to TDLib
  (`td_send` is thread-safe) as `processWebProxyCarrierMessage`, tagged with a reserved `@extra`
  whose answers the driver drops. So page traffic never crosses the UI thread and never queues in
  Quill; the UI thread only hands TDLib's data to a bounded queue.
- **Backpressure and limits.** The helper hands the page one delivery at a time and waits for the
  page's acknowledgement (tdesktop does the same). It queues at most 1,024 deliveries or 16 MiB, a
  delivery may be at most 1 MiB plus slack (TDLib's own cap), and a page message at most 2 MiB
  binary or 64 KiB text (TDLib's text cap). Quill queues at most 1,024 commands or 16 MiB per
  carrier, then fails the carrier and kills its helper. Going over any of these, or a malformed
  line, fails the carrier as `Terminated`; nothing blocks the UI thread.
- **No helper outlives Quill.** The helper reads its commands on a thread of its own, started
  before the web view. When stdin ends (Quill quit or crashed) or a line is broken, that thread
  ends the process 300 ms later even if the main loop is stuck. On Linux the kernel also kills the
  helper when Quill dies (`PR_SET_PDEATHSIG`). On Quill's side the host keeps each child itself.
  Quitting, logging out, a closed client and dropping the host kill every child from the calling
  thread. TDLib's close gives the page 1 s to receive `{"t":"close"}` before the kill. A watchdog
  on every poll and on every writer turn (so steady deliveries cannot hide it) kills a helper
  that has not said `ready` within 60 s or has sent nothing for 15 s since.
- **Hidden and idle.** The view is never shown, so the platforms would throttle it. macOS gets
  `with_background_throttling(Disabled)` and an `NSProcessInfo` activity that keeps App Nap
  away. WebView2 gets the flags that turn off occlusion and background timer throttling. WebKitGTK
  has no switch for this; the 3 s probe runs from the helper's own timer, not from page timers, so
  only the page's answer can be late. A 130 s idle soak with the window hidden passes on macOS:
  44 `alive` lines, one every 3 s, and the carrier never failed.
- **Errors follow the schema.** `Unsupported` means this system cannot run a web view, and TDLib
  then stops asking until the proxy is enabled again, so Quill sends it only when that is shown:
  the helper says so, or it exits before `ready` the way the dynamic loader does when a library is
  missing (127 on Unix; `STATUS_DLL_NOT_FOUND` and two similar codes on Windows). A lockdown that
  cannot be put in place also counts, because the page would run without it. Every other
  exit, crash, kill or failure to start is `Terminated`, and TDLib retries with its own backoff
  (2 s doubling to 30 s).
- **"Can host" is checked, not assumed.** The helper being installed is not enough. Once per app
  run, a background thread starts it in `probe` mode, which builds the view and the lockdown and
  exits. A pass is cached in the app folder for that helper build (its size and modification
  time), so later runs skip it; a fail is not cached, so installing WebKitGTK or the WebView2
  runtime is noticed on the next start. Until the check fails, web proxies are allowed. Once it
  fails (or a carrier shows the same), hosting turns off for every account and the earlier
  refusal applies: web proxies are not switched to and their carriers are refused as
  `Unsupported`.
- **Temporary carriers** (a `pingProxy` or `testProxy` of a web proxy) are refused with
  `Refused`, as the schema allows. Quill never pings or tests web proxies (tdesktop doesn't
  either) and auto-switch skips them, so these would only start a web view for nothing.

### State
`ProxyState::carriers` (`src/proxy_carrier/mod.rs`) is the only record of carriers: one entry
per carrier TDLib opened with its phase (live or failed), whether this client can host carriers,
and the work the driver still has to do (open, report, close). The reducer applies
`updateWebProxyCarrierOpened` / `Closed`, the host's failures and its hosting changes
(`apply_carrier_hosting`). The driver (`src/connect/proxy_carrier.rs`) does the queued work
through the `CarrierHost` trait and hands data to the host. The live host is
`src/proxy_carrier/helper.rs`; the tests use a fake one. A failed carrier marks its proxy's row
"not available" until TDLib opens the next one, which shows "connecting" again. The driver's
per-poll work for both UI loops is one call, `ConnectDriver::poll_ticks` (`src/connect/ticks.rs`).

### Enabling a web proxy
A client whose host can run carriers enables web proxies like any other. Without the helper, or
once the capability check failed, the driver refuses `enableProxy`, saves an added or edited web
proxy disabled, and the row says "Web proxies aren't supported in this version yet".

### Cost
Measured on macOS 27 (Apple M-series, debug helper build, local relay page):

| | Before the review fixes | After |
|---|---|---|
| Idle carrier, all processes (`footprint`) | 55 MB | 51 MB (helper 24, WebContent 13, GPU 10, Networking 6) |
| Idle carrier CPU, 30 s | not measured | 0.04 s (0.13% of one core) |
| 3-byte round trip, median / worst of 20 | 0.54 ms / 66 ms | 0.23 ms / 11 ms |
| Capability check | none | 0.21 s, once per helper build |

The round trips are not quite comparable. Before, the test polled the host every 200 µs and page
messages came back through the host's event queue. After, they come back on the reader thread
through the relay sink, which is the path the app now uses. Quill adds two threads per carrier
and no memory beyond the queues. Each signed-in account's TDLib asks for its own carrier, so the
cost is per account that uses the web proxy. Relay latency with the window in the background
(macOS App Nap, Windows occlusion) was not measured beyond the idle soak.

What else stays as it is: TDLib's own debug log may contain carrier traffic at high verbosity
levels, which Quill does not set.

## How verified
Carrier hosting:

- Pure tests of the bridge-origin, navigation and request rules, the bridge message format,
  base64 and size limits (`crates/webview-protocol/src/carrier.rs`), the bridge script and content
  rules (`crates/webview-host/src/carrier/script.rs`), the relay's start, gating and timeout rules
  (`carrier/relay.rs`), and the carrier table (`src/proxy_carrier/tests.rs`).
- Driver tests with a fake host (`src/connect/tests/proxy_carrier.rs`): open, data in order, data
  right behind its open, TDLib's answers to relayed messages dropped, close with
  `{"t":"close"}`, failures reported once with TDLib's error shapes, failed starts, hosting turning
  off and the refusal that follows, temporary carriers, quit and logout while open, and the URL
  kept out of `Debug` output. A redraw test checks that relay answers never redraw.
- The live host against stand-in helpers (`src/proxy_carrier/helper/tests.rs`): a missing program
  and an exit before `ready` are `Terminated` and keep hosting on; an exit with 127 and a helper
  that says `unsupported` are `Unsupported` and turn hosting off; page messages go to the relay
  sink with the reserved `@extra`; data for an unknown carrier fails it; shutdown ends every child.
- The real helper with WKWebView on macOS against a local relay page
  (`crates/webview-host/tests/carrier_live.rs`, ignored by default, run with
  `cargo test -p quill-webview --test carrier_live -- --ignored --test-threads=1`), 12 tests, all
  passing: bytes go both ways in order and the page hears `{"t":"close"}`; data sent before the
  page listens waits for it; `alert`, `confirm`, `prompt` and `print` return at once and WebRTC is
  gone, in frames too; leaving the scope by navigation or `about:blank` fails as `LoadFailed`; a
  redirect out of the prefix is blocked by the content rules; a 404 page and an unreachable page
  fail as `LoadFailed`; a page that hangs fails as `Terminated`; the helper exits when Quill hangs
  up; the probe starts the view and exits; a plain-`http` origin is refused before loading; and
  the 130 s idle soak. Debug builds accept a loopback `http://127.0.0.1` origin for these tests
  when `QUILL_WEBVIEW_CARRIER_LOOPBACK` is set; release builds never do.
- Quill's live host against the real helper (`src/proxy_carrier/helper_live_tests.rs`): a delivery
  sent before the page listens comes back after its hello, 20 more come back reversed and in
  order, nothing fails, and shutdown removes the data folder. The capability check passes.
- CI: the `carrier-live-linux` job runs the helper's clippy and both live suites under Xvfb with
  WebKitGTK on every PR; `carrier-live-windows` runs them with WebView2 on pushes to main and
  `cursor/` branches. Neither is a required check.
- Light and dark captures of the proxy list with the web proxy enabled (`ready-proxy-web-list`).
- Not run here: Linux and Windows run only in those CI jobs; Wayland is not covered by CI. No
  public web proxy was found to try, so a real relay page and real traffic are untested.

Earlier:
Unit tests for the address rules, link secret, validation, `proxyTypeWeb` both ways and web
links (`src/proxy_web.rs`), a driver test for the carrier refusal
(`src/connect/tests/proxy.rs`), a UI test of the form with the WEB type
(`src/ui/proxy/web_ui_tests.rs`), and light and dark captures of the form and the list from
`ready-proxy-web-form` and `ready-proxy-web-list`.
