# GIF panel: justified grid, playing previews, search as you type

## What was wrong

Most saved GIFs showed as grey boxes with their file name in large text
spilling over the neighbouring tiles; only GIFs with a JPEG thumbnail had a
picture. A "GIF search: none" label sat under the tabs, and the grid was a
fixed 96x72 three-column block with an empty strip on the right.

## Cause

Telegram sends most GIFs with a `thumbnailFormatMpeg4` thumbnail (a short
preview loop; the TDLib log has them next to the JPEG ones). The panel
passed every thumbnail to `img()`, which can't decode an MP4, so the
fallback drew `file_name`. The inline `@gif` results had the same bug and
were fixed in `inline-gif-loading.md`; the GIF panel never got that fix.

## What changed

- `quill::gif_grid::layout_gifs` packs the panel's GIFs with the inline
  results' mosaic (`inline_grid::layout_mosaic`, tdesktop's
  `AbstractMosaicLayout`): 96 pt rows, at most five per row, full rows
  justified to the panel width. A GIF without a size takes its thumbnail's
  shape or a square instead of being dropped.
- Each tile draws a still (the JPEG thumbnail, else the blurred
  minithumbnail, else a kit `Skeleton`) and plays over it, muted and
  looped, the GIF itself once downloaded or else its MP4 preview loop
  (tdesktop `VideoPreviewState`). No file names.
- `Session::gif_panel_file_ids_to_download`: every still, then the preview
  loops of the first 24 GIFs, then the full clips (5 MB or less) of the
  first 24 GIFs that have no preview, at the inline results' priority.
  Autoplay off or data saver: stills only.
- Clip players: `inline_players::Panel` gives the inline results and the
  GIF panel their own player keys and render passes, so one panel's pass
  never reaps the other's players. Leaving the GIFs tab closes the GIF
  state, which stops its players and downloads.
- Search: a kit `Input` with a search icon, a clear button and a spinner
  while loading; typing searches after 400 ms (tdesktop
  `kSearchRequestDelay`), an empty field shows the saved GIFs. Search
  results page in when the scroll gets within a viewport of the end
  (the "More GIFs" button is gone).
- Footer: Saved, Trending and the provider's emoji as section buttons,
  the same strip as the emoji and sticker tabs (`section_chip`). The
  selected one is derived from the panel state.
- The "GIF search: {provider}" label and the standalone panel header are
  gone; Esc on the GIFs tab closes the media panel.

## Not done

- Hover delete button on saved GIFs (tdesktop draws one); the context menu
  keeps Save / Remove.
- Downloads are by position (first 24), not by what is scrolled into view.

## Verification

`ready-gifs` demo now has nine saved GIFs of mixed shapes, most with MP4
previews (fixtures `demo-gif-loop-*.mp4`, `demo-gif-still-*.jpg`, made
from the showcase photos with ffmpeg). Captured before and after in light
and dark, the Trending section, and Esc. Not covered: real TDLib data and
search results, which need a live account.
