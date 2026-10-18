# Changes made by the Quill project

This directory is gpui-pre-wgpu 0.3.9 from crates.io (Apache-2.0,
`LICENSE-APACHE`), GPUI's wgpu renderer and cosmic-text text system (Linux),
used through `[patch.crates-io]` in Quill's `Cargo.toml`. Commit `dc79c62c` added
the unmodified registry copy (without the registry's `.cargo-ok`,
`.cargo_vcs_info.json`, `Cargo.toml.orig` and `Cargo.lock`), so
`git diff dc79c62c -- third_party/gpui-pre-wgpu` shows every change. The crate's
own unit tests read fonts from the Zed source tree and do not build here.

The Quill project changed it in 2026 so emoji draw in colour on Linux
(`docs/decisions/linux-titlebar-emoji.md`):

- `src/cosmic_text_system.rs`: cosmic-text's fallback list reaches GNU FreeSans
  and DejaVu Sans before Noto Color Emoji. FreeSans has boxed letters for the
  regional indicators, so a flag such as 🇺🇸 drew as two boxed letters; it and
  DejaVu also have outline glyphs for many emoji (❤, ⛰, keycaps, 😂).
  `layout_line_no_separators` now gives every grapheme that asks for emoji
  presentation (Unicode TR #51: emoji-default characters, U+FE0F, keycaps,
  flags, skin tones, ZWJ and tag sequences; U+FE0E keeps text) its own span
  that names Noto Color Emoji, when that family is installed, so the whole
  cluster shapes in colour. Text-default symbols (©, ™, a bare ❤) keep their
  text glyphs.
- `Cargo.toml`: `unicode-properties`, for the emoji presentation tables.
