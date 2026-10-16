# Changes made by the Quill project

This directory is gpui-component 0.7.3 from crates.io (Apache-2.0,
`LICENSE-APACHE`), used through `[patch.crates-io]` in Quill's `Cargo.toml`.
The commit before the one adding this file holds the unmodified registry copy
(without `.cargo-ok`, `.cargo_vcs_info.json` and `Cargo.lock`), so a diff
against it shows every change.

`src/tab/tab_bar.rs`: the sliding indicator sprang (280 ms) whenever the
selected tab's left or width changed, so a badge appearing in a tab restarted
the spring and kept the window redrawing every frame. The animation parameters
now carry a jump counter that is part of the spring key. A change of the
selected tab keeps the key and springs as before; a bounds-only change
(selection unchanged) bumps the counter, and a fresh spring adopts its first
value immediately, so the indicator jumps. The indicator carries a
`tab-indicator` debug selector under `test` / `test-support`. Background:
`docs/decisions/folder-tabs-no-badge-spring.md`.

Liquid glass on macOS (2026). `src/glass.rs` (new, exported as
`gpui_kit::component::glass`): `GlassVariant`, `GlassStyle` (a
`gpui::GlassEffect`, an optional 1px border and the corner radius;
`GlassStyle::regular` is macOS 26's Regular glass, fitted to
`NSGlassEffectView` captures and held to the luminance range
`readable_luminance` gives the theme's text colours), `glass_shadow`, the
`GlassProvider` global
the app installs, `glass_style()`, and `GlassSurface`, an element ported from
bezel's `Surface` (github.com/crabtalk/bezel, MIT, `crates/ui/src/surface.rs`;
notice in `licenses/bezel-MIT.txt`). `GlassSurface` paints its child in one
scene layer: the backdrop blur, the drop shadows cut to outside the shape,
the border, then the child. `src/menu/popup_menu.rs`: when
`glass_style(GlassVariant::Regular, cx)` is `Some`, the menu drops
`popover_style`'s fill and shadow and renders inside a `GlassSurface` that
paints `glass_shadow` outside the shape, with item highlights cut
concentric with the glass's radius; when `None`, it renders as before. The primitive comes from gpui-pre (see its QUILL-CHANGES.md).
