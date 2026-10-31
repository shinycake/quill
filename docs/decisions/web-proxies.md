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
- **Each host's files stay in a folder of its own.** An account's carriers live under
  `accounts/<key>/webview-carrier/`. Each host (one per client run of the account) claims a new
  generation there, `<pid>-<n>`, and the helper gets that folder. WebView2 keeps its profile in
  `webview2/` there (without one it would write next to the executable, which is read-only in an
  installed app). The macOS content rules and the WebKitGTK filter are compiled into `rules/` and
  `filters/` and removed once loaded. All the carriers of one host share WebView2's browser
  process, because WebView2 shares it per data folder. They still get one renderer each.
- **A generation is in use while its lock is held** (`src/proxy_carrier/helper/data_folder.rs`).
  The host holds a lock on `<pid>-<n>.lock` (`File::try_lock`) from the moment it claims the folder
  until its teardown has deleted it. The lock file is created with `create_new`, so a name left by
  an earlier process with the same pid is never reused. Only two things delete a generation:
  - its own host's teardown, once its helpers are gone;
  - the sweep a new host starts, which deletes every generation whose lock it can take, any lock
    file whose folder went, and anything else in there. A lock held by another host, in this
    process or another Quill on the same account root, keeps its folder.

  A sweep only opens lock files that exist; it never creates one. So a claim that holds its lock
  and still finds the file knows the file is its own, and a folder without a lock file is never in
  use (a claim makes the lock first, a teardown deletes it last). A host whose claim fails (the
  account folder cannot be written) tries again on a later `open`, at most every 10 s
  (`CLAIM_BACKOFF`), and until it has a folder it opens no carrier (`Terminated`, so TDLib may
  retry): no helper ever runs on WebView2's default folder next to the executable.

  Only `WouldBlock` from `try_lock` means another holder. Any other lock error (a filesystem that
  cannot lock: some NFS, SMB or FUSE mounts) ends the claim at once: it deletes the lock file it
  just made and tries no other name, so nothing piles up (`a_claim_that_cannot_lock_leaves_nothing`).
  A sweep that meets such an error cannot tell whether the lock is held, so it takes the safe side:
  it deletes the lock file only when its folder is already gone, keeps a folder next to one, and
  says so on stderr once per process (`a_sweep_that_cannot_lock_keeps_folders`). On NFS without
  `local_lock=flock`, `flock` becomes a per-process byte-range lock, which another open in the same
  process would get; local filesystems (APFS, ext4, NTFS) are fine.

  So a teardown that runs late, after the account's next client has started, never touches the
  new host's folder (`a_late_teardown_leaves_the_next_hosts_folder_alone`). Before this, both hosts
  used `webview-carrier` itself. The old teardown could delete, or on Windows rename away, the
  profile a new host's WebView2 was running on.
- **The folder goes when the client does, never on the UI thread.** It holds the cookies the pages
  set, so it should not outlive the account's client. On Windows, WebView2's browser process
  outlives the helper and holds the whole profile (`lockfile`, `Local State`, `History`,
  `Login Data`, caches) until it exits. On the `carrier-live-windows` runner that took 173 to
  822 ms after the web view was dropped, and more than 1.3 s once (run 39712613845), so a delete
  right after the helper exits can fail. In order:
  - Logout, quit or a closed client takes the host from the driver and runs its shutdown once,
    on a `quill-carrier-teardown` thread. The driver runs on the UI thread, so nothing there
    waits or deletes files. The session then says carriers cannot be hosted, so a later `open` is
    refused in one place rather than failing as `Unsupported` per carrier. If the thread cannot
    start, the host is dropped: its helpers are killed and its folder is deleted on a background
    thread.
  - Shutdown sends every running helper `close` with `last`, waits for the helpers still exiting
    (`SHUTDOWN_WAIT`, 3.5 s at most in all), then deletes its generation, retrying for 2 s.
  - A closing helper on Windows drops its web view and its environment. If Quill said this was
    the host's last carrier (`close` with `last`), it then waits up to `BROWSER_EXIT_WAIT` (2 s)
    for the browser process before it exits. Closing one carrier while another is open does not
    wait: the other's view keeps the shared browser process alive anyway, so the wait could only
    run out.
  - The close deadlines are defined together, with const asserts on their order, in
    `crates/webview-protocol/src/carrier.rs`: the page's notice (`PAGE_CLOSE_NOTICE`, 200 ms) plus
    the view's teardown (`VIEW_TEARDOWN`, 500 ms) plus the browser wait (`BROWSER_EXIT_WAIT`, 2 s)
    is less than the helper's hangup grace after a close (`CLOSED_HANGUP_GRACE`, 2.75 s), which
    is less than Quill's kill of a closed helper (`CLOSED_HELPER_DEADLINE`, 3.5 s). Shutdown waits
    the same 3.5 s, since every helper it closes is told it is the last.
  - Quill always hangs up (closes the helper's input) right after `close`. A hangup ends the
    helper 300 ms later, so a stuck loop cannot keep it alive, but after a `close` that grace is
    `CLOSED_HANGUP_GRACE`. Before this, the
    300 ms grace cut the browser wait short whenever Quill, not a test, ran the helper: the
    Quill-host live test on Windows never logged the wait, and at b8b8d837 it left the profile
    behind (run 39719957861).
  - What a teardown could not delete, and whatever a killed run left, the next host's sweep
    deletes on a thread of its own.
  - With `QUILL_WEBVIEW_DEBUG` set, a folder that survives its teardown is listed (up to 50
    relative paths with sizes). The helper logs whether the browser process exited and when: in
    the helper's live tests on Windows (runs of 70f176f and b8b8d837) every wait ended with
    `WAIT_OBJECT_0` (exited), after 156 ms to 1.2 s.
    Quit can end the process before the teardown thread is done; the next start sweeps the rest.
- **Locked down before the page runs.** The page is loaded only after the platform lockdown has
  succeeded, and the helper says `ready` only then:
  - Script dialogs and printing: `alert`, `confirm`, `prompt`, `print` and `onbeforeunload` are
    replaced with non-writable no-ops in every frame before any page script runs. Natively,
    WebKitGTK's `script-dialog` and `print` signals are handled and dropped, and WebView2 has
    its default script dialogs (`AreDefaultScriptDialogsEnabled`), context menus, dev tools, status
    bar and browser accelerator keys turned off. A live test checks that all four calls return
    at once and that the page keeps running.
  - The safe mode tdesktop uses: `RTCPeerConnection` (and its prefixed and helper classes),
    `WebTransport` and `WebAssembly` are removed. The bridge script does it in every frame at
    document start (wry's `with_initialization_script_for_main_only(…, false)`).
  - Frames are not refused, because no engine lets the host refuse one: `frame-src 'none'`
    keeps every other origin out, but `about:blank` and `srcdoc` frames still get a realm of
    their own, and a page reaches it without any getter Quill could wrap (`window[0]`,
    `frames.name`, `event.source`). So the guarantee is the engines' own injection: all three
    run the bridge in every frame, the empty document a new frame starts with included, before
    the page can touch it. `no_webrtc_webtransport_or_webassembly_in_frames_or_workers` checks,
    synchronously after each frame appears, frames reached by index, inside another frame, made
    by the parser (`innerHTML`), after a load and through `event.source`, and a sandboxed
    `srcdoc` frame (an opaque origin) reports on itself. It runs on WKWebView locally and on
    WebKitGTK and WebView2 in `carrier-live-linux` and `carrier-live-windows`. With the script
    injected into the main frame only, it fails at the first indexed frame. As a second layer
    the bridge strips a frame's window the first time the page reaches it through
    `contentWindow` or `contentDocument`.
  - Workers get the same: the page's `new Worker(url)`
    starts a small script of ours that removes them (and nested workers) and then loads the
    page's script with `importScripts` or `import()`. One difference a page could notice: the
    worker's `self.location` is a `blob:` address. The engine's own constructors are out of reach:
    the wrapper has a prototype of its own, and both its `constructor` and the original
    prototype's name the wrapper and cannot be changed. `no_route_to_a_worker_without_the_prelude`
    starts a page-made `blob:` worker through `Worker.prototype.constructor`, an instance's
    prototype and the original prototype; each runs the prelude. With the old wrapper, which
    shared the original prototype, `new Worker.prototype.constructor(url)` had `WebAssembly`.
  - No service workers. A service worker's fetches would be outside the page's policy. The
    bridge replaces `navigator.serviceWorker`, the `Navigator.prototype` getter behind it and
    `ServiceWorkerContainer.prototype.register` in every frame, and the getter in workers too.
    The content policy's `worker-src blob:` refuses any service worker on its own, because one
    needs an `http(s)` script. `no_service_worker_through_the_prototype` calls `register()`
    through the prototype's getter and checks it fails and the script is never fetched. On
    WKWebView, each layer alone keeps the worker out; with both off it registers. WebView2's
    request filter also refuses any request marked `Service-Worker: script`, wherever it points.
    On WebView2 runtimes before 1.0.2210 the filter does not see worker requests, so there the
    header check does nothing and the policy and the bridge are the only layers.
  - Natively, WebRTC
    is off in WebKitGTK's settings. WebView2 gets `--js-flags=--noexpose-wasm` (no WebAssembly in
    any frame or worker), `--disable-blink-features=WebTransport`, and
    `--force-webrtc-ip-handling-policy=disable_non_proxied_udp` (no UDP around the proxy, should a
    page reach WebRTC anyway). WKWebView has no public switch for any of these. A live test runs
    tdesktop's restrictions probe (`web_proxy_webview.cpp`, `RestrictionsProbeScript`) on the
    page and a fresh frame, then checks the frames above, a worker and service workers.
  - The path prefix is enforced natively, not only by the Content-Security-Policy. On macOS a
    `WKContentRuleList` blocks every load except the carrier's origin under its prefix (and its
    `wss:` form, `data:` and its own `blob:`). WebKitGTK gets the same rules as a
    `WebKitUserContentFilter`. WebView2 answers every request outside the scope with a 403 from a
    `WebResourceRequested` handler. A top-level page that ends up outside the scope (after a
    redirect, say) fails the carrier as `LoadFailed`.
  - WebView2's handler refuses a request whose address it cannot read, so an error there fails
    closed. On WebView2 older than 1.0.2210 the handler cannot cover worker requests.
  - WebView2 does not run a navigation's own request through that handler, and cancelling the
    navigation does not keep the request off the network. Measured on the `carrier-live-windows`
    runner, with the helper's debug trace on:
    - wry cancelled `location.href = '/other/'` in `NavigationStarting`, and the test server still
      got `GET /other/` (run 39701724561).
    - Not cancelled, the navigation never reached the request handler either. The handler logged
      every other document, fetch and worker request of the run, and nothing for `/other/`; the
      server got the request again (run 39708367632). WebView2's `put_Cancel` docs allow this: a
      `GET` may go out while the host is answering, and its answer may be used for the
      navigation.
  - So the bridge cancels navigations in the page itself, before the engine starts them. It adds
    the first `navigate` listener to each window's `navigation` it reaches and calls
    `preventDefault()` for any destination outside the scope (the rule of
    `CarrierScope::allows_navigation`, with blank documents allowed in frames only). It then sends
    `n` to the helper, which fails the carrier. The listener only uses natives taken before the
    page's script runs, so a page that swaps out `preventDefault`, the `String` methods,
    `RegExp.prototype.test` or `exec`, `URL`, `Function.prototype.call` or `addEventListener`, or
    stops propagation in its own listener, still cannot leave
    (`a_page_that_tampers_still_cannot_leave`, which leaves through `..%2F` under its own prefix).
    The encoded-separator check uses captured `indexOf` and `toLowerCase`, not a regular
    expression: `RegExp.prototype.test` reads `exec` from the regex at call time.
  - Behind the bridge, wry's handler cancels the page's navigations out of the scope and the
    lockdown's `FrameNavigationStarting` handler cancels its frames'. On WebView2 those cancels
    come too late to stop the request.
  - What is left on WebView2 is a navigation the page cannot cancel from script: the Navigation
    API makes a navigation started from a cross-origin document not cancelable. The only such
    document a page can make is an opaque-origin sandboxed `srcdoc` frame with
    `allow-top-navigation`, since `frame-src 'none'` keeps other origins out. For that one case the
    exposure is a single `GET` to an `http(s)` URL the page chose, sent from the helper's
    incognito profile. The answer is never shown or readable, because the navigation is
    cancelled and the current page stays. The carrier fails on the attempt. The page can use it
    to send data to a host of its choice, and that host learns the user's address, which the
    carrier's own server already knows. Cookies set by the answer stay in the helper's
    per-account profile.
  - What stays best effort:
    - WebView2's request handler does not see WebSocket or WebTransport connections, or a
      redirect that its network stack follows on its own. On Windows, `connect-src` in the CSP
      (and the top-level check, for redirects) is the only rule for those. A worker started from
      the prefix is outside the document's CSP, but it gets the removals above.
    - DNS prefetching of links in the page is not covered by any of the three platforms'
      filters.
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
  whose answers the driver drops. No page message waits in a queue of Quill's. The UI thread
  sees only TDLib's tiny `ok` for each one, dropped first thing in `ingest`, so a page that posts
  many small messages still costs one cheap envelope each on the receive path. The UI thread also
  hands TDLib's data to a bounded queue.
- **Backpressure and limits.** The helper hands the page one delivery at a time and waits for the
  page's acknowledgement (tdesktop does the same). It queues at most 1,024 deliveries or 16 MiB, a
  delivery may be at most 1 MiB plus slack (TDLib's own cap), and a page message at most 2 MiB
  binary or 64 KiB text (TDLib's text cap). Quill queues at most 1,024 commands or 16 MiB per
  carrier, then fails the carrier and kills its helper. Going over any of these, or a malformed
  line, fails the carrier as `Terminated`; nothing blocks the UI thread.
- **No helper outlives Quill.** The helper reads its commands on a thread of its own, started
  before the web view. When stdin ends (Quill quit or crashed) or a line is broken, that thread
  ends the process 300 ms later even if the main loop is stuck. On Linux the kernel also kills the
  helper when Quill dies (`PR_SET_PDEATHSIG`). The kernel ties that to the thread that started the
  helper, not the process, so `HelperCarrierHost::open` must run on a thread that lives as long as
  the carrier; today that is always the UI thread. On Quill's side the host keeps each child
  itself. Quitting, logging out, a closed client and dropping the host kill every child from the
  calling thread (the signal is sent there; only the reaping happens on a thread of its own). TDLib's close gives the page 1 s to receive `{"t":"close"}` before the kill. A watchdog
  on every poll and on every writer turn (so steady deliveries cannot hide it) kills a helper
  that has not said `ready` within 60 s or has sent nothing for 15 s since.
- **Hidden and idle.** The view is never shown, so the platforms would throttle it. macOS gets
  `with_background_throttling(Disabled)` and an `NSProcessInfo` activity that keeps App Nap
  away. WebView2 gets the flags that turn off occlusion and background timer throttling. WebKitGTK
  has no switch for this. The helper's own 3 s tick (the `alive` line, the page probe, the load
  and silence timeouts, and the end of the close grace) comes from a thread of its own through
  the event loop proxy, not from the loop's timer: tao's GTK loop treats `WaitUntil` like
  `Wait`, so on Linux an idle view never ticked, and Quill killed every idle carrier after 15 s.
  Only the page's answer can be late. The 130 s idle soak with the window hidden runs on macOS
  and in both CI jobs.
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
proxy disabled, and the row says "Web proxies aren't supported in this version yet". A web proxy
that was already in use when hosting turned off stays enabled (Quill does not change the user's
choice behind their back), but its row shows "not available" and the same note, so the reason
the connection is down is on screen next to the "Disable proxy" choice
(`ready-proxy-web-unhosted`).

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
  sink with the reserved `@extra`; data for an unknown carrier fails it; shutdown ends every child;
  a late teardown leaves the next host's folder alone; a host without a folder of its own opens
  no carrier until it can claim one. The data folder's own tests
  (`data_folder/tests.rs`): claims are new and empty, never a leftover's name; a sweep keeps held
  generations and removes the rest; a teardown leaves nothing. The driver's tests check the host
  shuts down once over many ticks of a closing client, and hosting is off after it.
  The close and shutdown tests once used `/bin/cat` as a helper that stays up. It does not:
  the host passes `--carrier`, cat rejects it and exits 1 about 2 ms after it starts, so those
  tests passed only when the close came first. Under CPU load (2 burner processes per core,
  a fresh copy of the test binary each run) they failed 1 run in 40, with
  `[(5, Terminated)]`, a real report of a helper that died. The stand-in is now a script that
  waits for the `close` command, as the real helper does. A stand-in that waited for the
  end of its input instead stayed up past 2 s after a close in 1 run of 100: on macOS, std
  creates a pipe and only then sets close-on-exec, and does not ask `posix_spawn` to close
  inherited descriptors. So a child started at the same moment can inherit another child's
  stdin and hold it open. The real helper exits on the command, not on the end of its input, and
  the host reaps it after `CLOSE_GRACE` either way. The stand-ins are checked-in scripts
  (`tests/fixtures/carrier-helper`) and the tests wait on the host's events, not on sleeps
  (`docs/decisions/carrier-helper-test-flake.md`).
- The real helper with WKWebView on macOS against a local relay page
  (`crates/webview-host/tests/carrier_live.rs`, ignored by default, run with
  `cargo test -p quill-webview --test carrier_live -- --ignored --test-threads=1`), 13 tests, all
  passing: bytes go both ways in order and the page hears `{"t":"close"}`; data sent before the
  page listens waits for it; `alert`, `confirm`, `prompt` and `print` return at once and WebRTC is
  gone, in frames too; tdesktop's restrictions probe passes on the page and a fresh frame, and
  frames reached by index, nesting, the parser, a load, `event.source` and a sandbox, and a
  worker, have none of the removed interfaces and no service workers; `register()` through the
  prototype's getter fails and its script is never fetched; leaving the scope by navigation or
  `about:blank` fails as `LoadFailed`; a
  redirect out of the prefix is blocked by the content rules; a 404 page and an unreachable page
  fail as `LoadFailed`; a page that hangs fails as `Terminated`; the helper exits when Quill hangs
  up; the probe starts the view and exits; a plain-`http` origin is refused before loading; and
  the 130 s idle soak. Debug builds accept a loopback `http://127.0.0.1` origin for these tests
  when `QUILL_WEBVIEW_CARRIER_LOOPBACK` is set; release builds never do.
- Quill's live host against the real helper (`src/proxy_carrier/helper_live_tests.rs`): a delivery
  sent before the page listens comes back after its hello, 20 more come back reversed and in
  order, nothing fails, and shutdown leaves the data folder empty. The capability check passes.
- CI: the `carrier-live-linux` job runs the helper's clippy and both live suites under Xvfb with
  WebKitGTK on every PR. `carrier-live-windows` runs them with WebView2 on pushes, and on PRs that
  change the carrier code (the helper, the protocol crate, `src/proxy_carrier` or the workflow).
  Neither is a required check. The first Linux run found the idle-tick bug above: the soak failed
  after 6 s without an `alive` line.
- Light and dark captures of the proxy list with the web proxy enabled (`ready-proxy-web-list`),
  and of the same list where no web view can run (`ready-proxy-web-unhosted`): the enabled row
  says "not available" and why.
- Not run here: Linux and Windows run only in those CI jobs; Wayland is not covered by CI. No
  public web proxy was found to try, so a real relay page and real traffic are untested.

Earlier:
Unit tests for the address rules, link secret, validation, `proxyTypeWeb` both ways and web
links (`src/proxy_web.rs`), a driver test for the carrier refusal
(`src/connect/tests/proxy.rs`), a UI test of the form with the WEB type
(`src/ui/proxy/web_ui_tests.rs`), and light and dark captures of the form and the list from
`ready-proxy-web-form` and `ready-proxy-web-list`.
