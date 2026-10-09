# Wasm live demo spike (codex/wasm-demo-spike)

Question: can Quill's real UI, fed by the demo fixtures, run in a browser via
WebAssembly for an interactive demo on the static product page?

## Verdict

The platform side is feasible today. gpui-pre 0.3.7 ships a browser backend
(`gpui-pre-web`, wgpu on WebGPU with automatic WebGL2 fallback), and gpui-kit
0.7.0 / gpui-component 0.7.0 compile for `wasm32-unknown-unknown` unchanged.
The blocker is Quill itself: the UI lives in the `quill` binary crate
(113k lines under `src/ui`, 156k more in the lib), and core types are entangled
with native pieces. Running the *real* UI in the browser is roughly 2-3 weeks of
refactoring, not a feature flag. A cheaper path that still delivers "the real
GPUI renderer and components, moving by itself": a dedicated `web-demo` crate
that shares Quill's design tokens and a few extracted view modules.

## What was built

`web-demo/` (own workspace, excluded from the root workspace so the native
build and gate are unaffected): a Quill-like chat list + conversation built from
gpui elements, bundled font, autoplay tour with a drawn cursor. Build with
`web-demo/build.sh`, serve `web-demo/www` statically.

Verified in a Chromium-based browser (WebGPU): renders, hover and click states
work, no console errors after the font fix. `screenshots/` holds two captures.

### Platform facts (from reading the crates)

- Entry: `#[wasm_bindgen(start)] fn start()` calling `gpui_kit::platform::web_init()`
  then `gpui_kit::application().run(..)` (same API as native). The canvas is
  created by gpui and appended to `<body>`, filling the viewport. One window only.
- Graphics: WebGPU preferred, WebGL2 fallback automatic
  (`WebBackendPreference::Auto`; `application_with_web_backend` to force).
- Threads: `gpui_platform::application()` on wasm requests worker threads but
  `WebDispatcher` checks `SharedArrayBuffer` + `Atomics.waitAsync` and falls back
  to a single-threaded dispatcher with a warning. No COOP/COEP headers are needed
  (GitHub Pages works); the demo ran on the single-threaded path.
- Fonts: the web platform starts with an empty font DB. Fonts must be added with
  `cx.text_system().add_fonts(..)` before `gpui_kit::init`, and the font family
  list gpui-component resolves (`.SystemUIFont`, then Helvetica, Segoe UI, Ubuntu,
  Adwaita Sans, Cantarell, Noto Sans, DejaVu Sans, Arial) must hit a bundled
  family or startup panics. The spike bundles Noto Sans Regular (310 KB).
  Quill uses system fonts on desktop, so a web build needs a bundled set.
- Build needs `RUSTC_BOOTSTRAP=1` on stable: `gpui-pre-web`'s default
  `multithreaded` feature pulls `wasm_thread`, which uses
  `feature(stdarch_wasm_atomic_wait)`. gpui-kit enables gpui_web with default
  features, so it cannot be turned off from outside. Fix: a `[patch]` for
  `gpui-pre-web` (Quill already patches gpui-base) with `default = []`, or a
  nightly toolchain for the web build only.
- `getrandom` 0.2 needs the `js` feature on wasm; 0.3 needs
  `--cfg getrandom_backend="wasm_js"` (set in `web-demo/.cargo/config.toml`).
- `wasm-bindgen-cli` must match the crate (0.2.129 used).

### Size and load

| artifact | raw | gzip -9 | brotli -q11 |
| --- | --- | --- | --- |
| spike wasm (release, opt-level s, fat LTO) | 14.9 MB | 4.8 MB | 3.2 MB |
| after `wasm-opt -Oz` | 11.2 MB | 4.3 MB | 3.1 MB |

Instantiation took 82 ms (localhost, cached); wasm download dominates in
production. GitHub Pages serves gzip (not brotli), so budget about 4.5 MB for
the spike; the full Quill UI would plausibly reach 25-40 MB raw / 8-12 MB gzip
(113k UI lines, image decoders, unicode tables, spellbook, emoji catalog).
The size is mostly wgpu + naga + cosmic-text + gpui itself, a fixed cost.

### Browser requirements

WebGPU: Chrome/Edge 113+ (desktop, Android), Safari 26+ (macOS/iOS 26),
Firefox 141+ on Windows and recent versions elsewhere is rolling out; verified
only in a Chromium build here. gpui's automatic WebGL2 fallback covers older
browsers but was not exercised (not tested: Safari, Firefox, WebGL2 path).
Treat browser version claims other than Chromium as unverified.

## Autoplay tour (done in the spike)

`web-demo/src/lib.rs`:

- A spawned task drives scripted steps (pause, glide-and-click). Events are
  injected with `AnyWindowHandle::update(cx, |_, window, cx| window.dispatch_event(PlatformInput::MouseMove|MouseDown|MouseUp, cx))`,
  the same mechanism as `QUILL_DEMO_CLICK` (untyped handle, so the root view is
  not leased while handlers run).
- Gliding uses cubic ease-in-out at 16 ms ticks, injecting a MouseMove per tick
  so hover styles follow the cursor.
- The synthetic cursor is an absolutely positioned div in the root view, drawn
  after all other children, with a smaller "pressed" state.
- Takeover: injected events never reach the DOM, so `pointermove`,
  `pointerdown`, `wheel`, `keydown`, `touchstart` listeners on `document` can
  only be the visitor. Any of them stops the tour at the next tick and hides the
  cursor. After 9 s without input the tour resets state and resumes.
- Pointer events matter: some automation only emits `pointerdown`, never
  `mousedown`.

For the real UI this maps directly onto `QUILL_DEMO_CLICK` steps (`p:`, `m:`,
`x,y`, `k:`, `w:`) reused as the tour script format.

## Blockers for the real UI

Measured by `cargo check --target wasm32-unknown-unknown --lib --no-default-features`
after gating the obvious native deps (about 130 errors in the first pass; rustc
stops before later phases, so expect more).

| blocker | where | effort |
| --- | --- | --- |
| UI is in the bin crate | `src/main.rs` `mod ui` | 2-3 days: move `ui` into the lib (or a `quill-ui` crate) |
| TDLib FFI | `telegram::ffi` (libloading), `connect::*` imports | 2 days: split pure types from loader, cfg native out, stub `TdJson` |
| ntgcalls | `calls::engine::{native,ntgcalls,types}` and `ntgcalls-sys`; `state` reducer imports `RemoteVideoState`/`TransportState` from there | 1 day: move plain types to `calls::engine::api` |
| interprocess | `single_instance` | 0.5 day: cfg |
| `directories`, paths, `std::fs` | many (`media_viewer`, settings, drafts, downloads) | 1-2 days: cfg or in-memory backend |
| `std::time::Instant` / `SystemTime` | 138 `Instant::now()` sites, 68 files with thread/time use. Panics on wasm. | 1-2 days: swap to `web_time` re-export |
| `std::thread`, blocking channels | connect poll loop, video decode, spellcheck | 2 days: cfg out in demo build |
| rodio / cpal / opus / ogg | audio, voice | 1 day: not behind a target-only gate today (the `ui` feature enables them all) |
| tray-icon, ksni, zbus, objc2-*, security-framework | `ui` feature deps | 1 day: new `web-demo` feature that excludes them, plus cfg on call sites |
| ureq (HTTP) | updater, translate | 0.5 day |
| system fonts / emoji / IME | text rendering | 1 day: bundle Noto Sans + emoji font (adds 2-10 MB) |
| macOS/Windows window-handle code (raw-window-handle, objc2) | window chrome | 1 day: cfg |
| fixtures with images | `ready-showcase` uses generated/decoded images | 1 day: `include_bytes!`, `image` png/webp already pure Rust |

Total: roughly 15-20 working days, with the risk concentrated in the monolith
refactor and a long tail of `cfg` call sites. That is past the one-week
threshold, which is why the spike stops at the platform proof.

## Recommended plan

1. Ship the product page with video/screenshot capture first (already done).
2. Phase A (about 1 week): promote `web-demo` into a `quill-web` crate that
   reuses Quill's color tokens and extracts pure view modules (chat row, bubble,
   composer, showcase fixtures) into a `quill-ui-core` crate that has no native
   dependencies. This renders real gpui elements with the real look and keeps the
   tour. Add the `[patch]` for `gpui-pre-web` so the build works on stable.
3. Phase B (optional, 2-3 weeks): grow Phase A until `ready-showcase` itself runs
   in-browser by moving more of `src/ui` into the shared crate.
4. Page integration: lazy-load the wasm only when the demo section scrolls into
   view, show the existing screenshot as poster, use `navigator.gpu` detection
   with the screenshot/video as fallback, gzip via Pages. Keep wasm under about
   6 MB gzip.

## Not verified

Safari, Firefox, WebGL2 fallback, mobile touch, IME, real Quill fixtures, and
load time over a real network.
