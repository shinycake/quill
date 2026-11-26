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

## Bars in two cards (step 8f)

Owner direction (2026-11-25), which replaces the plan's one pill per bar:
in Floating the header and every bar over the history (player, call,
capture notice, topic strip and info card, top tab strip, saved tags, bot
info, chat bars, pinned bar and list, pending bot reply, chat search,
thread root bar and info card) are rows of ONE card: header row on top,
each bar under it parted by an inset hairline (the theme border at half
alpha, inset by the header's padding), one radius-24 outline, one
shadow, the header's column width. With no bars the card is exactly the
header pill. Under the history the same holds for the composer's card:
the bottom tab strip and the busy banner sit over the composer row with
the hairline under them, and the composer note, the channel footer and
the bottom action bar take the composer's place in the card.

The helpers (`chrome/bars.rs`):

- `bar(el, BarLine)`: Classic draws the hairline the bar drew by hand
  (under or over it); in a card the bar drops it and takes the card's
  fill. It is called after the bar's own `bg`.
- `tinted_bar` / `tinted_tile`: a bar with its own colour (the call bars,
  the capture notice) becomes a rounded tile inset 6 px in the card,
  concentric with its corners, so its colour never reaches past the round
  ends while the renderer's rounded clip is pending (the `card_clip`
  seam). Classic is unchanged.
- `strip(el, BarLine)`: a strip over the composer, with the hairline
  under it in a card.
- `stack_card`: the card itself (the pill's fill, edge and shadow).
  `header_frame` and `composer_frame` draw nothing of their own in
  Floating: they are rows of their cards.

Deviation: the plan's `chrome().bar_reveal(..)` Rise presenter is not
built. Inside a card a bar opens by height under a rectangular clip, which
is right there (the clip no longer cuts a pill's round ends or shadow), so
the pinned bar keeps `slide_open`, and the card's height follows the
reveal with no state of its own. The hairline above a sliding bar shows
at full strength while the bar opens.

The bar helpers read `chrome_metrics()` (no theme readings), not
`chrome(cx)`: reading the full chrome per bar looks up the chat's
wallpaper each time, and that alone moved Classic's skeleton pulse by one
level at its corners in `ready-topic-loading`. Only the two cards read
the full chrome, and only in Floating.

The top card clips its rows (`card_clip`, rectangular until the rounded
mask lands); the composer's card does not, as its popups hang outside it.

Classic proof: the bars' helpers are identity tests in `chrome/bars.rs`
(both edges); the 23-capture set matches the baseline (known noisy demos
only) and `ready-channels`, `ready-voice-playing`, `ready-topic-loading`,
`ready-bot-chat` and `ready-direct-messages` are pixel-identical to main.

## Glass on the cards

Owner direction (2026-11-25): glass works in both layouts. Classic keeps
#1497's glass exactly. In Floating, with Settings › Appearance › Glass
effects on, the list card, the header-and-bars card, the composer card,
the side-panel card and the left tab column are backdrop glass over the
window's wallpaper, with the `Panel` role's recipe
(docs/decisions/glass-effects.md); no new style system.

- `chrome.card_glass(cx)` is the one question: `None` in Classic (square
  clip), else `glass::lens(GlassRole::Panel, cx)`, which is `None`
  whenever the look resolves solid (switch off, Reduce Transparency, high
  contrast, saving power, a software adapter). Every frame helper
  (`list_frame`, `panel_frame`, `tab_column_frame`, `card`, `pill`,
  `stack_card`) takes that lens: with it the card draws the lens, rounded
  as the card, and no fill, edge or shadow of its own (the lens is all
  three); without it the opaque card as before.
- The lens sits on the slot's card, outside the slot's cached view, and on
  the conversation's two cards, which sit outside the history's paint
  layer, so each reads the window's backdrop rather than degrading to its
  fallback quad.
- Chat rows rest on `list_row_fill`: transparent over glass, so the card
  shows through, the sidebar fill otherwise. Hover and selection keep
  their own fills.
- The chat list's own glass header (#1497) floats only when the list has
  no glass card (`chat_list_bar_glass`), so glass never sits over glass.

Deviation from glass-effects.md's budget: the always-visible glass is the
list card and the two conversation cards, well over 15% of the window,
because the owner asked for exactly these surfaces. The lens count and
bands stay inside the budget (scene test
`floating_cards_are_glass_only_while_the_look_is_glass`).

Cost, measured 2026-11-25 on Idan's M-series Mac with #1497's method
(release build with `demo-capture,frame-profiler`, `QUILL_GPU_PROBE=1`,
1200 by 800 at 2x, `ready-unread-landing` scrolled up and down under the
cards, first 30 frames skipped, ABBA twice). Relative numbers for this
decision only:

| Layout | GPU p50 off | GPU p50 on | p99 off | p99 on | lenses on |
| --- | --- | --- | --- | --- | --- |
| Floating | 0.98 to 1.00 ms | 3.26 to 3.38 ms (+2.3 ms) | 5.0 to 5.4 ms | 9.0 to 9.9 ms | 6 |
| Classic (same build) | 0.53 ms | 1.70 to 1.75 ms (+1.2 ms) | 3.5 to 3.8 ms | 7.3 to 7.5 ms | 4 |

CPU draw medians stayed at 5.5 to 6.2 ms in every run, on or off.
Floating glass is over #1497's scroll budget (+1.0 ms GPU, p99 under
8.3 ms): every lens re-blurs its backdrop each frame while the history
moves, and the list card is a tall lens whose backdrop (the wallpaper)
does not change. This is open for the owner; the remedies, in order, are
a renderer that reuses a lens's blur while its backdrop region is
unchanged, then fewer lenses (the list card solid while the history
scrolls), then solid cards on battery.

## List details and row sizes (step 9)

- **Row sizes.** `quill::density::STANDARD_WEBK` is Standard with Web K's
  72 px rows, 54 px photos and 44 px search field;
  `Density::metrics_in(rows)` picks it for `RowScale::WebK` and keeps
  Compact as it is. `QuillApp::row_density()` reads the window's row scale
  (`chrome_metrics().rows`), and only the chat list's rows, photos and
  search field use it; `density()` stays app-wide (the title bar, the
  sidebar insets), which keeps it out of per-window state
  (`check-window-scope.py`). The list's items are built on every render,
  so a window that crosses 600 px gets the other heights on its next
  frame; its scroll offset stays in pixels.
- **The open row** (`chat_row/row_ink.rs`). `RowInk::of(open,
  SelectedRow, ..)` holds every colour a row draws: fill, the open row's
  hover and pressed fills, title, secondary text, the sender's name, the
  typing line, the online dot's ring and the counters' ink. `Tint`
  returns the expressions the row used before (a test compares them).
  `Solid` fills the row with the accent and draws on it in the accent's
  text colour; the counters invert (`BadgeInk::OnFill`). Some accents are
  too light for white text (Quill Light's #2f81f7 is 3.7:1), so the solid
  fill moves in lightness, hue kept, until its text keeps 5.4:1 and the
  softened secondary text 4.5:1; a test holds that for every built-in
  theme and accent.
- **Search field and folder tabs** (`chrome/list_parts.rs`): the field is
  a pill and the folder tabs take the kit's Pill variant in Floating.
- Topic rows inside a forum keep the tint, and the select-mode check
  circle on the open row keeps its accent ring (it shows on the solid
  fill, but faintly): both are left as they are.
- Compact mode was not captured (no demo switch for it); the table test
  proves it is unchanged.

## Menus take Web K's shape

Owner note: context menus should have rounded corners like Web K. In
Floating every menu, dropdown and popover takes `.btn-menu`'s shape from
tweb (`scss/partials/_button.scss`): a 16px radius
(`$border-radius-medium`) and a `0 0 10px rgba(0, 0, 0, .15)` shadow, the
same in night mode, with its rows clipped to the corners
(`clip_rounded`). Classic menus keep the kit's radius and ring-and-shadow
edge, byte for byte (Classic captures identical, the chat menu included).

Two surfaces draw menus. Quill's own (`MenuSurface::menu_surface`) and
the kit's (`popover_style`, used by the popup menu, select, combobox,
popover, date picker and completion list). The kit gets a hook,
`PopoverShapeHook` (`third_party/gpui-component/src/popover_shape.rs`,
recorded in `QUILL-CHANGES.md`), and Quill answers it from
`chrome::menu_shape`.

Which window's shape: a menu is built while its window draws, and every
window notes its width before it builds anything
(`QuillApp::note_window_width`, main and chat windows). That call also
writes the metrics of the window being drawn
(`DrawnWindow(&'static ChromeMetrics)`), and both surfaces read it. A
chat window under 601 wide draws Classic menus while the main window
draws Floating ones. Glass menus take the radius and the clip; the lens
keeps its own rim and shadow.

## Menus pop (step 11a)

Floating menus open as `.btn-menu` does: from 0.8 scale and clear to the
menu itself over 200 ms on `cubic-bezier(.4, 0, .2, 1)`
(`--btn-menu-transition`), drawn through a snapshot
(`with_snapshot_animation`) that lives only for the run. Quill's menus
(`chrome::menu_pop`, wrapped by `DismissLayer::anchored`) grow from the
corner nearest where they opened, as tweb's `positionMenu` picks its
`top-left`/`bottom-right` class: a menu pushed left by the window's edge
grows from its right side. The kit's dropdown (`dropdown_popup`) pops
from its top left. The capture keeps 12px around the menu for the
shadow. Classic menus never take a snapshot and open as before (the kit's
fade and slide, Quill's at once).

Deviations:

- Glass menus don't pop: a lens inside a capture stays live and is not
  moved, so the glass would stand still while its rows scaled. They open
  as before.
- No fallback motion where the window has no snapshots
  (`QUILL_SNAPSHOTS=off`, an old GPU): Quill's menus appear at once, the
  kit's dropdown keeps its fade and slide.
- The kit's dropdown always grows from its top left; it does not know
  which way it was flipped.
- No backing colour in the capture: the message menu's strip and panel
  have a gap between them that a backing would fill.

## Chat switch and folder pages (steps 11b, 11c)

Both are one page giving way to the next, so they share one stage,
`page_cross::PageStage<K>`: an enum (`Idle`, `Crossing`) that owns its two
pictures, the page it drew last and where. The stage sits where the page
draws and starts a crossing when the page it is about to draw is not the
one it drew last (the plan's render-time variant: the many places that
open a chat or a folder stay as they are). The old page is the last
frame's pixels (`snapshot_last_frame`), the new one is drawn live through
a snapshot the stage owns, so a page that is still loading is captured
again as it changes. Pointer input to the region waits for the run (an
occluding layer while `Crossing`). Exits: the run settles; another page
replaces the run and its pictures; no page (a chat closed); Interface
animations off (never entered); the layout or the width band leaves
Floating (`cut`). Where the window has no snapshots
(`QUILL_SNAPSHOTS=off`, an old GPU) the new page fades in and the old one
is not drawn. Both sides read `motion::elapsed_since`, so
`QUILL_MOTION_HOLD_MS` holds a capture mid-run.

The chat switch (`app_slice::chat_switch`, W11) is the main window's
conversation slot, keyed by the open chat: the new chat arrives 200 px
from the right and fades in over 200 ms on the standard curve.

The folder pages (`chat_list::folder_slide`, W7) are the chat rows keyed by
the folder tab: the old page leaves and the new one arrives a page's width
to either side by tab order (Web K's slider: no fade), 200 ms, clipped to
the rows inside the list card.

Deviations:

- The old chat fades out where it is instead of leaving 200 px to the
  left: its picture holds the window's wallpaper behind the chat, and
  moving it showed a seam against the live wallpaper. The new chat still
  travels 200 px.
- The old chat's picture is taken at half scale. Two pane-sized pictures
  pass the window's snapshot budget (1.25 windows); past it the new
  chat's capture is refused and it painted in place. The old one only
  fades, so half scale does not show.
- Every chat switch runs forward. The plan's backward run belongs to the
  header's back action, which in the wide layout does not open another
  chat.
- Chat windows hold one chat each and never cross.
- A pane with glass in it cannot be taken from the last frame; then only
  the new chat moves in.
- Empty folder pages (the empty state, the loading skeleton) are drawn
  outside the stage and appear at once.

## Composer height (step 11d)

When the draft gains or loses a line, the rows under the composer pill
follow its new height over 250 ms on the standard curve (Web K's W12)
instead of jumping. The rows are laid out against the new height at
once, so the list keeps its place at the bottom, and drawn lower by what
the glide has left, as Web K moves its bubbles with `translateY`:
`WindowSlices::composer_glide` (`ComposerGlide`, a `Glide` in a `Cell`)
follows the measured height where the floating column draws, and the
history's box takes the offset; the history's cached slice replays, so a
glide frame costs no history render. A bar sliding open over the composer
(reply, edit, forward) already moves the measured height frame by frame,
so for 50 ms after a bar moved the rows follow at once; the first measure
and Interface animations off snap too. Classic's `composer_ms` is 0.

Deviations:

- The pill follows its content at once; only the rows glide. The pill's
  fill is drawn by the composer slice around its content, so gliding its
  box would change nothing on screen.
- A first version glided the rows' bottom padding instead. The list then
  lost its place at the bottom: each frame's small padding change moved
  the content under a list that no longer followed its tail.

## Title strip (step 10)

`frames::title_strip` takes the strip's lower line away in Floating and
leaves Classic's alone. It runs after the line is set: on Quill's own
frame the kit's `TitleBar` draws its border and then the caller's style,
and on the system frame the slim row sets `border_b_1` first. The strip
keeps its opaque fill on every platform, so the caption, the menu bar,
the update pill and Search stay legible over any wallpaper. The bar
keeps its height: the cards and the wallpaper do not move.

`FrameTitleBar::strip` takes the window's strip. The main window passes
`QuillApp::title_strip()`; a chat window reads it while its own state is
in place (the same update that notes its width), so a narrow chat window
next to a wide main window keeps its line.

Proof: the light main window differs from the step 9 build only in the
two device-pixel rows of the line (66 and 67 at 2x) and in the caption
and Search, which sit half a point lower: centred in the whole strip now
that no line takes its last point. The chat window capture shows no line. The system frame
cannot be drawn on macOS (`window_frame::supported`), so
`window_frame_tests` checks the style both frames end with: Classic keeps
1px, Floating ends at 0.

## Edges (step 12)

Checked on macOS (Metal) with the photo wallpaper behind every Floating
capture unless the case is the plain backdrop:

- **Bands.** 600, 601, 925 and 926 wide. Floating at 600 is pixel
  identical to Classic at 600 with the same wallpaper. 601 to 925 is the
  drawer band, 926 docks the list.
- **High contrast.** Every card and pill gets the 1pt theme hairline
  (measured: the list card's edge is two device pixels of the border
  colour) and no shadow. The demos could not show this before:
  `QUILL_DEMO_THEME=high-contrast` only re-rooted the chat palette, so
  `demo_look` now also picks the High Contrast theme for any demo.
- **Battery saving.** `battery_saving_flattens_floating`: with the system
  forcing power saving the cards are flat, and the shadow returns after.
  Motion snapping waits for step 11.
- **Plain backdrop.** Light and dark with no wallpaper at 1440: the cards
  sit on the theme background.
- **Glass on.** `ready-glass` and `ready-chats` with a chat menu open:
  glass cards, glass menu with Web K's radius over them.
- **Snapshots off.** `QUILL_SNAPSHOTS=off` showcase captures, light and
  dark: nothing in steps 3 to 10 uses snapshots yet.
- **RTL.** `ready-rtl-text` at 1440 and in the drawer band (800), light
  and dark: Hebrew and Arabic bubbles, header and composer pills read
  correctly.
- **Interface zoom 150%.** The bands move with zoom: at 900 px the window
  is 600 interface px and Classic (one column); at 901 it is Floating; at
  1386 (924 interface px) the list is a drawer; at 1390 it docks.

Not checked: chat windows at exactly 700 and 560 wide (the demo opens a
chat window at its default size, which is Floating; under 601 a chat
window answers Classic from its own width through the same
`chrome_metrics`, covered by `a_window_of_600_or_under_draws_classic`).
Windows and Linux GPUs, and the system frame, were not run.

## Appearance review round

The owner's review of the first full build (captures over three
wallpapers, light and dark) found these, all fixed in Floating only:

- **Title strip.** The strip keeps its height and controls but draws no
  fill or line; the window's wallpaper runs under it. The history is
  clipped at the header card's top, so no row shows in the gap above it
  (Web K stops its bubbles under the topbar). The wallpaper is now one
  layer at the window root (`QuillApp::window_backdrop_layer`), not a fill
  on the shell, so the strip and Settings stand on it too.
- **Glass cards.** Cards take their own recipe, `GlassRole::Card`, instead
  of the menus' `Panel`: frosted to the edge (no clear rim band), barely
  bent or lit, a wider blur and the whole frost floor. The slab's rim
  band, saturated twice, read as a warm glowing frame on a card the
  window's height; its "studio" rim light drew a bright streak halfway
  down the list card's right edge; a picture under the header card showed
  as a muddy block. The Archived chats row is clear on glass, as every
  chat row is.
- **Dark popovers.** Menus and kit popovers (the folder dropdown) take a
  fill lifted above the card's and a light 1 px edge in dark
  (`menu_shape::dark_surface`, carried by the kit's `PopoverShape::fill`
  and `edge`), and a darker shadow. The header ⋮ menu opens from the
  button's right edge, so it stays inside the header card, and its entries
  carry icons as the row and message menus do.
- **The drawer.** A scrim (black, 32 % once in) dims the chat behind the
  held drawer, and the drawer card casts a wider, darker shadow than a
  card. The chat header's title stays under the drawer, dimmed, as Web K's
  topbar does under its sliding column: the column is narrower than the
  chat column plus a drawer at these widths.
- **Pinned bar.** Its accent stripe starts at the header's inset (the
  hairline's start), with 4 pt more room above the card's lower corner.
- **Menu target.** While a context menu is open, its chat row keeps the
  hover fill and its message a light selection tint (Web K marks both).
- **Dark cards on a dark wallpaper.** Dark cards draw a 1 px white edge at
  12 %, so a near-black card holds its shape on a near-black pattern.
- **Onboarding picker.** Each option is one outlined tile with its radio
  inside; the two tiles match in height; the onboarding card is wider with
  larger previews; the wallpaper note is centred over its button. The
  previews draw a gradient wallpaper (behind the history in Classic,
  behind everything in Floating) and dark cards keep an edge. The Settings
  card shares the picker, so its tiles changed too.
- **Settings.** Settings notes the window's width as the chat columns do,
  so it follows the layout: in Floating its list and page are cards over
  the window's wallpaper. The Glass effects description says the Floating
  cards turn to glass too.
- **Chat switch.** While the bars float, only the history crosses
  (`floating_column` draws the stage around it); the header and composer
  pills stay where they are and show the new chat at once. The old
  picture still holds the old pills, under the live ones, so where the
  new header is shorter (no pinned bar) the old bar fades out below it.
- **Classic glass menu.** The blue blob in a glass message menu over an
  outgoing bubble and the light seam at its right edge are on main too
  (same capture on a main build): the menu recipe's rim bends the bubble
  in. Classic is left as it is.
