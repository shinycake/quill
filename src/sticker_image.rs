//! The still-image side of the sticker creator (tdesktop
//! `boxes/sticker_creator_box.cpp`, `PrepareEditorState` and the editor's
//! `prepared` callback): the picked image sits as a movable item on a
//! transparent square canvas, the result is fitted to the set's side, and a
//! rounded-corner mask (Large / Medium / Small / None) is cut out of it.
//! Pure pixels and numbers; the editor UI is `ui/photo_editor_set.rs`.

use image::RgbaImage;

/// Side of the editor's transparent canvas. tdesktop prepares the scene at
/// `kStickerSide` for emoji too and fits the result to the emoji side at the
/// end (`RunImageEditorAndCreate` → `FittedStickerSize`).
pub const CANVAS_SIDE: u32 = 512;

/// The caption at the top of the corners menu
/// (`lng_photo_editor_corners_about`).
pub const CORNERS_ABOUT: &str = "Choose how rounded the sticker corners should look.";
/// The menu's check item that keeps the picked image's aspect ratio
/// (`lng_photo_editor_keep_ratio`).
pub const KEEP_RATIO_LABEL: &str = "Keep original aspect ratio";

/// How round the sticker's corners are (`StickerCornersState`); the radius
/// is the shorter side times the multiplier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Corners {
    /// `ForumUserpicRadiusMultiplier`, tdesktop's default.
    #[default]
    Large,
    Medium,
    Small,
    None,
}

impl Corners {
    /// The menu's order (`lng_photo_editor_corners_large` … `_none`).
    pub const ALL: [Self; 4] = [Self::Large, Self::Medium, Self::Small, Self::None];

    pub fn label(self) -> &'static str {
        match self {
            Self::Large => "Large",
            Self::Medium => "Medium",
            Self::Small => "Small",
            Self::None => "None",
        }
    }

    /// Radius over the shorter side (`CornersMultiplier`).
    pub fn multiplier(self) -> f32 {
        match self {
            Self::Large => 0.3,
            Self::Medium => 0.2,
            Self::Small => 0.12,
            Self::None => 0.0,
        }
    }
}

/// An image the editor refuses (`PrepareEditorState`): empty, or longer than
/// ten times its shorter side.
pub fn accepts_image(width: u32, height: u32) -> bool {
    if width == 0 || height == 0 {
        return false;
    }
    let (long, short) = (width.max(height), width.min(height));
    long <= short.saturating_mul(10)
}

/// The ratio "Keep original aspect ratio" restores: 0 when the image is
/// square (within a hundredth), otherwise width over height clamped to
/// 1/3..3 (`PrepareEditorState`). Emoji editors force 0.
pub fn original_ratio(width: u32, height: u32) -> f64 {
    if width == 0 || height == 0 {
        return 0.0;
    }
    let ratio = f64::from(width) / f64::from(height);
    if (ratio - 1.0).abs() < 0.01 {
        0.0
    } else {
        ratio.clamp(1.0 / 3.0, 3.0)
    }
}

/// The crop (as fractions of the square canvas: x, y, w, h) that keeps
/// `ratio`: the largest centered rectangle of that ratio (`Crop::
/// setAspectRatio`). Ratio 0 or 1 is the whole canvas.
pub fn keep_ratio_crop(ratio: f64) -> (f32, f32, f32, f32) {
    if ratio <= 0.0 || !ratio.is_finite() {
        return (0.0, 0.0, 1.0, 1.0);
    }
    let (w, h) = if ratio >= 1.0 {
        (1.0, (1.0 / ratio) as f32)
    } else {
        (ratio as f32, 1.0)
    };
    ((1.0 - w) / 2.0, (1.0 - h) / 2.0, w, h)
}

/// The width (as a fraction of the canvas) of an image fitted into the
/// square canvas keeping its ratio (`QImage::scaled(KeepAspectRatio)`).
pub fn fitted_width(width: u32, height: u32) -> f32 {
    if width == 0 || height == 0 || width >= height {
        1.0
    } else {
        width as f32 / height as f32
    }
}

/// Cut a rounded rectangle out of the image's alpha (`ApplyShapeMask` with
/// `RoundedRect`): radius = shorter side × `multiplier`, anti-aliased on the
/// edge. A multiplier of 0 leaves the image as it is.
pub fn apply_corner_mask(image: &mut RgbaImage, multiplier: f32) {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 || multiplier <= 0.0 {
        return;
    }
    let radius = (width.min(height) as f32 * multiplier).min(width.min(height) as f32 / 2.0);
    if radius <= 0.0 {
        return;
    }
    let (w, h) = (width as f32, height as f32);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
        // Distance from the pixel center to the nearest corner circle
        // center, only where the pixel sits in a corner square.
        let dx = if px < radius {
            radius - px
        } else if px > w - radius {
            px - (w - radius)
        } else {
            0.0
        };
        let dy = if py < radius {
            radius - py
        } else if py > h - radius {
            py - (h - radius)
        } else {
            0.0
        };
        if dx == 0.0 || dy == 0.0 {
            continue;
        }
        let distance = (dx * dx + dy * dy).sqrt();
        // One pixel of anti-aliasing across the circle's edge.
        let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
        if coverage < 1.0 {
            pixel[3] = (f32::from(pixel[3]) * coverage).round() as u8;
        }
    }
}

/// The editor's result made ready for upload: fitted to `side` keeping its
/// ratio (`FittedStickerSize`), then the corner mask.
pub fn prepared_sticker_image(edited: &RgbaImage, side: u32, corners: Corners) -> RgbaImage {
    let (width, height) = edited.dimensions();
    let (fit_w, fit_h) = crate::sticker_set_box::fitted_size(width, height, side);
    let mut fitted = if (fit_w, fit_h) == (width, height) {
        edited.clone()
    } else {
        image::imageops::resize(edited, fit_w, fit_h, image::imageops::FilterType::Lanczos3)
    };
    apply_corner_mask(&mut fitted, corners.multiplier());
    fitted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_editor_refuses_empty_and_very_long_images() {
        assert!(accepts_image(512, 512));
        assert!(accepts_image(100, 1000));
        assert!(!accepts_image(100, 1001));
        assert!(!accepts_image(0, 100));
    }

    #[test]
    fn original_ratio_is_zero_for_squares_and_clamped_otherwise() {
        assert_eq!(original_ratio(512, 512), 0.0);
        assert_eq!(original_ratio(512, 510), 0.0);
        assert!((original_ratio(400, 200) - 2.0).abs() < 1e-9);
        assert!((original_ratio(100, 1000) - 1.0 / 3.0).abs() < 1e-9);
        assert_eq!(original_ratio(0, 10), 0.0);
    }

    #[test]
    fn keep_ratio_crop_is_centered_and_fitted() {
        assert_eq!(keep_ratio_crop(0.0), (0.0, 0.0, 1.0, 1.0));
        let (x, y, w, h) = keep_ratio_crop(2.0);
        assert!((x - 0.0).abs() < 1e-6 && (y - 0.25).abs() < 1e-6);
        assert!((w - 1.0).abs() < 1e-6 && (h - 0.5).abs() < 1e-6);
        let (x, y, w, h) = keep_ratio_crop(0.5);
        assert!((x - 0.25).abs() < 1e-6 && (y - 0.0).abs() < 1e-6);
        assert!((w - 0.5).abs() < 1e-6 && (h - 1.0).abs() < 1e-6);
        assert_eq!(fitted_width(400, 200), 1.0);
        assert_eq!(fitted_width(200, 400), 0.5);
    }

    #[test]
    fn the_corner_mask_clears_corners_and_keeps_the_middle() {
        let mut image = RgbaImage::from_pixel(100, 100, image::Rgba([200, 100, 50, 255]));
        apply_corner_mask(&mut image, Corners::Large.multiplier());
        assert_eq!(image.get_pixel(0, 0)[3], 0);
        assert_eq!(image.get_pixel(99, 99)[3], 0);
        assert_eq!(image.get_pixel(50, 50)[3], 255);
        assert_eq!(image.get_pixel(50, 0)[3], 255, "edges between corners stay");
        assert_eq!(image.get_pixel(0, 50)[3], 255);
        // Inside the corner arc.
        assert_eq!(image.get_pixel(29, 29)[3], 255);

        let mut plain = RgbaImage::from_pixel(10, 10, image::Rgba([1, 2, 3, 255]));
        apply_corner_mask(&mut plain, Corners::None.multiplier());
        assert_eq!(plain.get_pixel(0, 0)[3], 255);
    }

    #[test]
    fn prepared_images_fit_the_side_and_are_masked() {
        let edited = RgbaImage::from_pixel(512, 256, image::Rgba([9, 9, 9, 255]));
        let emoji = prepared_sticker_image(&edited, 100, Corners::Medium);
        assert_eq!(emoji.dimensions(), (100, 50));
        assert_eq!(emoji.get_pixel(0, 0)[3], 0);
        assert_eq!(emoji.get_pixel(50, 25)[3], 255);
        let sticker = prepared_sticker_image(&edited, 512, Corners::None);
        assert_eq!(sticker.dimensions(), (512, 256));
        assert_eq!(sticker.get_pixel(0, 0)[3], 255);
        assert_eq!(Corners::default(), Corners::Large);
        assert_eq!(Corners::ALL.len(), 4);
    }
}
