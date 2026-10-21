# gpui-kit 0.8.0 (GPUI snapshot still gpui-pre 0.3.9)

gpui-kit and gpui-base go from `=0.7.3` to `=0.8.0`. gpui-component,
gpui-component-macros and gpui-kit-assets move with them. The optional
gpui-fast crates go from 0.1.8 to 0.1.10 in the lockfile; Quill doesn't
enable the `gpui-fast` feature, so they are locked but not built (the same
goes for `gpui-pre-ios` and wgpu 30, which only the gpui-fast graph pulls in).
The default backend is still the gpui-pre 0.3.9 snapshot, so gpui-pre and
gpui-pre-{apple,macos,linux,windows,wgpu} were not re-vendored.

## What tdesktop does

Telegram Desktop's history and chat list are `Ui::ElasticScroll` areas.
`HistoryWidget` sets `OverscrollType::Real` at both ends
(`history_widget.cpp`), so a trackpad swipe past the newest or oldest
message stretches the history and springs back, as an `NSScrollView` does.
The dialogs list does the same, except that the top turns into a virtual
pull when stories are shown (`dialogs_widget.cpp`). Kit 0.8.0's new
default, a native macOS bounce on precise scrolling, is the same behavior
for the surfaces that use it.

## What changed

### Vendored crates

gpui-base and gpui-component were moved the way `gpui-kit-0-7-3.md`
describes: Quill's patch is the diff from the old pristine commit to main, a
pristine commit adds the 0.8.0 registry copy (without `.cargo-ok`,
`.cargo_vcs_info.json`, `Cargo.lock` or `Cargo.toml.orig`), and a second
commit three-way merges the patch onto it. Each crate's `QUILL-CHANGES.md`
names the new pristine commit.

| Crate | Pristine | What had to change |
|---|---|---|
| gpui-base 0.7.3 → 0.8.0 | `6d03a41e` | `input/base/element.rs`: 0.8.0 splits horizontal caret follow from a new resize-follow branch. The right-to-left caret margin (`_ if cursor_rtl => px(0.)`) moved inside it. `slider.rs`: 0.8.0 adds `Slider::accessibility_label` (#3494) on the element. Quill keeps `SliderState::label` as the fallback, so the element label wins when both are set and the eight labelled sliders didn't need touching. Everything else (bidi engine, formatting spans, inline tokens, tooltip hide, text style accessor) merged cleanly. |
| gpui-component 0.7.3 → 0.8.0 | `9f91f2c9` | `tab/tab_bar.rs`: kept. See below. `message_scroller.rs`: `top_anchor` and `scroll_to_item_offset` merged cleanly. 0.8.0's new `logical_scroll_top` reads the same row but ignores the follow state and has no setter, so it doesn't replace either. The installed-font prefetch in `theme/mono_font.rs` merged cleanly and is now listed in `QUILL-CHANGES.md`, which had missed it. |

The TabBar patch stays. 0.8.0's "sliding indicator appears after the first
frame" (#3574) records where render placed the indicator and asks for one
more frame when the selected tab was measured somewhere else. On a
bounds-only change it still retargets the same 280 ms spring, so an unread
badge on the selected folder would still spring the indicator and redraw the
sidebar every frame (`folder-tabs-no-badge-spring.md`). The two fixes work
together: #3574's extra frame is the one where Quill's jump counter bumps
the spring key. `TabVariant::Folder` (#3415) draws no sliding indicator, so
the patch doesn't reach it.

### A new gpui-pre patch: the list's scroll range counts its padding

With 0.8.0 on macOS, a trackpad swipe down the chat history stopped 16 px
short of the newest message. The list never resumed following its tail and
the jump-to-latest button stayed up. Main reached the end.

The cause is in gpui-pre's `ListState`. Its own wheel scroll and
`set_offset_from_scrollbar` clamp to `items + padding.top + padding.bottom
- viewport`, but `max_scroll_offset` (behind `max_offset_for_scrollbar`)
left the padding out. The two never met before, because the list scrolled
itself. Kit 0.8.0's `ScrollBounce` now owns vertical wheel scrolling and
moves the list through its `ScrollbarHandle`, clamping to
`content_size - viewport`, which is the smaller range. `MessageScroller`
pads its list with `py_2`, 8 px at each end.

`third_party/gpui-pre/src/elements/list.rs` now adds the padding in
`max_scroll_offset`, the same bound the list clamps to. The fix sits where
the two ranges disagree, so every padded `List`, not just the history, gets
the right range. The scrollbar thumb now spans the padded content too, which
moves it by a pixel or two. This is an upstream bug in the kit and the
GPUI snapshot together. It should go upstream once the owner says so.

### Scroll bounce, surface by surface

The default is accepted. What each Quill surface does now on macOS:

- **Chat history** (`MessageScroller`) bounces at both ends, as in
  tdesktop. Loading older messages is triggered when row 0 renders and the
  stretch is drawn over a clamped logical offset, so paging is unchanged.
  With the padding fix, a swipe to the end resumes tail following. New
  messages then stay anchored at the bottom, and the jump button hides. The
  pull to the next channel still owns overscroll at the bottom when there
  is a next channel. Its capture-phase listener consumes those wheel events
  before the bounce sees them (`PullMachine::wheel`), so the two never
  stack. With no next channel the history bounces. Horizontal-dominant
  gestures are ignored by the bounce, so horizontal strips inside the
  history still scroll.
- **Chat list** is a plain `v_virtual_list` and doesn't bounce, as on
  main. tdesktop's list does. That is a parity gap, not a regression.
- **Settings and most panels** are plain GPUI `overflow_y_scroll` areas
  and don't bounce. Nested scrolling there is GPUI's, and gpui-pre didn't
  change, so it behaves as on main.
- **Composer and other kit text areas** bounce when their text scrolls.
  A scroll that starts over a composer whose text can scroll stays in the
  composer at its end (#3484). This matches macOS text views.
- **Kit menus with `scrollable(true)`** (folder tab menu, gift resale and
  TON wallet menus) and Selects bounce. They have no nested areas.
- The bounce is off when the system asks for reduced motion
  (`cx.reduce_motion()`), and off on Linux and Windows.

No surface needed bounce turned off. MessageScroller has no `scroll_bounce`
builder, so turning it off would have meant another patch. The padding fix
made that unnecessary.

### Other breaking changes

None reached Quill's code. Quill doesn't match on `SelectionFormat` or
`TabVariant`. It doesn't build resize handles, use `DataTable`, or call
`set_selected_row`, `set_searchable` or `set_line_number`. Its animations
use Quill's own easing functions (`ui/motion.rs`) on the frame clock, not
the kit's `Easing::sample`. Everything compiled without changes.

### Not adopted

`ScrollableMask::vertical_wheel` (#3518) maps a mouse wheel to a horizontal
strip. tdesktop's strips don't do that. The WebView crate, syntect, the
gpui-shell runtime and the completion highlighters have no Quill use.

## How verified

- `bash quill-tools/gate.sh`: `GATE OK core=5525 0 ui=1574 0`, with
  fmt, both clippy runs (`-D warnings`), the language check and
  `scripts/third-party-licenses.sh --check` (regenerated).
- `ui::scroll_restore_tests::swiping_back_to_the_newest_message_resumes_following`
  swipes the real history up and then down past its end with phased
  trackpad events. It asserts the scroller follows its tail and isn't
  scrolled up. It fails without the gpui-pre fix ("the jump-to-latest
  button must go away at the end") and passes with it. On Linux and
  Windows the bounce is off, and the test checks the list's own scroll.
- Screenshots on an M1 Pro, debug `demo-capture` builds of this branch and
  of origin/main, 1200x800. Screens: `ready-chats` (chat list and open
  chat), `ready-settings-advanced`, `ready-privacy`, `ready-edit-delete`
  and the `ready-earn-link-box` dialog, in light and dark. On
  `ready-media`, scripted trackpad swipes captured the stretch at the top
  and bottom and the settled state after release. After a swipe to the end,
  main and this branch differ in 67 pixels, all in the scrollbar thumb.
  With `QUILL_DEMO_UPDATE_STREAM=4` after a swipe up and back, messages
  keep arriving at the bottom, as on main.
- Idle CPU (`scripts/idle-cpu-bench.sh`, 10 s settle, debug builds, one
  run each). This branch matches main within noise: active-idle 0.00 / 0.00,
  inactive-idle 0.00 / 0.00, active-stream10 7.43 / 8.12, typing
  6.08 / 6.67, stickers 12.20 / 12.25, gif 1.77 / 1.77 (%CPU, branch / main).

Not verified:

- Real trackpad gestures with momentum. The scripted swipes have phases but
  no momentum tail.
- Nested vertical scroll areas inside the history or a dialog. 0.8.0's
  "innermost area takes the whole gesture" wasn't exercised on any of them.
- The patched gpui-base and gpui-component unit tests, including the
  merged TabBar jump test. Neither crate is a workspace member. Quill's
  `folder_tabs_tests.rs` covers the jump through the real app.
- Linux and Windows at runtime. The bounce is off there, and the gpui-pre
  fix only widens a scrollbar range.
