//! The picture on a wallpaper or theme link preview (tdesktop
//! `HistoryView::ThemeDocument`, the attach of `WebPageType::WallPaper` and
//! `WebPageType::Theme`), and the emoji beside an AI compose style link
//! (`WebPageType::ComposeAiTone`).
//!
//! A wallpaper is drawn the way the chat draws it, from the document's
//! small preview (a pattern from its file, a fill from its colors) in a
//! box 160 px wide and as tall as the preview's proportions, between 100
//! and 240 px (`maxWallPaperWidth`, `minPhotoSize`, `maxWallPaperHeight`).
//! A theme is its document's preview picture in a 272 x 176 box
//! (`historyThemeSize`). When the link has a document, its size sits in a
//! small pill over the top left corner.

use super::*;
use crate::ui::wallpaper::{Wallpaper, background_wallpaper, paint_wallpaper};
use crate::ui::wallpaper_cache::Slot;
use quill::lang::{keys, tr};
use quill::telegram::envelope::{
    BackgroundType, LinkPreview, LinkPreviewKind, PreviewTheme, PreviewWallpaper, format_size_text,
};

/// `st::maxWallPaperWidth`.
const WALLPAPER_WIDTH: f32 = 160.0;
/// `st::minPhotoSize`.
const WALLPAPER_MIN_HEIGHT: f32 = 100.0;
/// `st::maxWallPaperHeight`.
const WALLPAPER_MAX_HEIGHT: f32 = 240.0;
/// `st::historyThemeSize`.
const THEME_SIZE: (f32, f32) = (272.0, 176.0);
/// `kStickerSetLines`: the style emoji is three lines tall.
const STYLE_EMOJI_LINES: f32 = 3.0;
/// `UnitedLineHeight()` over the message font size at the default size.
const LINE_HEIGHT_RATIO: f32 = 19.0 / 14.0;

/// The box of a wallpaper card's picture (`ThemeDocument::countOptimalSize`):
/// a link with no document is the full 160 x 240 box; otherwise the
/// preview's height at 160 px wide, a square when its size is unknown.
pub(in crate::ui) fn wallpaper_box(wallpaper: &PreviewWallpaper) -> (f32, f32) {
    if !wallpaper.has_document {
        return (WALLPAPER_WIDTH, WALLPAPER_MAX_HEIGHT);
    }
    let (tw, th) = if wallpaper.thumb_width > 0 && wallpaper.thumb_height > 0 {
        (wallpaper.thumb_width as f32, wallpaper.thumb_height as f32)
    } else {
        (1.0, 1.0)
    };
    (
        WALLPAPER_WIDTH,
        (WALLPAPER_WIDTH * th / tw).clamp(WALLPAPER_MIN_HEIGHT, WALLPAPER_MAX_HEIGHT),
    )
}

/// What the wallpaper card paints from the files that are on disk: a photo
/// from the document's small preview (never blurred, as in the gallery), a
/// pattern from its file over the fill, a fill from its colors. `None`
/// while a photo is still downloading.
pub(in crate::ui) fn card_wallpaper(
    wallpaper: &PreviewWallpaper,
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
) -> Option<Wallpaper> {
    let background = &wallpaper.background;
    let local = |file: Option<&ParsedFile>| {
        file.and_then(|file| files.get(&file.id.0))
            .and_then(|file| file.usable_path())
            .and_then(|path| sandboxed_display_path(path, media_roots))
    };
    match &background.kind {
        BackgroundType::Wallpaper { .. } => local(background.thumbnail.as_ref())
            .map(|path| Wallpaper::Image {
                path,
                blurred: false,
                tiled: false,
            })
            .or_else(|| {
                local(background.file.as_ref()).map(|path| Wallpaper::Image {
                    path,
                    blurred: false,
                    tiled: false,
                })
            }),
        _ => {
            let path = local(background.file.as_ref());
            background_wallpaper(background, path.as_deref().and_then(|path| path.to_str()))
        }
    }
}

/// The size pill over the picture's top left corner
/// (`_statusText`, `msgDateImgDelta`).
fn size_pill(size: i64) -> Option<AnyElement> {
    (size > 0).then(|| {
        div()
            .absolute()
            .top(px(4.))
            .left(px(4.))
            .px_1()
            .rounded_sm()
            .bg(bg_black())
            .text_xs()
            .text_color(text_on_fill())
            .child(format_size_text(size))
            .into_any_element()
    })
}

fn empty_picture() -> AnyElement {
    div()
        .size_full()
        .bg(fill_muted())
        .flex()
        .items_center()
        .justify_center()
        .child(
            Icon::new(gpui_kit::assets::IconName::Image)
                .size(px(24.))
                .text_color(text_bright()),
        )
        .into_any_element()
}

fn wallpaper_picture(
    row_id: u64,
    wallpaper: &PreviewWallpaper,
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
) -> AnyElement {
    let (width, height) = wallpaper_box(wallpaper);
    let frame = div()
        .id(("link-preview-wallpaper", row_id))
        .relative()
        .flex_none()
        .w(px(width))
        .h(px(height))
        .rounded_md()
        .overflow_hidden()
        .role(Role::Image)
        .aria_label(tr(keys::Q_REST_WALLPAPER_PREVIEW));
    let painted = match card_wallpaper(wallpaper, files, media_roots) {
        Some(paint) => paint_wallpaper(frame, Some(&paint), 0, Slot::Thumb),
        None => frame.child(empty_picture()),
    };
    painted
        .when_some(
            wallpaper
                .has_document
                .then(|| size_pill(wallpaper.size))
                .flatten(),
            |this, pill| this.child(pill),
        )
        .into_any_element()
}

fn theme_picture(
    row_id: u64,
    theme: &PreviewTheme,
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
) -> AnyElement {
    let frame = div()
        .id(("link-preview-theme", row_id))
        .relative()
        .flex_none()
        .w(px(THEME_SIZE.0))
        // A narrow bubble shrinks the picture, keeping its shape.
        .max_w_full()
        .aspect_ratio(THEME_SIZE.0 / THEME_SIZE.1)
        .rounded_md()
        .overflow_hidden()
        .bg(fill_muted())
        .role(Role::Image)
        .aria_label(tr(keys::THEME_PREVIEW_TITLE));
    let path = theme
        .thumbnail
        .as_ref()
        .and_then(|file| files.get(&file.id.0))
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots));
    let frame = match path {
        Some(path) => frame.child(
            img(crate::ui::image_budget::sized_media(
                &path,
                (px(THEME_SIZE.0), px(THEME_SIZE.1)),
                None,
                crate::ui::image_budget::Fit::Cover,
            ))
            .id(("link-preview-theme-img", row_id))
            .absolute()
            .inset_0()
            .size_full()
            .object_fit(ObjectFit::Cover)
            .with_fallback(|| div().size_full().bg(fill_muted()).into_any_element()),
        ),
        None => frame,
    };
    frame
        .when_some(size_pill(theme.size), |this, pill| this.child(pill))
        .into_any_element()
}

/// The style's emoji right of an `addstyle/` card's copy, as a sticker
/// three lines tall (tdesktop draws it with the sticker set preview's
/// `Sticker` view); its fallback emoji until the file is here, a sparkle
/// while the emoji is unresolved. `None` for other cards and for a style
/// without an emoji.
pub(in crate::ui) fn style_emoji(
    row_id: u64,
    preview: &LinkPreview,
    custom_emoji: Option<&quill::emoji::CustomEmojiStickers>,
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
    font: Pixels,
) -> Option<AnyElement> {
    let LinkPreviewKind::TextCompositionStyle { custom_emoji_id } = preview.kind else {
        return None;
    };
    if custom_emoji_id == 0 {
        return None;
    }
    let side = font * LINE_HEIGHT_RATIO * STYLE_EMOJI_LINES;
    let sticker = custom_emoji.and_then(|stickers| stickers.lookup(custom_emoji_id));
    let path = sticker
        .and_then(|sticker| sticker.display_file_id())
        .and_then(|id| files.get(&id.0))
        .and_then(|file| file.usable_path())
        .and_then(|path| sandboxed_display_path(path, media_roots));
    let glyph = match (path, sticker) {
        (Some(path), _) => img(path)
            .size(side)
            .object_fit(ObjectFit::Contain)
            .into_any_element(),
        (None, Some(sticker)) if !sticker.emoji.is_empty() => div()
            .text_size(side * 0.6)
            .child(sticker.emoji.clone())
            .into_any_element(),
        _ => Icon::new(gpui_kit::assets::IconName::Sparkles)
            .size(side * 0.5)
            .text_color(accent())
            .into_any_element(),
    };
    Some(
        div()
            .id(("link-preview-style-emoji", row_id))
            .flex_none()
            .size(side)
            .flex()
            .items_center()
            .justify_center()
            .role(Role::Image)
            .child(glyph)
            .into_any_element(),
    )
}

/// The picture of a wallpaper or theme preview, `None` for other cards.
pub(in crate::ui) fn picture(
    row_id: u64,
    preview: &LinkPreview,
    files: &quill::state::FileMap,
    media_roots: &[PathBuf],
) -> Option<AnyElement> {
    match &preview.kind {
        LinkPreviewKind::Background(wallpaper) => {
            Some(wallpaper_picture(row_id, wallpaper, files, media_roots))
        }
        LinkPreviewKind::Theme(theme) => Some(theme_picture(row_id, theme, files, media_roots)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{WALLPAPER_MAX_HEIGHT, WALLPAPER_MIN_HEIGHT, wallpaper_box};
    use quill::telegram::envelope::{Background, BackgroundType, PreviewWallpaper};

    fn wallpaper(has_document: bool, thumb: (i32, i32)) -> PreviewWallpaper {
        PreviewWallpaper {
            background: Background {
                id: 0,
                is_default: false,
                is_dark: false,
                name: String::new(),
                file: None,
                thumbnail: None,
                kind: BackgroundType::Wallpaper {
                    blurred: false,
                    moving: false,
                },
            },
            thumb_width: thumb.0,
            thumb_height: thumb.1,
            size: 0,
            has_document,
        }
    }

    #[test]
    fn a_link_without_a_document_takes_the_whole_box() {
        assert_eq!(wallpaper_box(&wallpaper(false, (0, 0))), (160.0, 240.0));
    }

    #[test]
    fn height_follows_the_preview_within_bounds() {
        assert_eq!(wallpaper_box(&wallpaper(true, (320, 480))).1, 240.0);
        assert_eq!(wallpaper_box(&wallpaper(true, (160, 160))).1, 160.0);
        assert_eq!(
            wallpaper_box(&wallpaper(true, (1000, 100))).1,
            WALLPAPER_MIN_HEIGHT
        );
        assert_eq!(
            wallpaper_box(&wallpaper(true, (100, 1000))).1,
            WALLPAPER_MAX_HEIGHT
        );
        // Unknown size: a square, like tdesktop's 1 x 1.
        assert_eq!(wallpaper_box(&wallpaper(true, (0, 0))).1, 160.0);
    }
}
