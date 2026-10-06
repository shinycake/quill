//! Photo editing before sending (Telegram Desktop's photo editor): quarter
//! turns, horizontal flip, a crop rectangle and freehand brush strokes.
//! Geometry is kept in normalized coordinates (0..1) of the working image,
//! so the editor can show a downscaled preview while the result renders at
//! full resolution.

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

/// Turn the image a quarter counterclockwise (Telegram Desktop's rotate).
pub fn rotate_ccw(image: &RgbaImage) -> RgbaImage {
    image::imageops::rotate270(image)
}

pub fn flip(image: &RgbaImage) -> RgbaImage {
    image::imageops::flip_horizontal(image)
}

/// The final picture: strokes painted at full resolution, then cropped.
pub fn render(image: &RgbaImage, crop: CropRect, strokes: &[Stroke]) -> RgbaImage {
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
    let crop = crop.clamped();
    let x = (crop.x * width as f32).round() as u32;
    let y = (crop.y * height as f32).round() as u32;
    let w = ((crop.w * width as f32).round() as u32).clamp(1, width - x.min(width - 1));
    let h = ((crop.h * height as f32).round() as u32).clamp(1, height - y.min(height - 1));
    image::imageops::crop_imm(&canvas, x, y, w, h).to_image()
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
}
