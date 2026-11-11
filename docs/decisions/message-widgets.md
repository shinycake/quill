# Message widgets

TDLib 1.8.70 added `messageWidget`: a bot's small HTML document inside a
message, with its own script, data, storage and queries back to the bot.
Before this change Quill showed a card saying widgets don't run. Now they run
in the chat history on macOS, Windows and Linux.

## What Telegram Desktop does (in our words)

Telegram Desktop doesn't use a web view. It runs the widget's script in
QuickJS inside widget-sandbox, which checks the document and every call the
script makes, and draws the checked DOM with its own renderer (`tgw/` over
twidget's litehtml fork). Its behavior, as read from the source:

- A widget without a size of its own takes 160 px until the runtime reports
  a height; heights stay between 50 and 600 px. A widget that has a width
  is drawn at least 200 px wide and never wider than the bubble allows.
- At most eight widgets have a live runtime. Widgets on screen always get
  one; off screen, the one hidden longest is released first and keeps its
  last frame.
- Two fatal stops in a row and the widget is not started again until its
  content changes. A stopped widget shows its last frame with a centered
  "Widget stopped" pill.
- Links always ask first. Telegram links open in the app.
- `copyText` copies and shows "Copied: text". Haptics do nothing on a
  desktop.
- Storage: 4 KB per area and message, written a moment after the last
  change. sessionStorage lives while the app runs.
- Bot queries go through `getWidgetQueryAnswer`. Error 502 (or
  `WIDGET_RESPONSE_TIMEOUT`) means the bot timed out, a flood wait means
  rate limited, anything else failed. The widget gets that failure code.
- Public data is polled only while the widget is on screen and asked for
  it, every `poll_period` seconds as TDLib answers (30 s by default, kept
  between 5 s and an hour).
- A forwarded copy (`is_copy`) is not told which kind of chat it is in.
- The chat list and notifications say "Widget" (or the caption).

## Architecture

```
TDLib ──► WidgetContent (session) ──► WidgetsUi (per screen) ──► widget bubble
                                         ▲    │ commands
                       frames, events    │    ▼
                                     Runtime thread (one per live widget)
                                         │ qw_* C ABI, JSON
                                     native/widget/quill_widget.cpp
                                         │
                       WidgetHost + QuickJS (widget-sandbox) ─ TwidgetCore (litehtml)
                                         │ display list, canvas ops
                                     Rust painter (tiny-skia + cosmic-text)
```

- State (`src/message_widget`, `src/ui/widgets_state.rs`): every widget
  message seen has a `WidgetEntry` with an explicit `Lifecycle` (`Idle`,
  `Waiting`, `Loading`, `Running`, `Released`, `Stopped`, `Failed`). The
  bubble (`src/ui/widget_bubble.rs`) is a function of the entry: frame,
  placeholder, pill or fallback card. Nothing in the bubble keeps state of
  its own.
- Visibility (`src/ui/widgets_ui.rs`): each history pass notes the
  widgets its rows drew (per window), and once the pass is drawn the
  window's other widgets go off screen (hidden, told so, trimmed). Waiting
  widgets start under the live limit, releasing the oldest hidden one.
- Runtime thread (`src/ui/widget_runtime/thread.rs`): owns the native
  widget from create to destroy. It sleeps on its inbox and wakes only for a
  command or for the time the widget named (`next_tick_in`). Frames are
  painted only when the document changed, and only inside its damage
  rectangles. The thread hands the frame and events to the UI and pokes one
  signal. The UI never blocks on it.
- Frames: RGBA painted opaque over the bubble color, swapped to BGRA, held
  as an `OwnedImage` so a replaced frame leaves the GPU atlas. A new frame
  redraws only the history view that shows it.
- Animation: a widget that asks for frames (`requestAnimationFrame`, CSS
  animations) gets ticks from the history's frame clock
  (`request_animation_tick`) while it is on screen. Otherwise nothing ticks:
  idle CPU is zero.
- Input: pointer down/move/up and hover go to the runtime in CSS px, and
  the CSS cursor comes back as GPUI's. The wheel scrolls the history. A
  right-click opens the message menu, unless the widget claims the press for
  a menu of its own. After a press, the widget's selected text feeds the menu's
  Copy Selected Text.
- Host: theme params come from Quill's theme, flattened onto the bubble
  color (incoming or outgoing), and font size from Appearance. Locale comes
  from the language pack. Storage goes to `<data>/widgets/<account>/` (removed
  with the account). Links go through the link confirmation, or straight in
  for Telegram links. Queries and polls go through the driver.
- TON: Quill never builds wallet features. The glue never offers
  `TelegramWidget.wallet` (`context.wallet = false`, no wallet handler). A
  request that still asks for the wallet card fails in the glue. Quill's
  query type always has `with_wallet: false`.
- Safety: FFI is confined to `widget_runtime/ffi.rs`. Every string the
  C side returns is freed there, every unsafe block says why it is sound, and
  callbacks catch panics. No `unwrap`, `expect` or indexing in production
  code.

### Backends per OS

There is one painter on every OS, Quill's own Rust code over tiny-skia and
cosmic-text, so Linux gets exactly what macOS and Windows get. Generic font
families map to each OS's interface font: Helvetica Neue on macOS, Segoe UI
on Windows, then Cantarell, Noto Sans, DejaVu Sans or Liberation Sans on
Linux (`SANS_FONTS` and kin). Color emoji are drawn as images. The C++
side is portable: on macOS it links CoreFoundation (litehtml's date input),
on Windows bcrypt and the system ICU (Intl, Windows 10 1903+), on Linux libm
and pthread. build.rs compiles it optimized in every profile.

## Licenses

- QuickJS-ng (TelegramMessenger fork): MIT. twidget and its litehtml fork:
  BSD-3-Clause. gumbo-parser: Apache-2.0. The JS bundles contain linkedom
  (ISC), parse5 (MIT) and css-tree (MIT). All texts are in
  `licenses/message-widgets.txt`, and `THIRD_PARTY.md` has the row.
- widget-sandbox publishes no license. At v0.6.1 and on `main` the
  repository has no LICENSE file, its `package.json` is private with no
  license field, and GitHub reports none (checked 2026-11-09). The owner
  cleared shipping it as is on 2026-11-09. If upstream adds a license
  later, its text goes into `licenses/message-widgets.txt`.
- Telegram Desktop's `tgw/` (GPLv3) was read for behavior only. No code was
  copied, translated or paraphrased. The glue and the painter are Quill's own
  (MIT).

## Cost

Measured on an M-series Mac, 2026-11-09:

- Binary size (macOS arm64 release, `cargo build --release`, base 61e2d74fe
  against this branch): 97,454,192 → 106,951,216 bytes (+9.1 MiB, +9.7%).
  Stripped: 79,242,400 → 87,509,664 bytes (+7.9 MiB, +10.4%). Most of it is
  QuickJS, litehtml with gumbo, the sandbox and the JS bundles' bytecode.
- Build time: the C++ is compiled by Quill's build script, which ran 5.2 s
  before and 48 to 76 s after (two release runs, `--timings`). The script runs
  before the quill crate compiles, so on an incremental build that is added to
  wall time. The Rust crates' own times moved by less than the run-to-run
  noise on a machine shared with other builds (lib 221 s base, 155 s head;
  bin 803 s base, 550 s head; the base run was the busier one).
- Idle CPU with three widgets on screen (debug build, `ready-widgets` demo):
  0.0% once the widgets have drawn, about 5 s after launch.
- Cargo.lock: two lines. `cosmic-text` 0.19 and `tiny-skia` 0.12 were
  already in the tree and are now direct dependencies.

## How a widget sits in its bubble

The bubble is the widget's canvas, as Telegram's widget spec and Telegram
Desktop's HtmlWidget have it: no padding of its own, clipped by the bubble's
rounded corners, as wide as the message column allows. The widget's body
margin (`--tg-text-padding`, 8px 12px) is its only inset. When nothing sits
under the widget (no caption, no buttons, no reactions), the time goes on a
dark pill over the bottom right corner, as on photos, and the widget keeps
the whole bubble: Quill reports no `contentSafeArea` band. With a band, the
default styles pad the body by it, so a widget whose background sits on an
inner element left a strip of bare bubble under the pill (owner, 2026-11-10:
"widgets take up all of the message"). Telegram's own apps draw widgets to the
bottom edge with the time over them, and the spec already tells widgets to
put nothing tappable under the time. With a caption the time stays on the
caption's line. A reply or forward header keeps the bubble's padding above
the widget. The default `<button>` takes the app's primary color and radius
through the `--tg-button-*` tokens. Demo: `ready-widgets-showcase`.

## Windows: CRLF bundles (#1425)

The vendored JS bundles are hashed against their bytecode and carry a
version stamp on their first line. A Windows checkout with `core.autocrlf`
turned their line ends into CRLF, so every widget was refused, and the
refusal then aborted the app: the validator reported the refusal while it
was still being built, the host thread ran that report at once, and the
diagnostics lock it took was declared after the thread and did not exist
yet. `.gitattributes` keeps `third_party/widgets/**` byte for byte, the lock
is declared first (`native/patches/twidget-quill.patch`), and
`a_refused_bundle_stops_the_widget` re-runs the test binary with CRLF
bundles to prove the widget now stops with its pill. The `windows-widgets`
CI job boots every corpus widget and an Intl widget on Windows, under cdb
so a crash prints its native stack.

## Limits and what is left

- No wheel or keyboard inside a widget, by design: widget profile 1 gives a
  widget neither (twidget's `Renderer::wheel` and `Renderer::key` do
  nothing, see widget-sandbox `docs/profile-v1.md`), and Telegram Desktop
  sends neither. The wheel scrolls the history, as there.
- Not built, as in Telegram Desktop: WebGL and three.js widgets (twidget's
  3D backend is Apple-only) and copies of Chart.js or D3 served in place of
  CDN URLs.
- `dpr` is one scale for the app, the last window's, as Telegram Desktop's
  `style::DevicePixelRatio()`. A widget dragged between a 1x and a 2x
  screen keeps its first scale until it restarts.
- A chat window that closes takes its widgets off screen at once
  (`widgets_window_closed`), unless another window's last pass showed them;
  they move to that window.

## Bounds against a hostile widget

- Blur (CSS `filter`, box and drop shadows) stops at 200 device px
  (`effects::MAX_BLUR_SIGMA`): each pass costs the box width per line, and
  a bot's CSS can ask for any size.
- At most 16 off-screen widgets keep their entry and last frame
  (`KEEP_HIDDEN`); older ones are dropped and start over from their stored
  data. A permanently deleted widget message drops its storage file, as
  Telegram Desktop's `DeleteMessageStorage` does.
- The theme goes to a runtime only when it changed: each set re-sanitises
  the widget's CSS and rebuilds its layout.
- Font families a widget names are looked up in the installed set read once;
  nothing grows per name.
- Query answers carry Quill's own token, mapped to the asking runtime's;
  a restarted runtime never gets the answer meant for the one before.
- A logout drops what widgets still had to write, so nothing lands back in
  the deleted folder.

## Verification

- Unit tests: lifecycle, pool, failure mapping, poll periods, heights,
  widths and storage (`src/message_widget/tests.rs`); painter: boxes,
  clips, hidden groups, group opacity, damage, shadows, gradients, a damaged
  repaint matching a whole one, and a masked take
  (`src/ui/widget_runtime/paint_tests.rs`); fonts, images, geometry and
  effects.
- Driver tests with recorded JSON (`src/connect/tests/widgets.rs`): a query
  answer, 502/429/400 mapping, bad queries refused, one poll at a time with
  the period clamped.
- Native smoke tests (`src/ui/widget_runtime/smoke_tests.rs`): boot
  `tabs.html` and `canvas-bar-chart.html` from widget-sandbox's corpus
  through the real sandbox, renderer and painter, and find drawn pixels.
  They wait for the runtime's own "rendered" report, with no timers.
- `/verify` demo `ready-widgets` (tabs, a canvas bar chart in an outgoing
  bubble, a theme-following card), captured light and dark, and a click on
  the Details tab that switches it. The capture found two painter bugs,
  both fixed with tests. Rects without a key clipped everything away.
  tiny-skia's masked Source/Clear blends changed pixels outside the mask,
  which broke damage repaints and canvas `clearRect` under a clip.
- Windows: the `windows-widgets` CI job runs the painter tests and boots
  every corpus widget plus an Intl widget (system ICU). Linux is untested:
  the libm/pthread link and fontconfig picks. Also not run: a live widget from a real
  bot (queries, polls, `is_copy`), the CPU-budget stop on a real runaway
  widget, and chat-window edge cases.
