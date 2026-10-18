# Linux: one title row, and colour emoji

Two problems were reported on Linux Mint (Cinnamon, X11) with Quill's own
window frame:

1. The in-window menu bar (File, Edit, View, Window, Help) sat in its own
   30 px row above the 34 px title bar, which held only the word "Quill",
   search and the window buttons. That made two stacked bars of chrome.
2. Flag emoji drew as two boxed letters (🇺🇸 as boxed "U" and boxed "S"),
   while 🚀 and 🐧 drew fine.

## One title row

Telegram Desktop on Linux has no menu bar. Its own frame shows the title and
the window buttons in one row, and its commands are in the main menu.
Quill keeps its menus for keyboard users and for discoverability, but now
puts them in the title row:

- `FrameTitleBar::menu` (`src/ui/window_frame.rs`) takes the in-window menu
  bar and puts it at the start of the row, in place of the "Quill" caption.
  The OS taskbar already names the window. With the system frame ("Use system
  window frame"), the slim action row holds the menus on the left and the
  actions on the right.
- The menu fills the free width, so its empty space is still title bar.
  Dragging the window and double-clicking to maximize work there, and each
  menu button stops its own mouse-down so a click opens the menu instead of
  moving the window.
- macOS is unchanged: it uses the native menu bar.

This was checked with captures under xfwm4 with its compositor, which gives
GPUI client-side decorations as Cinnamon does. The menus open from the row
and a double-click maximizes. Synthetic drags don't move windows under Xvfb,
whether the build has this change or not, so dragging is checked in code
only: it uses the same title-bar handlers as the double-click.

## Colour emoji

Root cause: GPUI's Linux text system is cosmic-text. Quill sets no font
fallbacks, so every character missing from the UI font goes through
cosmic-text's fixed Unix fallback list: Noto Sans, DejaVu Sans, **FreeSans**,
mono fonts, Noto Sans Symbols, and **Noto Color Emoji last**. Mint installs
GNU FreeSans by default, and FreeSans has the regional indicator letters as
boxed-letter glyphs, so it won the flag and shaped each letter alone. 🚀 is
not in FreeSans, so it reached Noto Color Emoji. DejaVu and FreeSans also took
❤️, ⛰️, keycaps and the big-emoji 😂 in black outline.

Options considered:

- `FontFallbacks` on the root text style: GPUI loads each fallback as a
  primary face, and it skips faces without an "m". Noto Color Emoji has no
  "m", so it can never enter the chain.
- A Quill-side wrapper around the platform text system that splits runs by
  font ID: same problem, because there is no `FontId` for the emoji font.
- Patching the cosmic-text system (chosen): `third_party/gpui-pre-wgpu`
  gives each grapheme that asks for emoji presentation (Unicode TR #51) a span
  that names Noto Color Emoji. Text-default symbols (©, ™, a bare ❤) keep
  their text glyphs, and U+FE0E keeps text.

Every layout and measurement goes through that function, so widths match
what is drawn. The renderer only paints Noto Color Emoji in colour
(`check_is_known_emoji_font`), so that is the only family used. Without it,
behavior is unchanged. Sprite emoji sets (Twemoji, Noto packs) are
unaffected, because they replace emoji before text shaping.

The `ready-emoji-set-chat` demo now puts flags in the chat title, so the bold
header and the chat list row show them too.
