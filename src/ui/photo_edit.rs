//! Photo editing before sending (Telegram Desktop's photo editor): quarter
//! turns, horizontal flip, a crop rectangle and freehand brush strokes.
//! Geometry is kept in normalized coordinates (0..1) of the working image,
//! so the editor can show a downscaled preview while the result renders at
//! full resolution.
#![deny(clippy::indexing_slicing)]

use image::{Rgba, RgbaImage};

/// A crop rectangle in normalized image coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl CropRect {
    pub const FULL: CropRect = CropRect {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    /// Smallest crop side, as a fraction of the image.
    pub const MIN_SIDE: f32 = 0.05;

    /// Clamp into the image, keeping at least `MIN_SIDE` per side.
    pub fn clamped(self) -> Self {
        let w = self.w.clamp(Self::MIN_SIDE, 1.0);
        let h = self.h.clamp(Self::MIN_SIDE, 1.0);
        CropRect {
            x: self.x.clamp(0.0, 1.0 - w),
            y: self.y.clamp(0.0, 1.0 - h),
            w,
            h,
        }
    }

    /// The same region after the image turns a quarter counterclockwise.
    pub fn rotated_ccw(self) -> Self {
        CropRect {
            x: self.y,
            y: 1.0 - self.x - self.w,
            w: self.h,
            h: self.w,
        }
    }

    /// The same region after a horizontal flip.
    pub fn flipped(self) -> Self {
        CropRect {
            x: 1.0 - self.x - self.w,
            ..self
        }
    }
}

/// A freehand brush stroke: color, width as a fraction of the image's
/// shorter side, and points in normalized coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub color: [u8; 4],
    pub width: f32,
    pub points: Vec<(f32, f32)>,
}

impl Stroke {
    pub fn rotated_ccw(&self) -> Self {
        Stroke {
            points: self.points.iter().map(|&(x, y)| (y, 1.0 - x)).collect(),
            ..self.clone()
        }
    }

    pub fn flipped(&self) -> Self {
        Stroke {
            points: self.points.iter().map(|&(x, y)| (1.0 - x, y)).collect(),
            ..self.clone()
        }
    }
}

/// A sticker or emoji placed on the photo: its picture, its center in
/// normalized coordinates, and its width as a fraction of the photo's.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub image: RgbaImage,
    pub center: (f32, f32),
    pub width: f32,
}

impl Placed {
    /// Height over width of the picture.
    pub fn aspect(&self) -> f32 {
        let (width, height) = self.image.dimensions();
        height as f32 / width.max(1) as f32
    }

    /// After a quarter turn of a `photo_w`×`photo_h` photo: the picture
    /// turns too and keeps its size on screen.
    pub fn rotated_ccw(&self, photo_w: u32, photo_h: u32) -> Self {
        let (x, y) = self.center;
        Placed {
            image: rotate_ccw(&self.image),
            center: (y, 1.0 - x),
            width: self.width * photo_w as f32 / photo_h.max(1) as f32 * self.aspect(),
        }
    }

    pub fn flipped(&self) -> Self {
        Placed {
            image: flip(&self.image),
            center: (1.0 - self.center.0, self.center.1),
            width: self.width,
        }
    }
}

/// Turn the image a quarter counterclockwise (Telegram Desktop's rotate).
pub fn rotate_ccw(image: &RgbaImage) -> RgbaImage {
    image::imageops::rotate270(image)
}

pub fn flip(image: &RgbaImage) -> RgbaImage {
    image::imageops::flip_horizontal(image)
}

/// The quarter turns and flip waiting for the full-size picture: Rotate and
/// Flip turn only the preview, and the render applies these once, off the
/// UI thread.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Orientation {
    ccw_turns: u8,
    flipped: bool,
}

impl Orientation {
    /// After one more Rotate. A flipped picture turns the other way under
    /// the flip, so the pending turn goes back one.
    pub fn rotated_ccw(self) -> Self {
        let step = if self.flipped { 3 } else { 1 };
        Orientation {
            ccw_turns: (self.ccw_turns + step) % 4,
            ..self
        }
    }

    pub fn flipped(self) -> Self {
        Orientation {
            flipped: !self.flipped,
            ..self
        }
    }

    /// The size of a `width`×`height` picture once oriented.
    pub fn dimensions(self, (width, height): (u32, u32)) -> (u32, u32) {
        if self.ccw_turns % 2 == 1 {
            (height, width)
        } else {
            (width, height)
        }
    }

    /// The picture as it shows: turned, then flipped.
    pub fn apply(self, image: RgbaImage) -> RgbaImage {
        let turned = match self.ccw_turns {
            1 => image::imageops::rotate270(&image),
            2 => image::imageops::rotate180(&image),
            3 => image::imageops::rotate90(&image),
            _ => image,
        };
        if self.flipped { flip(&turned) } else { turned }
    }
}

/// The edit at full size: the picture, its pending turns and what was
/// drawn and placed on it, ready to render off the UI thread.
pub struct RenderJob {
    pub working: RgbaImage,
    pub orientation: Orientation,
    pub crop: CropRect,
    pub strokes: Vec<Stroke>,
    pub placed: Vec<Placed>,
}

impl RenderJob {
    pub fn render(self) -> RgbaImage {
        let picture = self.orientation.apply(self.working);
        render(&picture, self.crop, &self.strokes, &self.placed)
    }
}

/// The final picture: strokes painted at full resolution, stickers and
/// emoji laid over them, then cropped.
pub fn render(
    image: &RgbaImage,
    crop: CropRect,
    strokes: &[Stroke],
    placed: &[Placed],
) -> RgbaImage {
    let mut canvas = image.clone();
    let (width, height) = canvas.dimensions();
    let side = width.min(height) as f32;
    for stroke in strokes {
        let radius = (stroke.width * side / 2.0).max(0.5);
        let to_px = |(x, y): (f32, f32)| (x * width as f32, y * height as f32);
        let mut points = stroke.points.iter().copied().map(to_px);
        let Some(mut last) = points.next() else {
            continue;
        };
        stamp(&mut canvas, last, radius, stroke.color);
        for point in points {
            // Stamp discs every half radius along the segment.
            let (dx, dy) = (point.0 - last.0, point.1 - last.1);
            let steps = ((dx * dx + dy * dy).sqrt() / (radius / 2.0).max(0.5)).ceil() as usize;
            for step in 1..=steps.max(1) {
                let t = step as f32 / steps.max(1) as f32;
                stamp(
                    &mut canvas,
                    (last.0 + dx * t, last.1 + dy * t),
                    radius,
                    stroke.color,
                );
            }
            last = point;
        }
    }
    for item in placed {
        overlay_item(&mut canvas, item);
    }
    let crop = crop.clamped();
    let x = (crop.x * width as f32).round() as u32;
    let y = (crop.y * height as f32).round() as u32;
    let w = ((crop.w * width as f32).round() as u32).clamp(1, width - x.min(width - 1));
    let h = ((crop.h * height as f32).round() as u32).clamp(1, height - y.min(height - 1));
    image::imageops::crop_imm(&canvas, x, y, w, h).to_image()
}

/// Draw a placed item onto the canvas at its size and place. Only the
/// part inside the canvas is scaled, so a picture zoomed far past the
/// edges costs no more than the canvas itself.
pub fn overlay_item(canvas: &mut RgbaImage, item: &Placed) {
    let (width, height) = canvas.dimensions();
    let (src_w, src_h) = item.image.dimensions();
    if width == 0 || height == 0 || src_w == 0 || src_h == 0 {
        return;
    }
    let w = (item.width * width as f32).round().max(1.0) as i64;
    let h = (w as f32 * item.aspect()).round().max(1.0) as i64;
    let x = (item.center.0 * width as f32 - w as f32 / 2.0).round() as i64;
    let y = (item.center.1 * height as f32 - h as f32 / 2.0).round() as i64;
    // The visible rectangle, on the canvas and on the picture.
    let (x0, y0) = (x.max(0), y.max(0));
    let (x1, y1) = (
        (x + w).min(i64::from(width)),
        (y + h).min(i64::from(height)),
    );
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let part = |from: i64, total: i64, source: u32| -> u32 {
        ((from as f64 / total as f64) * f64::from(source)).floor() as u32
    };
    let sx = part(x0 - x, w, src_w).min(src_w - 1);
    let sy = part(y0 - y, h, src_h).min(src_h - 1);
    let sw = (part(x1 - x, w, src_w).max(sx + 1) - sx).min(src_w - sx);
    let sh = (part(y1 - y, h, src_h).max(sy + 1) - sy).min(src_h - sy);
    let visible = image::imageops::crop_imm(&item.image, sx, sy, sw, sh);
    let picture = image::imageops::resize(
        &*visible,
        (x1 - x0) as u32,
        (y1 - y0) as u32,
        image::imageops::FilterType::Triangle,
    );
    image::imageops::overlay(canvas, &picture, x0, y0);
}

/// A filled, anti-aliased disc blended over the image.
fn stamp(image: &mut RgbaImage, (cx, cy): (f32, f32), radius: f32, color: [u8; 4]) {
    let (width, height) = image.dimensions();
    let x0 = (cx - radius - 1.0).floor().max(0.0) as u32;
    let y0 = (cy - radius - 1.0).floor().max(0.0) as u32;
    let x1 = ((cx + radius + 1.0).ceil() as u32).min(width);
    let y1 = ((cy + radius + 1.0).ceil() as u32).min(height);
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let cover = (radius + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let alpha = cover * f32::from(color[3]) / 255.0;
            let pixel = image.get_pixel_mut(x, y);
            let Rgba(old) = *pixel;
            let mix = |new: u8, old: u8| {
                (f32::from(new) * alpha + f32::from(old) * (1.0 - alpha)).round() as u8
            };
            *pixel = Rgba([
                mix(color[0], old[0]),
                mix(color[1], old[1]),
                mix(color[2], old[2]),
                old[3].max((alpha * 255.0) as u8),
            ]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_follows_rotation_and_flip() {
        let crop = CropRect {
            x: 0.1,
            y: 0.2,
            w: 0.3,
            h: 0.4,
        };
        let turned = crop.rotated_ccw();
        assert!((turned.x - 0.2).abs() < 1e-6);
        assert!((turned.y - 0.6).abs() < 1e-6);
        assert!((turned.w - 0.4).abs() < 1e-6);
        // Four quarter turns come back.
        let back = turned.rotated_ccw().rotated_ccw().rotated_ccw();
        assert!((back.x - crop.x).abs() < 1e-6 && (back.y - crop.y).abs() < 1e-6);
        let flipped = crop.flipped();
        assert!((flipped.x - 0.6).abs() < 1e-6);
    }

    #[test]
    fn render_paints_strokes_then_crops() {
        let image = RgbaImage::from_pixel(100, 50, Rgba([0, 0, 0, 255]));
        let stroke = Stroke {
            color: [255, 0, 0, 255],
            width: 0.2,
            points: vec![(0.1, 0.5), (0.9, 0.5)],
        };
        let out = render(
            &image,
            CropRect {
                x: 0.5,
                y: 0.0,
                w: 0.5,
                h: 1.0,
            },
            &[stroke],
            &[],
        );
        assert_eq!(out.dimensions(), (50, 50));
        // The stroke crosses the middle row.
        assert_eq!(out.get_pixel(10, 25)[0], 255);
        assert_eq!(out.get_pixel(10, 2)[0], 0);
    }

    #[test]
    fn rotate_turns_dimensions() {
        let image = RgbaImage::new(30, 10);
        assert_eq!(rotate_ccw(&image).dimensions(), (10, 30));
    }

    #[test]
    fn placed_pictures_composite_and_follow_rotation() {
        let photo = RgbaImage::from_pixel(100, 50, Rgba([0, 0, 0, 255]));
        let dot = Placed {
            image: RgbaImage::from_pixel(10, 10, Rgba([0, 255, 0, 255])),
            center: (0.5, 0.5),
            width: 0.2,
        };
        let out = render(&photo, CropRect::FULL, &[], std::slice::from_ref(&dot));
        assert_eq!(out.get_pixel(50, 25)[1], 255);
        assert_eq!(out.get_pixel(5, 5)[1], 0);
        // A 20 px wide square on a 100×50 photo stays 20 px wide when the
        // photo turns to 50×100.
        let turned = dot.rotated_ccw(100, 50);
        assert!((turned.width - 0.4).abs() < 1e-6);
        assert_eq!(turned.center, (0.5, 0.5));
    }

    #[test]
    fn a_pending_orientation_matches_turning_step_by_step() {
        let image = RgbaImage::from_fn(5, 3, |x, y| Rgba([x as u8, y as u8, 7, 255]));
        // R, F, R, R, F, R in every prefix: the recorded orientation lands
        // on the same pixels as each step done in order.
        let steps = [true, false, true, true, false, true, false];
        let (mut stepped, mut pending) = (image.clone(), Orientation::default());
        for rotate in steps {
            if rotate {
                stepped = rotate_ccw(&stepped);
                pending = pending.rotated_ccw();
            } else {
                stepped = flip(&stepped);
                pending = pending.flipped();
            }
            assert_eq!(pending.dimensions(image.dimensions()), stepped.dimensions());
            assert_eq!(pending.apply(image.clone()), stepped);
        }
    }
}
