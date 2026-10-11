//! The wallpaper thread's half: reads photo and pattern files and renders a
//! [`Wallpaper`] at an exact pixel size (tdesktop `PrepareBackgroundImage`
//! plus `CacheBackground`). Everything here runs off the UI thread; the
//! pixel maths lives in `quill::wallpaper_raster`.

use super::wallpaper::Wallpaper;
use quill::wallpaper_raster::{
    BLUR_RADIUS, BLUR_SIDE, FillSampler, MAX_IMAGE_SIDE, Mask, MixTable, PatternMix, Raster, blur,
    compose_fill, compose_pattern, cover_rects, fit_within, long_image_crop, pattern_tile_size,
    tile,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A rendered background and where its top-left corner sits in the area
/// (a covering photo may overhang by a pixel).
pub(super) struct Rendered {
    pub raster: Raster,
    pub x: i32,
    pub y: i32,
}

/// A decoded pattern file.
enum Pattern {
    /// TGV (gzipped SVG) or plain SVG, rasterised at each tile size.
    Svg(Box<resvg::usvg::Tree>),
    Png(image::RgbaImage),
}

impl Pattern {
    fn read(path: &Path) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
        if data.starts_with(&[0x89, b'P', b'N', b'G']) {
            return Some(Self::Png(image::load_from_memory(&data).ok()?.to_rgba8()));
        }
        // `Tree::from_data` unzips TGV itself (resvg feature `svgz`).
        let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).ok()?;
        let size = tree.size();
        (size.width() > 0.0 && size.height() > 0.0).then(|| Self::Svg(Box::new(tree)))
    }

    fn size(&self) -> (f32, f32) {
        match self {
            Self::Svg(tree) => (tree.size().width(), tree.size().height()),
            Self::Png(image) => (image.width() as f32, image.height() as f32),
        }
    }

    /// The pattern's coverage at exactly `width` x `height`.
    fn mask(&self, width: u32, height: u32) -> Option<Mask> {
        let alpha = match self {
            Self::Svg(tree) => {
                let size = tree.size();
                let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
                resvg::render(
                    tree,
                    resvg::tiny_skia::Transform::from_scale(
                        width as f32 / size.width(),
                        height as f32 / size.height(),
                    ),
                    &mut pixmap.as_mut(),
                );
                pixmap.pixels().iter().map(|p| p.alpha()).collect()
            }
            Self::Png(image) => {
                image::imageops::resize(image, width, height, image::imageops::FilterType::Triangle)
                    .pixels()
                    .map(|p| p.0[3])
                    .collect()
            }
        };
        Some(Mask {
            width,
            height,
            alpha,
        })
    }
}

/// Files the thread read recently, so a resize re-renders without
/// decoding again. Two of each: the chat on screen and the one before.
#[derive(Default)]
pub(super) struct Sources {
    photos: Vec<((PathBuf, bool), Option<Arc<Raster>>)>,
    patterns: Vec<(PathBuf, Option<Arc<Pattern>>)>,
    masks: Vec<((PathBuf, u32, u32), Option<Arc<Mask>>)>,
}

const KEEP: usize = 2;

/// The value for `key`, made once and kept among the last [`KEEP`].
fn remember<K: PartialEq, V: Clone>(
    list: &mut Vec<(K, Option<V>)>,
    key: K,
    make: impl FnOnce() -> Option<V>,
) -> Option<V> {
    if let Some(at) = list.iter().position(|(k, _)| *k == key) {
        let entry = list.remove(at);
        let value = entry.1.clone();
        list.push(entry);
        return value;
    }
    let value = make();
    if list.len() >= KEEP {
        list.remove(0);
    }
    list.push((key, value.clone()));
    value
}

/// A photo as tdesktop prepares it: long images trimmed to 40:1, at most
/// 2960 px a side, and for Blur fitted in 900 px and blurred.
fn read_photo(path: &Path, blurred: bool) -> Option<Raster> {
    let mut image = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?
        .to_rgba8();
    if let Some(crop) = long_image_crop(image.width(), image.height()) {
        image = image::imageops::crop_imm(
            &image,
            crop.x as u32,
            crop.y as u32,
            crop.width,
            crop.height,
        )
        .to_image();
    }
    let side = if blurred { BLUR_SIDE } else { MAX_IMAGE_SIDE };
    let (w, h) = fit_within(image.width(), image.height(), side);
    if (w, h) != image.dimensions() {
        image = image::imageops::resize(&image, w, h, image::imageops::FilterType::Triangle);
    }
    let (w, h) = image.dimensions();
    let mut raster = Raster::from_rgba(w, h, image.into_raw());
    if blurred {
        blur(&mut raster, BLUR_RADIUS);
    }
    Some(raster)
}

/// `source`'s `from` part scaled to `width` x `height` (Qt's smooth
/// scaling; channel order does not matter to the filter).
fn crop_scale(
    source: &Raster,
    from: quill::wallpaper_raster::PixelRect,
    width: u32,
    height: u32,
) -> Option<Raster> {
    let view = image::RgbaImage::from_raw(source.width, source.height, source.bgra.clone())?;
    let part = image::imageops::crop_imm(
        &view,
        from.x.max(0) as u32,
        from.y.max(0) as u32,
        from.width,
        from.height,
    )
    .to_image();
    let scaled = image::imageops::resize(
        &part,
        width.max(1),
        height.max(1),
        image::imageops::FilterType::Triangle,
    );
    Some(Raster {
        width: scaled.width(),
        height: scaled.height(),
        bgra: scaled.into_raw(),
    })
}

impl Sources {
    /// `wallpaper` over a `width` x `height` pixel area; `None` when there
    /// is nothing to draw (a missing photo).
    pub(super) fn render(
        &mut self,
        wallpaper: &Wallpaper,
        width: u32,
        height: u32,
    ) -> Option<Rendered> {
        if width == 0 || height == 0 {
            return None;
        }
        let whole = |raster| Rendered { raster, x: 0, y: 0 };
        match wallpaper {
            Wallpaper::Solid(color) => Some(whole(compose_fill(
                &FillSampler::new(&quill::telegram::envelope::BackgroundFill::Solid(*color)),
                width,
                height,
            ))),
            Wallpaper::Fill(fill, rotation) => Some(whole(compose_fill(
                &FillSampler::rotated(fill, *rotation),
                width,
                height,
            ))),
            Wallpaper::Pattern {
                fill,
                path,
                intensity,
                inverted,
                rotation,
            } => {
                let sampler = FillSampler::rotated(fill, *rotation);
                let pattern = remember(&mut self.patterns, path.clone(), || {
                    Pattern::read(path).map(Arc::new)
                });
                // Until the pattern reads, the fill shows alone (tdesktop
                // paints the gradient while the pattern loads).
                let Some(pattern) = pattern else {
                    return Some(whole(compose_fill(&sampler, width, height)));
                };
                let (pw, ph) = pattern.size();
                let (tw, th) = pattern_tile_size(pw, ph, height);
                let mask = remember(&mut self.masks, (path.clone(), tw, th), || {
                    pattern.mask(tw, th).map(Arc::new)
                })?;
                let table = MixTable::new(PatternMix::new(&fill.colors(), *intensity, *inverted));
                Some(whole(compose_pattern(
                    &sampler, &table, &mask, width, height,
                )))
            }
            Wallpaper::Image {
                path,
                blurred,
                tiled,
                ..
            } => {
                let photo = remember(&mut self.photos, (path.clone(), *blurred), || {
                    read_photo(path, *blurred).map(Arc::new)
                })?;
                if *tiled {
                    return Some(whole(tile(&photo, width, height)));
                }
                let (from, to) = cover_rects(width, height, photo.width, photo.height);
                let raster = crop_scale(&photo, from, to.width, to.height)?;
                Some(Rendered {
                    raster,
                    x: to.x,
                    y: to.y,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Pattern, Sources, remember};
    use crate::ui::wallpaper::Wallpaper;
    use quill::telegram::envelope::BackgroundFill;

    const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20" viewBox="0 0 10 20"><rect x="0" y="0" width="5" height="20"/></svg>"#;

    fn scratch(name: &str, data: &[u8]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("quill-wallpaper-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, data).unwrap();
        path
    }

    #[test]
    fn svg_and_tgv_patterns_rasterise_at_the_tile_size() {
        use flate2::{Compression, write::GzEncoder};
        use std::io::Write;
        let svg = Pattern::read(&scratch("p.svg", SVG.as_bytes())).expect("svg");
        assert_eq!(svg.size(), (10.0, 20.0));
        let mask = svg.mask(8, 16).unwrap();
        assert_eq!(mask.alpha.len(), 128);
        assert_eq!(mask.alpha[0], 255, "left half covered");
        assert_eq!(mask.alpha[7], 0, "right half clear");
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        gz.write_all(SVG.as_bytes()).unwrap();
        let tgv = Pattern::read(&scratch("p.tgv", &gz.finish().unwrap())).expect("tgv");
        assert_eq!(tgv.mask(8, 16), svg.mask(8, 16));
        assert!(Pattern::read(&scratch("bad.tgv", b"not a pattern")).is_none());
    }

    #[test]
    fn png_patterns_use_their_alpha() {
        let img = image::RgbaImage::from_fn(4, 4, |x, _| {
            image::Rgba([0, 0, 0, if x < 2 { 255 } else { 0 }])
        });
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let pattern = Pattern::read(&scratch("p.png", &png)).expect("png");
        let mask = pattern.mask(4, 4).unwrap();
        assert_eq!(mask.alpha[0], 255);
        assert_eq!(mask.alpha[3], 0);
    }

    #[test]
    fn patterns_compose_at_the_area_size_and_photos_cover_or_tile() {
        let mut sources = Sources::default();
        let path = scratch("area.svg", SVG.as_bytes());
        let pattern = Wallpaper::Pattern {
            fill: BackgroundFill::Freeform(vec![0xffffff, 0xeeeeee, 0xdddddd, 0xcccccc]),
            path: path.clone(),
            intensity: 50,
            inverted: false,
            rotation: 0,
        };
        let out = sources.render(&pattern, 60, 40).unwrap();
        assert_eq!((out.raster.width, out.raster.height), (60, 40));
        // The pattern darkens somewhere and leaves the fill elsewhere.
        let reds: std::collections::HashSet<u8> = out
            .raster
            .bgra
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| p[2])
            .collect();
        assert!(reds.len() > 4);

        let photo = image::RgbaImage::from_fn(30, 10, |x, _| image::Rgba([x as u8 * 8, 0, 0, 255]));
        let photo_path = scratch("photo.png", &{
            let mut png = Vec::new();
            image::DynamicImage::ImageRgba8(photo)
                .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
                .unwrap();
            png
        });
        let cover = Wallpaper::Image {
            path: photo_path.clone(),
            blurred: false,
            tiled: false,
        };
        let out = sources.render(&cover, 20, 20).unwrap();
        assert_eq!((out.raster.width, out.raster.height), (20, 20));
        let tiled = Wallpaper::Image {
            path: photo_path.clone(),
            blurred: false,
            tiled: true,
        };
        let out = sources.render(&tiled, 70, 25).unwrap();
        assert_eq!((out.raster.width, out.raster.height), (70, 25));
        assert_eq!(out.raster.pixel(0, 0), out.raster.pixel(30, 10), "repeats");
        let blurred = Wallpaper::Image {
            path: photo_path,
            blurred: true,
            tiled: false,
        };
        assert!(sources.render(&blurred, 20, 20).is_some());
        let missing = Wallpaper::Image {
            path: "/nonexistent/quill.jpg".into(),
            blurred: false,
            tiled: false,
        };
        assert!(sources.render(&missing, 20, 20).is_none());
    }

    #[test]
    fn sources_keep_only_the_last_two() {
        let mut list: Vec<(u8, Option<u8>)> = Vec::new();
        let mut made = 0;
        for key in [1, 2, 1, 3, 1] {
            remember(&mut list, key, || {
                made += 1;
                Some(key)
            });
        }
        assert_eq!(made, 3, "1 and 2 made, 1 reused, 3 made, 1 kept");
        assert_eq!(list.len(), 2);
    }
}
