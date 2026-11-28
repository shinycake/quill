# The selection check

One round check marks a picked item everywhere Quill lets you pick some
of several: history rows, the photos and files of an album, shared media,
and the tiles of the story album picker. It is drawn by
`src/ui/select_check.rs`.

Before this there were three looks: a green check on rows and album
tiles, an accent one in the story picker, and another in shared media.
The green one was hard to read. Green sat next to the blue selection tint and
blue bubbles, the tick was a 12 px icon with a thin stroke, the empty
state was a grey dot, and a picked photo was washed in the accent at 45 %,
which turned its colours grey.

## What it looks like

- Empty is a ring. Picked is a disc in the chat's accent with a white
  tick. The accent is the chat theme's (`chat_palette::themed`), so the
  check matches the bubbles and the selection tint.
- It is 22 px across, 2 px larger than tdesktop's `msgSelectionCheck`.
  The tick is a 2 px path with round caps and joins, on the proportions of
  Telegram iOS's `CheckNode`.
- What the check is drawn over decides how it keeps its contrast
  (`Ground`):
  - `Scene`, a wallpaper, photo or video: a white ring over a soft dark
    halo, the disc inside the ring.
  - `Surface`, a plain theme background or an incoming bubble: a grey
    ring, and an accent disc that covers it.
  - `Accent`, an outgoing bubble: a white ring, and a white disc with an
    accent tick.
- A picked photo or video keeps its own colours. It gets a 3 px accent
  ring inside its edge with a white hairline inside that, so the ring
  shows on a picture of the accent's colour (`picked_tile`).
- A picked file of a file album gets a light wash behind its row
  (`picked_file`).

## Where the row check sits

tdesktop slides an outgoing bubble `msgSelectionOffset` (30 px) aside and
draws the check near the edge. Quill's check sat 11.5 px from the edge, so
a 20 px check reached 1.5 px into the bubble's corner. The gutter is now
34 px and the check is centred in it, 6 px clear on both sides
(`chat_wide::MSG_SELECTION_OFFSET`, `MSG_SELECTION_CHECK_SIZE`).

The check is level with a one-line bubble, and with the last line of a
taller row (`motion::CHECK_LANE`).

## Motion

The run is tdesktop's 160 ms. Along it the disc closes in from the centre
over the first 60 %, the tick draws from its short stroke on, and the
whole check swells 8 % at the midpoint (`select_check::Frame`). Album
tiles animate like rows (`motion::CheckOf`). Inactive windows and reduced
motion show the end state at once, as before.

## Demos

- `ready-selection`
- `ready-albums` with `QUILL_DEMO_ALBUM_FORWARD=select`
- `ready-file-album` with `QUILL_DEMO_FILE_ALBUM=select` (incoming) or
  `=own-select` (outgoing)
- `ready-story-album-picker`
- `ready-shared-media` with `QUILL_DEMO_SHARED_MEDIA_SELECT=2` and
  `QUILL_DEMO_SHARED_MEDIA_TAB=photos` or `=files`

Not done: the forward box keeps its own small badge on the chat photo. No
demo reaches the wide chat layout; a test in `album_select_tests.rs`
covers the check's place there.
