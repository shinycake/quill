//! The active emoji set's sprites (parity:emoji-set-style): which emoji
//! of a text the set draws, and the picture of each.
//!
//! The atlas lives on the UI thread (one per thread, so GPUI tests stay
//! apart). A sheet is decoded the first time one of its emoji is asked
//! for, on the background executor; until then that emoji keeps its
//! system glyph, and once the sheet is in every window redraws. A decoded
//! sheet is kept as one small image per emoji, so only what is painted
//! goes to the GPU atlas.
#![deny(clippy::indexing_slicing)]

use std::cell::RefCell;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::*;
use quill::emoji_sets_style::catalog::EmojiSetId;
use quill::emoji_sets_style::pack::{Cell, SpritePack, sheet_path};
use smallvec::SmallVec;

enum Sheet {
    Missing,
    Loading,
    Ready(Rc<Vec<Option<Arc<RenderImage>>>>),
    Failed,
}

struct Atlas {
    id: EmojiSetId,
    dir: PathBuf,
    pack: Arc<SpritePack>,
    sheets: RefCell<Vec<Sheet>>,
    /// Wakes the app when a sheet lands.
    app: AsyncApp,
}

thread_local! {
    static ACTIVE: RefCell<Option<Rc<Atlas>>> = const { RefCell::new(None) };
}

fn active() -> Option<Rc<Atlas>> {
    ACTIVE.with(|a| a.borrow().clone())
}

/// The set in use (the system font when none is installed).
pub(super) fn active_set() -> EmojiSetId {
    active().map_or(EmojiSetId::System, |a| a.id)
}

/// Draw emoji from `pack` (unpacked in `dir`), or with the system font for
/// `None`. Pictures of the previous set leave the GPU atlas.
pub(super) fn install(set: Option<(EmojiSetId, PathBuf, Arc<SpritePack>)>, cx: &App) {
    let next = set.map(|(id, dir, pack)| {
        let sheets = (0..pack.sheets).map(|_| Sheet::Missing).collect();
        Rc::new(Atlas {
            id,
            dir,
            pack,
            sheets: RefCell::new(sheets),
            app: cx.to_async(),
        })
    });
    let previous = ACTIVE.with(|a| a.replace(next));
    if let Some(previous) = previous {
        let sheets = std::mem::take(&mut *previous.sheets.borrow_mut());
        let images: Vec<Arc<RenderImage>> = sheets
            .into_iter()
            .filter_map(|sheet| match sheet {
                Sheet::Ready(cells) => Some(cells),
                _ => None,
            })
            .flat_map(|cells| cells.iter().flatten().cloned().collect::<Vec<_>>())
            .collect();
        super::image_budget::retire_all(images);
    }
}

/// Start decoding `sheet` if nothing has yet.
fn request(atlas: &Rc<Atlas>, sheet: u16) {
    {
        let mut sheets = atlas.sheets.borrow_mut();
        match sheets.get_mut(usize::from(sheet)) {
            Some(slot @ Sheet::Missing) => *slot = Sheet::Loading,
            _ => return,
        }
    }
    let path = sheet_path(&atlas.dir, sheet);
    let pack = atlas.pack.clone();
    let owner = Rc::downgrade(atlas);
    atlas
        .app
        .spawn(async move |cx| {
            let decoded = cx
                .background_spawn(async move { decode_sheet(&path, &pack) })
                .await;
            let Some(atlas) = owner
                .upgrade()
                .filter(|a| active().is_some_and(|now| Rc::ptr_eq(a, &now)))
            else {
                // The set changed meanwhile.
                if let Some(cells) = decoded {
                    super::image_budget::retire_all(cells.into_iter().flatten());
                }
                return;
            };
            if let Some(slot) = atlas.sheets.borrow_mut().get_mut(usize::from(sheet)) {
                *slot = match decoded {
                    Some(cells) => Sheet::Ready(Rc::new(cells)),
                    None => Sheet::Failed,
                };
            }
            cx.update(|cx| cx.refresh_windows());
        })
        .detach();
}

/// One image per cell of a sheet (empty cells are `None`), BGRA as GPUI
/// draws them.
fn decode_sheet(
    path: &std::path::Path,
    pack: &SpritePack,
) -> Option<Vec<Option<Arc<RenderImage>>>> {
    let bytes = std::fs::read(path).ok()?;
    let sheet = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
        .ok()?
        .to_rgba8();
    let edge = pack.cell;
    let count = pack.columns.saturating_mul(pack.rows);
    let mut cells = Vec::with_capacity(count as usize);
    for index in 0..count {
        let (x, y) = pack.cell_origin(u16::try_from(index).ok()?);
        if x.saturating_add(edge) > sheet.width() || y.saturating_add(edge) > sheet.height() {
            cells.push(None);
            continue;
        }
        let mut cell = image::imageops::crop_imm(&sheet, x, y, edge, edge).to_image();
        if cell.pixels().all(|p| p.0[3] == 0) {
            cells.push(None);
            continue;
        }
        for pixel in cell.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        cells.push(Some(Arc::new(RenderImage::new(SmallVec::from_buf([
            image::Frame::new(cell),
        ])))));
    }
    Some(cells)
}

fn image_of(atlas: &Rc<Atlas>, cell: Cell) -> Option<Arc<RenderImage>> {
    let found = match atlas.sheets.borrow().get(usize::from(cell.sheet)) {
        Some(Sheet::Ready(cells)) => return cells.get(usize::from(cell.index)).cloned().flatten(),
        Some(Sheet::Missing) => true,
        _ => false,
    };
    if found {
        request(atlas, cell.sheet);
    }
    None
}

/// The picture of one emoji (a grapheme) in the active set, if it draws it
/// and its sheet is decoded.
pub(super) fn sprite(emoji: &str) -> Option<Arc<RenderImage>> {
    let atlas = active()?;
    let cell = quill::emoji_sets_style::split::grapheme_cell(&atlas.pack, emoji)?;
    image_of(&atlas, cell)
}

/// A stretch of text: plain characters, or one emoji's picture.
pub(super) struct SpritePiece {
    pub(super) range: Range<usize>,
    pub(super) image: Option<Arc<RenderImage>>,
}

/// `text` cut at the emoji the active set draws. `None` when the system
/// font draws everything (no set, or no such emoji): callers keep their
/// usual text path, so the default costs nothing.
pub(super) fn pieces(text: &str) -> Option<Vec<SpritePiece>> {
    let atlas = active()?;
    let pieces = quill::emoji_sets_style::split::pieces(&atlas.pack, text)?;
    let mut out: Vec<SpritePiece> = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let image = piece.cell.and_then(|cell| image_of(&atlas, cell));
        // An emoji whose sheet is still decoding reads as text for now.
        match out.last_mut() {
            Some(last) if image.is_none() && last.image.is_none() => {
                last.range.end = piece.range.end
            }
            _ => out.push(SpritePiece {
                range: piece.range,
                image,
            }),
        }
    }
    out.iter().any(|p| p.image.is_some()).then_some(out)
}

/// The edge of an emoji picture next to text of `font` size (a glyph's
/// box is a little larger than the font size).
pub(super) fn edge_for(font: Pixels) -> Pixels {
    font * 1.15
}

/// One emoji as a picture `size` wide, when the active set draws it.
pub(super) fn glyph(emoji: &str, size: Pixels) -> Option<AnyElement> {
    let image = sprite(emoji)?;
    Some(
        img(ImageSource::Render(image))
            .size(size)
            .flex_none()
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
    )
}

/// A one-line label whose emoji are the active set's pictures: the text
/// stretches flow in a row and the last one ends in an ellipsis. `None`
/// when there is nothing to draw (the caller keeps its own text), or for
/// right-to-left text, which keeps the bidirectional line.
pub(super) fn one_line(text: &str, font: Pixels) -> Option<AnyElement> {
    let shown = quill::display_text::filtered(text);
    let text = shown.as_ref();
    if quill::text::has_rtl_text(text) {
        return None;
    }
    let pieces = pieces(text)?;
    let last_text = pieces.iter().rposition(|p| p.image.is_none());
    let edge = edge_for(font);
    let mut line = div()
        .flex()
        .flex_row()
        .items_center()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap();
    for (position, piece) in pieces.into_iter().enumerate() {
        line = line.child(match piece.image {
            Some(image) => img(ImageSource::Render(image))
                .size(edge)
                .flex_none()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => {
                let text = text.get(piece.range).unwrap_or_default().to_string();
                let part = div().whitespace_nowrap().child(text);
                if Some(position) == last_text {
                    part.min_w_0().flex_shrink(1.).truncate().into_any_element()
                } else {
                    part.flex_none().into_any_element()
                }
            }
        });
    }
    Some(line.into_any_element())
}
