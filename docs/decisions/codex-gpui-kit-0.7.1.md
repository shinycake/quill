# gpui-kit 0.7.1 (GPUI snapshot gpui-pre 0.3.8)

gpui-kit goes from `=0.7.0` to `=0.7.1`. Its GPUI snapshot crates move from
gpui-pre 0.3.7 to `=0.3.8`. Release notes:
`gh release view v0.7.1 --repo longbridge/gpui-component` (the repository is
now `longbridge/gpui-kit`).

## Vendored crates

Quill patches four crates through `[patch.crates-io]`. Each one was moved the
same way:

1. Take Quill's patch as the diff between the old pristine commit and main.
2. Commit the new registry copy as is, without `.cargo-ok`,
   `.cargo_vcs_info.json`, `Cargo.lock` or `Cargo.toml.orig`.
3. Three-way merge the patch onto it (old pristine as the base) in a separate
   commit, and fix what upstream moved.

Each crate's `QUILL-CHANGES.md` names its new pristine commit, so
`git diff <commit> -- third_party/<crate>` still shows the whole patch.

| Crate | Pristine | What had to change |
|---|---|---|
| gpui-base 0.7.0 → 0.7.1 | `73f33094` | `element.rs`: 0.7.1 adds the horizontal scroll offset to every caret after layout and clamps right-aligned carets inside that loop. Quill's rule that a caret on a right-to-left paragraph clamps the same way now goes through a per-caret `rtl` flag on `CursorRenderInfo`. `Cargo.toml`: 0.7.1's new `unicode-linebreak` dependency sits next to Quill's `unicode-bidi` and `unicode-bidi-mirroring`. Every other file merged cleanly. |
| gpui-pre-macos 0.3.7 → 0.3.8 | `126460ec` | `step()`: 0.3.8 hands the display link's frame-signal timestamp to the frame callback. Quill keeps that and runs its idle check afterwards, which still stops the link after 250 ms with nothing to draw. `WindowFrameSource::is_running` and `requests` sit next to the new `take_signal`. |
| gpui-pre-linux 0.3.7 → 0.3.8 | `d2e24766` | X11 was refactored: the calloop loop no longer carries the client (its data is `()`), and callbacks reach the client through a weak `X11ClientStatePtr`. The frame waker's ping source now does the same. The new `X11Connection` `Drop` removes that ping source along with the refresh timers. |
| gpui-pre-windows 0.3.7 → 0.3.8 | `d2e24766` | The window list now holds `TrackedWindow` (a handle plus its frame signal), and the vsync thread has a stop flag. `FrameGate::wait_for_frames` takes any list entry type. The vsync loop copies the parked set before reading the list, so the lock order doesn't change. It records the frame signal only for windows it actually invalidates and still checks the stop flag. |

Upstream fixed none of what Quill patches. The macOS display link, the X11
refresh timer and the Windows vsync invalidation still run at the refresh
rate for as long as a window is visible, and the input engine still has no
bidi support. All four patches stay.

The other gpui-pre sub-crates Quill links (`gpui-pre`, `-apple`,
`-platform`, `-wgpu` and the small utility crates) are unpatched and come
from the registry. 0.3.8 moves the macOS dispatcher into `gpui-pre-apple`,
but Quill's patch doesn't touch the dispatcher, so nothing else needs
vendoring. The X11/Windows idle-frame code (`frame_idle.rs`) didn't change,
and the two copies are still identical.

## API changes in Quill

- The continuation-indent change gives `LineWrapper::wrap_line` an
  `IndentAdjustment` argument. Only the kit calls it (`text/inline_flow.rs`,
  `SameIndent`). Quill's text goes through GPUI's line layout, which doesn't
  use `wrap_line`, so Quill needed no change. Message bubbles, the composer
  and the right-to-left composer capture pixel-identical before and after.
- Built-in kit charts now draw their data in on first paint. Quill
  draws no kit charts: channel statistics render TDLib's graph JSON as text
  sparklines (`ui/statistics.rs`). tdesktop's statistics charts animate on
  zoom, filter and the details popup, but they don't draw in when shown. If
  Quill moves statistics to kit charts, use `.appear(false)`.
- For the interface zoom, 0.3.8 adds four `Platform` methods with default
  bodies:
  `set_initial_windowing`, `request_windowing`, `set_activation_policy` and
  `graphical_environment`. `ZoomPlatform` (from main, PR #560) compiled
  without them, which would have hidden the real platform's versions.
  `forwards_every_trait_method` caught all four, and they now pass straight
  through. `PlatformWindow` gained nothing.
- Everything else compiled unchanged, including main's newer code.

## Free fixes

These need no Quill code:

- The IME candidate window is placed correctly before the first repaint,
  and IME rewrites of a typed character (Korean among others) are one undo
  step. The composer is a kit `Textarea`, so it gets both.
- Textarea fixes cover row-based height, clipping in short single-line
  frames, and soft wrap agreeing with horizontal scroll. Quill's textareas are all
  `auto_grow(min, max)` or single-line, and none had a workaround to remove.
  For auto-grow the min-height formula is the same as before.
- Mask validation counts characters instead of bytes. Quill uses no
  input masks.
- TextView now follows the container's text color in filled bubbles
  (#3329). Quill doesn't use the kit's `TextView`. Bubble text is Quill's own styled text with explicit colors, so
  there was no workaround to remove.
- Borderless (ghost) buttons now show a keyboard focus line, and
  popup menus keep a single keyboard highlight. Both apply across Quill's
  ghost icon buttons and menus.
- A phaseless mouse wheel scrolls again after bouncing at an edge. This applies to the message list, which is the kit's
  `MessageScroller`.
- Checkbox labels and button text follow the Input/Select size ladder
  (#3287), so they are a step smaller in places. You can see
  it in the folder editor's "Include types" checkboxes and in the Join
  channel and Refresh buttons. This is the kit's intended sizing and it
  follows the Design Guides, so Quill doesn't override it.

## New features

### Adopted: custom accent color (ColorSelect)

tdesktop's accent row in Chat Settings ends with a custom circle that opens a
free-form HSL editor (`ColorsPalette::selectCustom`). The editor limits
lightness per theme (`ColorizerFrom`): at most 160/255 on day themes and at
least 64/255 on night themes. Quill had seven presets and Default.

Appearance → Accent color now has a Custom field, a `ColorSelect` with
the presets featured at the top of its palette. A committed color gets the
same lightness limit, is shown limited in the field, and becomes
`accent_rgb`. Pure black is stored as `0x000001`, because 0 means the theme
default. Unit tests cover the limits and the RGB round trip.

ColorSelect doesn't fit anywhere else. Folder tag colors, name colors and
profile colors are fixed palettes in Telegram, and Quill already shows them
that way.

### Not adopted: speech input

The kit's `speech` feature does on-device dictation: `SFSpeechRecognizer` on
macOS, `Windows.Media.SpeechRecognition` on Windows, and on Linux an
application-supplied recognizer. tdesktop has no composer dictation. Its
voice-to-text is server-side transcription of received voice messages
(premium). Quill's `src/voice_input.rs` isn't dictation either: it is the
cpal microphone capture for voice notes. Turning the feature on would add
`objc2-speech`, `objc2-avf-audio` and new `windows` features, and on macOS it
would need a speech-recognition usage string and a new permission prompt.
Linux would still have nothing without a recognizer. Left out.

### Not adopted: inline-token hover

`on_token_hover` reports pointer entry and exit over kit inline tokens.
Quill's composer doesn't use inline tokens: mentions and custom emoji live in
Quill's own entity table over plain text. tdesktop's input field shows no
tooltip for them either. Revisit if the composer moves custom emoji to kit
tokens.

## Follow-ups

- The IME candidate window is still misplaced under interface zoom on
  macOS and Windows. At scales above 100% the candidate window opens toward the window's top-left corner
  (see `codex-interface-scale-full.md`). It can't be one multiply in
  gpui-base's `bounds_for_range`. GPUI also calls that method for
  `selected_bounds` → `update_ime_position`, which the zoom decorator already
  scales, so Linux and the Windows caret would get scaled twice. The places
  to fix are the two platform paths that call `bounds_for_range` directly:
  macOS `first_rect_for_character_range` (gpui-pre-macos `window.rs`) and
  the Windows candidate position (gpui-pre-windows `events.rs`). Both are
  vendored now. They need the window's zoom, for example from a small setter
  the decorator calls.
- Kit charts for channel statistics, with `.appear(false)`. That would
  replace the text sparklines and match tdesktop's chart widget.
- `DockArea::set_split_sizes`, scrollable code blocks, Markdown
  `range_for_source` and the Questionnaire change: Quill uses none of these
  components today.
- `third_party/gpui-base/src/input/bidi_paragraph.rs` has an unused
  `wrap_width` field warning. It predates this upgrade and was left alone.

## Verification

- Gate (`quill-tools/gate.sh`): fmt, core clippy, core and UI tests, run
  before the code commits and again after merging main. The last run printed
  `GATE OK core=2585 0 ui=210 0`.
- `forwards_every_trait_method` and the rest of the interface-zoom tests
  pass against 0.3.8.
- Cross `cargo check` from macOS of the patched `gpui-pre-windows`
  (`x86_64-pc-windows-msvc`) and `gpui-pre-linux` (X11 + Wayland,
  `x86_64-unknown-linux-gnu`), with no warnings. CI's package jobs build the
  full Linux and Windows UI.
- `scripts/test-frame-idle.sh`: 9 tests pass, and the two copies are
  identical.
- `cargo deny check licenses` passes. `THIRD_PARTY_LICENSES.md` was
  regenerated: borsh, filetime and x11-clipboard are gone, and notify is now
  8.2. `THIRD_PARTY.md` names the new versions.
- Demo captures (`QUILL_DEMO_CAPTURE`, 1200x800). Debug builds of the branch
  base and of this branch were compared pixel by pixel:
  - identical: `ready-chats` (chat list, bubbles, composer),
    `ready-chats-composer` and `ready-rtl-composer`;
  - `ready-message-menu`: only the fixture's clock differs;
  - `ready-channel-stats` and `ready-folders-tag-color`: control text a step
    smaller (#3287, above);
  - `ready-appearance`: the new Custom row.

### Idle CPU (internal numbers)

This uses the same check as `codex-idle-cpu.md`: a reduced
`scripts/idle-cpu-bench.sh` (four scenarios, 15 s settle, mean of 30
one-second `top` samples) on release builds of the branch base and of this
branch, run alternately on this M1 Pro. %CPU, run 1 / run 2:

| Scenario | 0.7.0 / 0.3.7 | 0.7.1 / 0.3.8 |
|---|---|---|
| active, idle | 0.00 / 0.20 | 0.00 / 0.00 |
| inactive, idle | 0.00 / 0.00 | 0.00 / 0.00 |
| active, 10 updates/s | 2.22 / 2.84 | 2.47 / 1.99 |
| active, typing dots | 2.51 / 3.35 | 3.13 / 3.34 |

An idle window still reads 0.00%. With the display link left running it was
0.4 to 0.6%, so the patched link still pauses. The other rows are within
run-to-run noise. The release binary grew from 48,765,232 to 48,934,528
bytes (+0.35%); that build doesn't include the accent field.
