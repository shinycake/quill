# Photo editor before sending (tdesktop)

Telegram Desktop lets you edit a photo in the send box (pencil on the
preview): crop with handles, rotate, flip, and paint (colors, brush
sizes, undo); stickers can be added too. Quill could only send the file
as picked.

Now a pencil on each photo tile opens a full-window editor:
- **Crop:** drag the corner handles or move the rectangle. The area
  outside is shaded (in Draw mode too, so you see what will be sent).
  Rotate (a quarter counterclockwise), Flip and Reset.
- **Draw:** freehand strokes in tdesktop's eight colors, three brush
  sizes, Undo.
- **Done** renders at full resolution into a new PNG and swaps it into
  the attachment; Cancel discards the edit.

Implementation:
- `ui/photo_edit.rs` keeps the geometry normalized (0..1) to the working
  image. Rotation and flip transform the pixels, the crop rectangle and
  existing strokes together. `render` stamps anti-aliased discs along
  each stroke and then crops. Unit tests cover crop math, rotation and
  rendering.
- `ui/photo_editor.rs` paints a downscaled preview, the crop shade and
  handles, and the strokes on a GPUI canvas, mapping the pointer through
  the last painted image bounds. GPUI's `img` can't rotate, hence the
  pixel-level transforms.

Stickers and emoji on the photo are the next slice.

Verified live in Saved Messages: cropped and drew on a picture, then sent
it.
