# Pictures sent as files

Parity items `parity:media-image-files`, `parity:media-image-formats` and
`parity:media-image-file-thumbnail-send`. Slice A of HDR photo support; the
HDR viewer (slice B) builds on the decoder added here.

## What tdesktop does

- A document whose message has a thumbnail is drawn with the thumbed file
  layout (`HistoryView::Document`, `st::msgFileThumbLayout`): a 72 px
  picture, then the name, the size and one link: "Open with" once the file
  is downloaded, "Download" before that, "Cancel" while it downloads.
  Clicking the picture cancels, opens or downloads.
- Opening a document goes through `ResolveDocument`. A file opens in the
  media viewer when `CheckImageOpenInApp` passes: it is smaller than
  `kReadBytesLimit` (64 MB, lib_ui `ui/image/image_prepare.h`), named or typed as an image
  (`Core::FileIsImage`, `DetectNameType`'s extension list, or the `image/`
  MIME type when the file has no name), and Qt can read it. The picture
  area is capped at `kReadMaxArea` (12032 × 9024).
- The viewer opened on a document pages over the chat's files
  (`SharedMediaType::File`). Pictures show as pictures, other files as a
  document card with download and open actions.
- Sending a file: `PrepareFileThumbnail` scales the picture to 320 px on
  its long side and saves a quality-87 JPEG. `FileThumbnailUploadRequired`
  uploads it unless the server makes its own: a JPEG, PNG, GIF or WebP
  file (or an MP4 video) of at most 5 MB. The file goes as it is.

## What Quill does

- `src/ui/message_media/document_thumbed.rs` draws the thumbed row when
  TDLib gives the document a JPEG, PNG or WebP thumbnail. Until the
  thumbnail has downloaded it falls back to the minithumbnail. Thumbnails
  of documents now download with the rest of the chat's thumbnails.
- `src/media_viewer/files.rs` (core): `is_image_file` (tdesktop's list),
  `opens_as_picture`, and the viewer items of a chat's files (kinds
  `ImageFile` and `File`). `src/ui/media_viewer/files.rs` opens a document
  the way `ResolveDocument` does and draws the file card. The Shared Media
  Files tab opens pictures in the same viewer.
- `src/ui/image_decode/` is the one decoder for chat pictures, the viewer,
  Copy Image and rotation:
  - First it sniffs the content (ISO-BMFF brands, the JPEG XL signature,
    SVG text).
  - It enforces the byte and area limits and decodes at the requested
    size.
  - The `image` crate reads the common rasters. Their ICC profile goes to
    sRGB with `moxcms`, and EXIF orientation is applied.
  - `jxl-oxide` reads JPEG XL everywhere, converting to sRGB itself.
  - `resvg` draws SVG.
  - On macOS, ImageIO reads HEIC, AVIF, JPEG XL, camera RAW, PSD and the
    rest. The picture is drawn into an sRGB bitmap, so ColorSync handles
    the profile, and the thumbnail call applies orientation. Only pre-14
    symbols are used, so Quill still launches on older macOS.
  - On Windows, WIC reads HEIC and AVIF through Microsoft's HEIF/AV1 codec
    extensions when they are installed. It converts to 32bppBGRA, then to
    sRGB with a WIC colour transform, and applies the
    `System.Photo.Orientation` metadata.
  - On Linux (and as the macOS fallback), the system's libheif is loaded
    at run time if it is installed. An ICC profile or nclx colour
    (Display P3, BT.2020 PQ/HLG) is converted to sRGB with `moxcms`.
  - Third-party decoders run inside `catch_unwind`. A failure falls back
    to the file card, never a crash.
- Sending (`src/ui/send_box_thumbnails.rs`): when a picture goes into the
  tray as a file and the server will not make its thumbnail, Quill decodes
  the picture in the background. It writes a 320 px, quality-87 JPEG
  (transparent pixels over white) and passes it as
  `inputDocument.thumbnail`. Sending waits for it, as it already waits for
  the video sound check. The file is sent unchanged, so HDR data and
  metadata survive. The tray tile shows that thumbnail for pictures it
  can't read itself.

## Deviations

- "Open with" opens the file in the system's default app. Quill has no
  "Open With" app chooser.
- In the chat, tdesktop shows a picture sent as a file as a file row with
  its thumbnail. Quill shows it as a photo instead (owner's call): framed
  like a compressed photo, with a pill in the top-left corner (a file glyph
  and the format, `HEIC`) so it reads as a file, and the name and size
  along the bottom (`src/ui/message_media/picture_file.rs`). Files without
  a thumbnail, or of 64 MB and more, keep the thumbed or plain file row
  (`quill::media_viewer::shows_as_photo`). A file that fails to decode
  keeps showing its thumbnail. Picture files up to 10 MB download on their
  own under the photo auto-download setting.
- The upload rule checks the file extension, where tdesktop checks the
  MIME type it detected.
- HEIC/AVIF on Windows need Microsoft's HEIF and AV1 codec extensions
  (Windows 11 ships them on most installs). On Linux they need libheif
  from the distribution. Without them the viewer shows the file card, with
  "Open in another app". JPEG XL works everywhere.
- HDR pictures show their SDR rendition here. Slice B adds HDR output
  (see `quill-tools/designs/hdr-viewer.md`).
- The viewer sizes a picture file from its thumbnail's aspect until the
  decode finishes. TDLib does not report a document's pixel size.

## Tests

- `ui::image_decode::tests`:
  - sniffing and the decode-size fit;
  - EXIF orientation;
  - HEIC and AVIF through ImageIO, and libheif when installed;
  - JPEG XL through both paths;
  - P3 → sRGB;
  - garbage, truncated and oversized input;
  - SVG and unpremultiply.
  The fixtures in `tests/fixtures/images/` are 32 × 16 pictures, red on
  the left and blue on the right.
- `media_viewer::files::tests`: the extension and MIME rules, and the
  viewer items.
- `ui::media_viewer::files::tests` (UI): a remote HEIC downloads on click;
  a local one opens in the viewer as "File 1 of 3"; the text file is a
  card.
- Envelope: a document keeps a JPEG thumbnail and drops a TGS one.
- Requests: `inputDocument.thumbnail` shape. Composer: when to upload a
  thumbnail. UI: a JPEG XL file gets a 32 × 16 JPEG thumbnail, and a small
  PNG gets none.
- Screenshot demos `ready-image-files` and `ready-image-file-viewer`.
  Slice B reuses them for HDR.
