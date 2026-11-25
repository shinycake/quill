# Floating layout

Issue #1507. A second window layout, Floating, beside today's Classic:
the chat list as a rounded card, the conversation's header and composer
as pills over the history, one wallpaper behind the whole window, and a
centred 696 px conversation column. Modelled on Telegram Web K by the
owner's decision. Classic stays the default and keeps every pixel and
every timing it had.

This record grows with the work; each section names the step that
landed it.

## One choice, everything else derived (step 3)

- `AppearancePrefs.layout: LayoutStyle` (`classic` | `floating`) is the
  only new persisted state. A settings file without the key reads as
  Classic.
- `quill::layout_style::resolve(choice, override, readings) -> Chrome`
  is pure. The first matching row wins:

  | Choice | Reading | Result |
  | --- | --- | --- |
  | Classic | any | `CLASSIC`, no edge, flat |
  | Floating | width under 601 | the same value as Classic |
  | Floating | high contrast | `FLOATING`, flat, hairline |
  | Floating | battery saving forced | `FLOATING`, flat; hairline on a plain backdrop |
  | Floating | plain backdrop under the contrast floor | `FLOATING`, shadowed, hairline |
  | Floating | otherwise | `FLOATING`, shadowed, no edge |

- The contrast floor is a WCAG contrast ratio of 1.15 between the card
  fill (`theme.sidebar`) and the plain window background
  (`theme.background`). The plan named a floor without a number; 1.15
  is where a card stops reading as a separate surface in the built-in
  themes without a wallpaper.
- `ChromeMetrics` is data, two constants. `CLASSIC` holds today's
  literals, pinned by `classic_holds_todays_literals` and, for the slide
  timings, by `classic_slides_keep_todays_timing` against
  `sidebar_reveal::SLIDE_DURATION`.
- Breakpoints, Web K's, in interface pixels: 600 and under is one
  column with Classic's chrome (`ONE_COLUMN_TO`), 601 to 925 shows the
  list as a drawer (`DRAWER_TO`), above 925 the list is docked.
- `QuillApp::chrome(cx)` resolves for the window whose state is in
  place, from that window's width (`WindowSlices.width`, written by the
  window's own render). When a resize crosses 600 the window's slices
  are rendered again (`note_window_width`).
- `QUILL_DEMO_LAYOUT=classic|floating` overrides the choice for demos;
  it is read once, in the core crate.
- The card radius is `theme.radius × 4` (24 at the default radius 6),
  `ui/chrome/radius.rs`, Quill's own step above the kit's `radius_4xl`.

## The guard

`scripts/check-layout-style.py` fails when feature code names
`LayoutStyle::` (anywhere) or one of the metrics' enums (in a file that
reaches `layout_style`) outside the files that decide the look:
`layout_style.rs`, the pref, `density.rs`, `ui/chrome/`,
`ui/shell_layout/`, `ui/app_slice*`, the conversation's two split files,
`chat_row/row_ink.rs`, `ui/appearance/`, `ui/wallpaper.rs`, the two
reveal owners and the folder slide. The metrics' enum names (`Edge`,
`Clip`) are common words elsewhere (`pull_next` has its own `Edge`), so
they count only in a file that imports the layout module.

## Settings and search (step 3)

Settings › Appearance gets a Layout row above Compact mode: two tiles,
each a miniature of the window drawn from the same `ChromeMetrics` the
real window reads (`ui/chrome/miniature.rs`), in the kit's
`RadioGroup` (its `Radio` takes custom children). The catalog entry
`appearance.layout` makes it searchable (layout, floating, classic,
cards, rounded).

## Onboarding (step 4)

After the highlights a third card, `FirstRun::Style { since }`, shows the
same picker as Settings (`layout_picker`), so the two cannot differ. Its
ways out: Start Messaging, Escape (an `esc_stack` layer), and the window
closing. Picking a tile writes the Appearance pref at once and stays on the
card. The highlights' button now reads Continue and leads to it.

The wallpaper offer is derived, not stored: it shows while Floating is the
choice, the wallpaper source is the theme's and the theme has none. During
onboarding no account is signed in yet, so the card says the wallpaper
appears after sign-in. Enter acts on the focused button only; the card has
no key binding of its own.

What's New gets a Highlights line in the sample notes; the release notes
line is in the PR description.

## Wallpaper cache (step 5)

The cache drops the oldest pictures over its budget, but never one shown
this frame, and always keeps the newest. A window-wide wallpaper at 2560 px
can then exceed the budget without the picture on screen being evicted and
decoded again every frame.

## Shell geometry and the drawer band (step 6)

`ShellInput` carries `gutter`, `chat_column` and `drawer_to` from the
metrics (`QuillApp::chrome_metrics`, which needs no theme, so the shell
keeps reading its input without a `cx`). The list column owns its left
gutter, a docked panel column its right one, a forum column the gutter
before it. With a `chat_column` a panel docks only when the list, a full
696 column with its two gutters and the panel fit; otherwise it is
`PanelPane::Over`, a card over the chat that leaves the chat pane its whole
width. `Fill` is not produced then.

From 601 to 925 the list is a drawer (`ListDock::Drawer`): docked 0 whatever
the preference, no forum column, no folder rail, its width the dragged
width less two gutters and a 44 px strip of chat.

Deviation: the plan put `Held` into `Peek`. `Peek` is the view of the
pointer stage that lives inside the hidden preference (`Mode::Hidden`), so
a held drawer in a window whose list is not hidden had nowhere to live.
`SidebarReveal` gains its own two-stage enum, `Drawer { Closed, Held }`,
next to `Mode`, and `Peek::Held` is how the view reads it. Its exits: a chat
opened, a click beside it (`DismissLayer`), Escape, the button again, the
window leaving the band (`leave_drawer_band`, at once), the window closing
(dropped with its owner). None of them writes the hidden preference. Hover
peeking in the band works as before when the list is hidden; with the list
shown the button or a click at the reveal edge opens the drawer.

No debug assertion holds "a gutter never comes with a width of 600 or
under" (the never-crash rule); a test does.
