# Changes made by the Quill project

This directory is gpui-component 0.11.0 from crates.io (Apache-2.0,
`LICENSE-APACHE`), used through `[patch.crates-io]` in Quill's `Cargo.toml`.
The commit "Vendor gpui-base and gpui-component 0.11.0 unmodified from crates.io" added the unmodified registry copy (without `.cargo-ok`,
`.cargo_vcs_info.json`, `Cargo.lock` and `Cargo.toml.orig`), so
`git diff <that commit> -- third_party/gpui-component` shows every change.
(First vendored at 0.7.3 in `255bf944`; carried to 0.8.0 in
`docs/decisions/gpui-kit-0-8-0.md`, to 0.8.1 in `docs/decisions/gpui-kit-0-8-1.md`
to 0.10.0 in `docs/decisions/gpui-kit-0-10-0.md`, and to 0.11.0 in
`docs/decisions/gpui-kit-0-11-0.md`, where the patch applied unchanged.)

`src/tab/tab_bar.rs`: the sliding indicator sprang (280 ms) whenever the
selected tab's left or width changed, so a badge appearing in a tab restarted
the spring and kept the window redrawing every frame. The animation parameters
now carry a jump counter that is part of the spring key. A change of the
selected tab keeps the key and springs as before; a bounds-only change
(selection unchanged) bumps the counter, and a fresh spring adopts its first
value immediately, so the indicator jumps. The indicator carries a
`tab-indicator` debug selector under `test` / `test-support`. Background:
`docs/decisions/folder-tabs-no-badge-spring.md`.

0.8.0's "sliding indicator appears after the first frame" (#3574) is a
different fix: it records where render placed the indicator
(`TabIndicatorBounds::placed`) and asks for one more frame when the selected
tab was measured elsewhere. It still retargets the same spring when only the
selected tab's bounds change, so a badge would still spring the indicator.
The patch is kept and works with it: the extra frame is the one that bumps
the jump counter. `TabVariant::Folder` (#3415) draws no sliding indicator.

`src/message_scroller.rs`: `MessageScrollerState::top_anchor` exposes the
list's top row and its pixel offset, so Quill can remember where a chat was
left and return to it (`docs/decisions/sweep-scroll-restore.md`). Re-apply
when re-vendoring. It gates on the list's follow state, not on
`is_scrolled_up`, so a position set by `scroll_to_offset` reads correctly
before the next layout. 0.8.0's `logical_scroll_top` (#3485) reads the same
position but not the follow state. Quill's `scroll_to_item_offset` setter was
dropped at 0.10.0, whose `scroll_to_offset` (#3922) does the same.

`src/theme/{mod,mono_font}.rs`: `installed_font_names` is public, and
`prefetch_installed_font_names` fills the same once-per-process list on a
background task. Quill starts the prefetch when the platform exists, so theme
init finds the list ready and the font picker reads the same list instead of
listing the fonts a second time on the main thread (commit `cfdb6d7e`,
`docs/decisions/launch-time-fonts.md`).

`src/theme/{system_font,mono_font}.rs`: theme init no longer reads the
installed list when the default fonts load under their own names. The list
takes about 130 ms to build on macOS, and theme init waited about 28 ms for
the prefetch on a warm launch. `resolve_default_font` lists the families only
when `.SystemUIFont` resolved to another family (`substitute` takes the list
as a closure), and the monospace default is kept when GPUI loads it as
itself. Only the web reads the list first, because it starts with no font
and resolving one would panic there (`docs/decisions/launch-warm-waits.md`).

`src/highlighter/{mod,syntect_backend}.rs`: `highlighter::capture_names` is
public with the `syntect` feature (without `tree-sitter`). It returns the
highlight names of a text for a language, so Quill colors message code blocks
from its own theme tokens and runs the parse on a background thread
(`docs/decisions/code-syntax-highlight.md`). Re-apply when re-vendoring.

`src/message_scroller.rs`: `MessageScrollerState::distance_to_edges` passes
`ListState::estimated_distance_to_edges` through, so Quill's history asks for
the next page while three screens are left (`docs/decisions/history-preload.md`).
Re-apply when re-vendoring.

`src/message_scroller.rs`: `MessageScrollerState::estimated_row_heights`
passes `ListState::estimated_item_heights` through, so channel ads go between
posts at least two screens apart (`docs/decisions/sponsored-placement.md`).
Re-apply when re-vendoring.

`src/message_scroller.rs`: `MessageScroller::with_jump_button_shown_after`
keeps the jump-to-latest button hidden until the viewport is more than that
distance above the last row (tdesktop `historyToDownShownAfter`, 480px), and
`jump_button_always` shows it wherever the list is scrolled, for a history
whose loaded window stops short of the latest message. Background:
`docs/decisions/unread-open-landing.md`.

`src/dialog/dialog.rs`: `Dialog::wrap_surface` wraps the dialog's surface
(everything above the backdrop) in an element the caller passes. The base
dialog lays the surface out in place but prepaints and paints it in a
deferred pass after the rest of the window, so an element that sets
something up for its subtree while it draws has to sit inside the surface.
Quill puts its bounded image cache there, so pictures in dialogs leave
memory once off screen (`docs/decisions/dialog-images-bounded-cache.md`).
Re-apply when re-vendoring.

`src/message_scroller.rs`: `MessageScroller::wrap_jump_button` wraps the
built-in jump-to-latest button, while it is drawn, in an element the caller
passes. Quill marks the button as covering the rows' animations
(`anim_layer::occluder`) with its own bounds, only while it shows
(`docs/decisions/codex-history-anim-layer.md`). Re-apply when re-vendoring.

`src/message_scroller.rs`: the built-in jump-to-latest button calls
`block_mouse_except_scroll`. It floats over the rows, and a GPUI hitbox with
the default behavior leaves the hitboxes behind it hovered, so one click on
the button also clicked the row content under it (a clip or photo opened the
media viewer). The wheel over the button still scrolls the list. Background:
`docs/decisions/jump-click-through.md`. Re-apply when re-vendoring.

`src/message_scroller.rs`: `MessageScrollerState::scroll_by` passes
`ListState::scroll_by` through. Quill's history lands a jumped-to message in
the middle of the view and the "Unread messages" bar at its top by measuring
where the paint put them and scrolling by the difference
(`docs/decisions/unread-open-landing.md`). Re-apply when re-vendoring.

`src/root.rs`, `src/window_ext.rs`: `WindowState::last_dialog_id`,
`WindowState::close_dialog_by_id` (with `close_dialog_at` split out of
`close_dialog_internal`) and `WindowExt::{last_dialog_id, close_dialog_by_id}`.
Quill's `QuitGate` must close only its own "Stop and quit?" alert, never a box
that opened on top of it (`src/ui/export_quit.rs`). Closing a dialog by id
moves focus only when it was the newest; otherwise it hands its saved focus
handle to the dialog above. Re-apply when re-vendoring.

`src/theme/mod.rs`, `src/title_bar.rs`, `src/dialog/dialog.rs`: the title-bar
height is a theme value, `Theme::title_bar_height` (default
`TITLE_BAR_HEIGHT`, not serialized), changed with
`Theme::set_title_bar_height`. `TitleBar` draws at that height and a dialog's
backdrop leaves the band above it alone (`dismiss_below_y`). The setter also
moves `sheet.margin_top` and `notification.margins.top` by the same amount,
so sheets and notifications keep their distance from a shorter bar; it is a
plain `&mut self` setter, so callers change it through `Theme::global_mut`
and refresh the windows themselves. Quill's Compact mode sets a 26px bar from
`DensityMetrics::title_bar` in `apply_appearance`
(`docs/decisions/compact-mode-density.md`). Write the height only through the
setter: the field is public, and writing it directly leaves the sheet and
notification margins behind. Re-apply when re-vendoring.

`src/menu/submenu_aim.rs` (new), `src/menu/{mod,menu_item,popup_menu}.rs`:
menu aim, Telegram Desktop's rule for reaching a submenu diagonally (lib_ui
`PopupMenu::SubmenuAim`, `insideSubmenuAim`, 300 ms rest). `SubmenuAim` is
the pure decision, public as `menu::{SubmenuAim, AimGeometry, AimPointer,
SUBMENU_AIM_REST}` because Quill's hand-built menus use it too
(`src/ui/menu_aim.rs`). In `PopupMenu`, an item takes the selection on a
pointer move (`item_pointer_moved`) instead of on hover, and only when the
aim lets it: while the pointer crosses rows inside the triangle from where
it left the submenu's row to the submenu's near edge, the submenu stays,
the crossed rows are not lit (`MenuItemElement::lit_by_pointer`) and the
row under the pointer takes the selection once the pointer rests for 300 ms
(a `Task` held by the waiting stage; the clock is the executor's, so tests
advance it). The corridor takes the submenu's side from the measured
bounds, so it does not depend on `update_submenu_menu_anchor`. Re-apply
when re-vendoring: upstream has no hover intent for submenus.

`src/popover_glass.rs` (new), `src/styled.rs`, `src/notification.rs`: glass
effects (`docs/decisions/glass-effects.md`). `PopoverGlass` is a global
holding one function from `PopoverGlassSurface::{Popover, Toast}` to an
optional `LensStyle`; Quill installs it. While it answers `Some`,
`popover_style` (popovers, menus, selects, comboboxes, the date picker,
input completions) and the notification card draw that glass through
gpui-pre's `Styled::backdrop_glass` instead of their fill, edge ring and
shadow; while it answers `None`, or with no global, they draw exactly as
before. `src/tab/tab_bar.rs` asks it too, with
`PopoverGlassSurface::TabSelection { travel }` (how far the sliding
indicator still has to go, in tab widths), for the segmented variant's
selection pill of a bar built with `TabBar::glass_selection()`. Re-apply
when re-vendoring.

`src/popover_shape.rs` (new), `src/styled.rs`: the Floating layout
(`docs/decisions/floating-layout.md`). `PopoverShapeHook` is a global
holding one function to an optional `PopoverShape` (corner radius and
shadow); Quill installs it. While it answers `Some`, `popover_style` draws
that radius and shadow instead of the theme radius and the ring-and-shadow
edge, and clips the content to its rounded shape (`clip_rounded`); while it
answers `None`, or with no global, popovers draw exactly as before. Glass
popovers take the radius and the clip. With `pop` set, `dropdown_popup`
(`src/popover.rs`) enters with Web K's pop, a scale from .8 and a fade over
200ms on `cubic-bezier(.4, 0, .2, 1)`, drawn through a snapshot
(`with_snapshot_animation`); with `QUILL_SNAPSHOTS=off` or a glass popover
it keeps the fade and slide. Re-apply when re-vendoring.
The shape may also carry a `fill` (in place of the theme's popover colour)
and an `edge` (a 1px border colour); the app sets both for dark menus,
which would otherwise merge with near-black cards.

## Rounded content masks

Changed by the Quill project in 2026
(`docs/decisions/rounded-content-masks.md`): gpui-pre's `ContentMask` gained
a rounded part, so the masks built here are made with `ContentMask::rect`
instead of a struct literal. Nothing else changes.
