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

## Window backdrop, cards and the drawer (step 7)

`chrome::frames` holds the frames every site passes through: `list_frame`
and `panel_frame` (Classic's border and fill, or a rounded card fill with a
hairline when the edge asks for one), `card` (the shadow, corners and clip
around a slot's content; Classic returns its input) and `peek_surface`.
The shell paints the viewed chat's wallpaper across the shell area
(`paint_window_backdrop`) and the history paints none (`history_backdrop`),
so gradient bubbles read their area from the window. The window backdrop
reuses the history's wallpaper slot, so the cache keeps one picture per
window. A chat window does the same on its body.

Seam for the renderer's rounded clip: `frames::card_clip` clips to the
rectangle until `clip_rounded` lands, and the surface inside rounds its own
fill. Content that paints to a card's edge (a hovered row at the bottom
corner, a profile cover) still shows square corners until then.

The list slot owns its gutter (left, top, bottom). A docked panel column is
a padded card; `PanelPane::Over` is an absolute card at the right gutter,
offset by `(1 - shown) × (width + gutter)` while it slides. In the drawer
band the header button holds the drawer open, a click at the reveal edge
does too, a `DismissLayer` beside it closes it, and opening a chat closes
it. With no chat open the drawer is shown.

Not in this step: the motion durations and curve from `Motion` (Classic's
200 ms ease-out cubic still drives every slide); step 11 moves the slides
onto snapshots and takes its timings then. The title bar keeps the theme's
background (step 10).

Verification note: screenshot demos stop a slide where it is (a pre-existing
behaviour of the demo pipeline, the same on main for the Classic sidebar
toggle), so the held drawer's resting place is proved by
`chrome/cards_tests.rs` (its card lands at x = 16), not by a capture.

## The header and composer float (step 8a, 8b)

`session_history` builds the header and the bars (`history_bars`, in
`conversation/top_bars.rs`) apart from the body (`history_body`), in the
order the one chain used. Classic still draws them in the conversation
slice. In Floating, `ConversationPart::Bars` renders them in a Header
slice of their own with its own animation layer, so a typing peer's dots
and a title's emoji status tick in the header's layer, not the
history's. `floating_column` lays the history over the whole pane and
the header and composer boxes over it, each a gutter from the pane's
edge and centred at most the chat column wide; the boxes take the
heights their slices measured, as the composer did. The strips under the
history (bottom subsection tabs, the busy-call banner) join the composer
slice.

Order in Floating is header pill, then the player, call and capture
bars, then the chat's own bars; Classic keeps the player and call bars
above the header. Web K shows its audio and call bars under the chat
header, so that order follows it. Each bar is wrapped in a box that
blocks clicks (`block_mouse_except_scroll`), which the wheel passes
through; the gaps between bars have no box.

Deviations from the plan, recorded here:

- The pills do not clip their content: the composer's popups (the
  emoji panel, the round-video circle) hang outside the pill, and a
  rounded clip would cut them. A hover fill that reaches a pill's end
  stays square until a bar is reshaped in step 8f.
- `MeasureHeight.redraw` stays one view until a slice other than the
  slot reads a measured height (step 8c), and `chrome/frame.rs` lands
  with its first reader in 8c.
- The typing test proves the dots tick with no slice rendering and with
  the header's own layer; a typing update itself still redraws every
  conversation slice through the session's notify, as it did.

## The history under the pills (step 8c)

`chrome/frame.rs` holds the pure geometry (`floating_frame`,
`HistoryInsets`); `QuillApp::history_insets` gives the rows' insets from
the measured header and composer heights (zero in Classic, so every
reader below has one path). The list carries them as padding on its
first and last rows (the kit's 8 px past each pill), and those two rows
are measured again when an inset changes (`note_history_insets`). A
change of either pill's height redraws the history as well as the slot
(`MeasureHeight.redraw` is a list).

`history.viewport` is a `HistoryViewport { full, readable }`. Reads, the
date pill's top row, the on-screen range, landings, sponsored views and
paging use `readable`; drag selection, its edge auto-scroll, the pull
indicator, the wide selection width and the reveal use `full`. The
landing loop needed no other change: it measures its error against the
readable top and converges as before. The three open-loop scrolls that
put a row at the top (Page Up/Down, keyboard selection, accessibility
focus) go through `scroll_row_to_view_top`, which scrolls the row under
the header. Scroll memory saves and restores offsets of the same list,
so it needs none.

The jump button, the corner buttons and the date pill move clear of the
pills by the insets; the pull hint and a chat window's composer toast
sit over the composer pill's box.

Not tested directly: that the tail is kept when the composer grows (the
kit's tail-follow covers it, as the last row's padding is part of the
content), and scroll memory across a composer height change.

## The centred column (step 8d)

`chrome::frame::RowColumn` says how rows are placed: across the pane
(Classic), or in a column centred in it, at most `chat_column` wide and
a gutter from each edge. The seed and the paint probe lay bubbles out
for `RowColumn::width_in(pane)`, so the column of 696 lays text, media
and albums out as a wide Classic pane does, and the wide layout (880)
never turns on in Floating; the adaptive-wide checkbox hides by its
own rule. The probe still keeps the pane's width: when only the sides
move (a resize, the list or a panel sliding), it redraws the history
for the new row padding without measuring rows again.

The rows' side padding is `HistoryInsets.left/right` plus the kit's
12 px row inset (the kit's row padding replaces its `px_3`), so
bubbles sit 12 px inside the pills' edges. `left_reserved` stays 0
until the left tab column becomes a card (step 8e).

## Bodies that are not a list (step 8e)

Every body that takes the list's place (placeholders, the history skeleton,
load failures, intros, the secret chat explainer, Saved Messages sublists,
forum topics, thread status, the empty chat) goes through
`chrome().pane_frame()`. In Classic it returns `None` and the body is left as
it was. In Floating the body is padded clear of the measured pill heights
and laid in a centred box at most `chat_column` wide with a gutter on each
side.

Deviation: the plan reused the list's `HistoryInsets` sides here. Those
sides come from the list's measured pane width, which a placeholder never
measures (the topic skeleton drew across the whole pane on its first
frames). The pane frame centres with flex and a max width instead, so it
needs no width and lands right on the first frame.

The thread root bar and thread info card move into the floating bars when a
thread is shown. `thread_shown` is one rule for that: no thread pending and
a thread for the open chat that has not failed. Classic keeps them where
they were.

The Left tab column becomes a card (`tab_column_frame`: Classic keeps its
border_r, Floating draws a surface card) set a gutter in from the pane, and
`left_tabs_reserved` shifts the pill lanes and the row column right by the
card's width plus a gutter, so the pills centre in the rest. Its active
marker sits on the card's edge; it is not clipped until the rounded mask
lands (the `card_clip` seam).

Classic proof: the 23-capture set matches the Step 0 baseline (known noisy
demos only), and Classic `ready-bot-threads` in Left mode,
`ready-direct-messages` and `ready-topic-loading` are pixel-identical to
main.
