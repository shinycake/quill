# Concentric highlights and glass highlights

Owner ask, 2026-11-26: a hover or selection highlight inside a rounded
container must have corners that match the container's (concentric:
inner radius = outer radius minus the inset), everywhere, in Classic and
Floating, light and dark, glass on and off. With glass effects on, the
highlight is glass, not a flat fill. Same round: the folder tab pill must
hold its icon and counter, and the main menu must stay readable in glass.

## One rule, one helper per container kind

- `gpui_kit::component::popover_shape::concentric_radius(outer, inset)` is
  the rule (floored at zero). Nothing else subtracts radii by hand.
- Kit popovers: `popover_frame(window, cx)` reads the radius and edge the
  popover in that window draws (the app's Floating shape or the theme
  radius; no edge width under glass, whose rim is inside the lens), and
  `popover_row_radius(padding, ..)` applies the rule. `PopupMenu` rows and
  the select and combobox rows use it.
- Quill's own menus (`menu_surface`): `menu_radius(look)` and
  `menu_row_radius(look)`; rows inside a context menu take
  `menu_row_corners(items)`, which reads the look the window noted when it
  reset the menu's claims at the top of its frame
  (`ContextMenuItems::note_look`). The composer's hashtag list and the emoji
  suggestion bar use the same menu radii.
- List rows on a card: `QuillApp::list_row_corners` is the card radius
  minus the column's padding and the card's edge (12 in Floating's
  Standard density). Classic's list has no rounded card, so its rows keep
  the theme radius. Chat rows, search results, archive, contact, community
  and saved-sublist rows use it.
- Rows that run edge to edge (the bot command list, the profile details
  card) are square and their container clips them (`clip_rounded`), so the
  first and last row follow the container's curve.

A consequence: in Classic, menus are 6 px with a 5 px inset, so their rows
are 1 px, close to square. That is the rule applied as asked; a rounder
Classic would need a larger Classic menu radius, which is a look change of
its own.

## Glass highlights

- `GlassRole::Highlight` is a clear dome tinted a quarter of the way from
  the background to the accent, with the role's whole frost floor (as media
  glass keeps), so labels and shortcuts keep the recipe test's limits over
  black, white and grey. The accent share is where the light and dark
  contrast tests still pass with margin.
- The kit asks through `PopoverGlassSurface::Highlight`
  (`popover_glass::highlight` for kit menu items and select rows); Quill's
  rows use `RowHighlight::{hover_highlight, lit}` and
  `PressableDiv::pressable_lit`.
- Chat rows on a glass card: the open row is the glass highlight with the
  theme's text; the hovered row keeps the plain hover fill, so the two stay
  apart.
- The folder pill in Floating is the glass highlight when glass is on, its
  label in the theme's text colour (`TabBar::glass_selection` now covers the
  Pill variant).

## Readable glass menus and panels

Menus and panels now take the bar's thicker frost and blur (the `Card`
recipe already did): the main menu sits over the chat list, whose titles
read through the clearer glass. The backdrop now shows as colour and shape,
not text.

## Folder tab pill

A Pill tab with a prefix or suffix pads its whole content, so the folder
icon and the unread counter sit inside the capsule with the label (6 px
between them) instead of on its rounded ends. On the solid accent pill the
icon takes the label's colour and the counter inverts (white with accent
text). Over glass the strip drops its edge fades, which were opaque blocks
of the list colour.

## Segmented bars (2026-11-27)

The kit's segmented `TabBar` took its pill's radius as the bar's less 2 or
3 px, but the pill sits 2, 3 or 4 px inside the bar (`segment_inset`: the
bar's side padding and half of what the pill leaves of its height). It now
takes `concentric_radius(bar radius, segment_inset)`, so every segmented
bar's selection and hover follow the bar's corners (Classic: 8 and 4 at the
default size). `TabBar::bar_radius` rounds a bar with its own radius and
its tabs follow; Floating gives the Chats / Contacts / Calls bar (and the
glass tuner preview's copy of it) a capsule, like its search field and
folder pills (`chrome::list_parts::segmented_tabs`).

## Not covered

Main menu tiles do not touch the popover's corners (the search field and
the Night Mode row do), so they keep their radius. Call panels use their
own chrome. Linux and Windows are untested on real hardware; the captures
are macOS demo builds.
