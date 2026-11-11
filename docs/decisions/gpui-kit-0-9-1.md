# gpui-kit 0.9.1

gpui-kit moves from 0.9.0 to 0.9.1, which takes GPUI to the gpui-pre 0.3.12
snapshot. That unblocked the 0.3.12 re-vendor prepared on 2026-11-09
(`docs/decisions/gpui-pre-0-3-12.md`, branch `claude/gpui-pre-0-3-12`), which is
rebased onto main here. The round also takes the two compatible crate updates
from #1418 and adopts one new GPUI API.

## What tdesktop does

For the submenu change: `PopupMenu::prepareGeometryFor` (lib_ui
`ui/widgets/popup_menu.cpp`) opens a child menu on the right of its parent and,
when the right side has no room on the screen, on the parent's left. Near the
bottom it moves up to fit. Everything else in this round is Quill's own
dependency policy (`docs/dependency-updates.md`).

## What changed upstream that reaches Quill

Kit 0.9.1 (release notes: longbridge/gpui-kit v0.9.1). Fixes Quill gets
without code of its own:

- Input: the platform hears when the caret moves (the IME panel follows it),
  the caret keeps its visual line at a soft wrap, and decoration styles compose
  in a fixed order instead of hash order.
- Scrolling: a smooth-scrolling wheel stops at the edge on macOS, a horizontal
  swipe over an area that fits goes to the parent, and the overscroll stream
  survives sideways packets.
- Menus open a scrollable menu on its checked item and size items in rems.
- Slider: dragged values snap to steps counted from `min`, and assistive
  technology steps one thumb.
- Motion: an interrupted transition reverses as CSS does.
- `rustls-webpki` and `webpki-root-certs` updates for CVE-2026-111943.

GPUI 0.3.12: see the 0.3.12 decision (Windows colour glyphs on the CPU,
accessed-entity scopes that skip released entities, cursor resolved after the
mouse listeners, list insets, `Anchored::attach`, animated WebP).

## What Quill adopted

`src/ui/menu_surface.rs` `submenu_panel` (the Mute submenu of the chat-row and
community-row menus) uses `Anchored::attach` with the parent menu's edges as
the target. It opens to the right of the menu and flips to the left when the
right does not fit, as tdesktop does; before, it snapped back inside the window
on top of its own menu. The panel is now measured from the row's corner (an
absolute wrapper) rather than from where the row's padding put the next child,
so it meets the menu's edge exactly (it sat 13 px off) and its first row levels
with the parent row. Tests: `chat_menu_submenus_tests.rs`
`the_mute_submenu_opens_right_of_the_menu_where_it_fits` and
`the_mute_submenu_moves_left_of_the_menu_near_the_right_edge`.

## What Quill did not adopt, and why

- `TransitionState` (motion that outlives its element): Quill's animations run
  on its own frame clock (`src/ui/frame_clock.rs`, `src/ui/motion.rs`) with
  manual clocks in tests; the kit's transitions read the executor clock.
  Nothing in Quill keeps keyed motion state that a virtualized row loses.
- `Dialog::accessibility_label` is on `gpui_base::dialog::Dialog`; Quill's
  dialogs are the `gpui_component` wrapper, which does not pass it through.
- `Button::selected_accent`, `Slider::track_focus` and `Toolbar::axis` are in
  the release notes but not in the published 0.9.1 crates.
- GridLayout, `ColumnFixed::Right`, Markdown task checkboxes, the PieChart
  hole and `TimeField::separator`: no Quill surface needs them (the statistics
  pie is solid, as in tdesktop).
- Animated WebP in `img`: Quill decodes pictures itself and sends an animated
  WebP as a file, as tdesktop does.

## What Quill's patches needed

- `gpui-base`: two conflicts in the input engine. `element.rs` keeps the
  window-scoped caret blink against upstream's paint tuple (`selected_range` is
  gone from it); `state.rs` keeps `Hsla` (upstream's fold backgrounds) next to
  Quill's `AnyWindowHandle`. Everything else applied.
- `gpui-component`: applied cleanly.
- `gpui-pre`: the ad placement patch (`ListState::estimated_item_heights`,
  6561f2e24) landed on main after the 0.3.12 re-vendor was prepared and is
  carried onto 0.3.12.

## Other updates

`cargo update`: notify-types 2.3, gpui-fast 0.1.14 (not built: Quill does not
enable the feature), the html5ever 0.40 / selectors 0.41 family under the kit's
HTML text, and patch releases. Held as before: moxcms 0.9, x11rb 0.14 (still
asked for by gpui-pre-linux 0.3.12), zbus 6. TDLib stays at 1.8.70: master's
four commits since are bot-only (`replyToCallbackQueryWithEphemeralMessage`
returns the message). Rust 1.99.1 is still the latest stable.

## How it was verified

- `quill-tools/gate.sh` (fmt, core clippy, core and UI tests, licenses) and
  macOS `cargo clippy --features demo-capture --all-targets -- -D warnings`.
- Screenshots (ready-chats, light and dark): the Mute submenu right of the menu
  in a wide window, and flipped to its left in a 640 pt window.
- CI on the pull request: Linux UI tests, the Windows UI check and the
  gpui-pre-apple Metal tests.

Not verified: real Linux or Windows hardware, and the Windows colour emoji path
(now on the CPU).
