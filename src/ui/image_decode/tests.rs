use crate::ui::image_decode::{
    DecodeError, DecodeSize, MAX_TEXTURE_EDGE, Sniffed, decode_image, sniff, unpremultiply,
};
use std::path::Path;

fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/images")
        .join(name);
    std::fs::read(path).unwrap()
}

/// The fixtures are 32x16: red on the left half, blue on the right.
fn assert_halves(image: &image::RgbaImage, name: &str) {
    assert_eq!(image.dimensions(), (32, 16), "{name}");
    let left = image.get_pixel(4, 8).0;
    let right = image.get_pixel(27, 8).0;
    assert!(left[0] > 200 && left[2] < 60, "{name}: left {left:?}");
    assert!(right[2] > 200 && right[0] < 60, "{name}: right {right:?}");
    assert_eq!(left[3], 255, "{name}");
}

#[test]
fn sniffs_signatures_and_brands() {
    assert_eq!(sniff(&fixture("halves.heic"), None), Sniffed::Heif);
    assert_eq!(sniff(&fixture("halves.avif"), None), Sniffed::Avif);
    assert_eq!(sniff(&fixture("halves.jxl"), None), Sniffed::JpegXl);
    assert_eq!(
        sniff(&fixture("halves-exif6.jpg"), None),
        Sniffed::Raster(image::ImageFormat::Jpeg)
    );
    assert_eq!(
        sniff(b"<?xml version=\"1.0\"?><svg xmlns=\"x\"/>", None),
        Sniffed::Svg
    );
    assert_eq!(
        sniff(b"\0\0\0\x0cfoo", Some(Path::new("shot.tga"))),
        Sniffed::Raster(image::ImageFormat::Tga)
    );
    assert_eq!(sniff(b"random bytes", None), Sniffed::Other);
    // A `mif1` major brand that lists `avif` is AVIF.
    let mut mif1 = b"\0\0\0\x1cftypmif1\0\0\0\0mif1avifmiaf".to_vec();
    mif1.extend_from_slice(&[0; 8]);
    assert_eq!(sniff(&mif1, None), Sniffed::Avif);
}

#[test]
fn decode_size_shrinks_the_short_side_and_caps_the_long_one() {
    let thumb = DecodeSize { short_edge: 100 };
    assert_eq!(thumb.fit((400, 200)), (200, 100));
    assert_eq!(thumb.fit((50, 80)), (50, 80), "never enlarged");
    assert_eq!(DecodeSize::FULL.fit((4000, 3000)), (4000, 3000));
    let (w, h) = DecodeSize::FULL.fit((30_000, 3_000));
    assert_eq!(w, MAX_TEXTURE_EDGE);
    assert_eq!(h, 819);
    assert_eq!(
        DecodeSize::FULL.long_edge((30_000, 3_000)),
        MAX_TEXTURE_EDGE
    );
    // The chat's sized decodes (avatars, thumbnails).
    let edge = |short_edge| DecodeSize { short_edge };
    assert_eq!(edge(92).fit((160, 160)), (92, 92));
    assert_eq!(edge(200).fit((160, 160)), (160, 160));
    assert_eq!(edge(36).fit((1280, 960)), (48, 36));
    assert_eq!(edge(36).fit((40, 30)), (40, 30));
    assert_eq!(edge(5).fit((0, 10)), (0, 10));
}

#[test]
fn jpeg_exif_orientation_is_applied() {
    let image = decode_image(&fixture("halves-exif6.jpg"), None, DecodeSize::FULL).unwrap();
    // Orientation 6: turned a quarter clockwise, so red ends up on top.
    assert_eq!(image.dimensions(), (16, 32));
    let top = image.get_pixel(8, 4).0;
    let bottom = image.get_pixel(8, 27).0;
    assert!(top[0] > 200 && top[2] < 60, "top {top:?}");
    assert!(bottom[2] > 200 && bottom[0] < 60, "bottom {bottom:?}");
}

#[test]
fn jpeg_xl_decodes_on_every_platform() {
    let bytes = fixture("halves.jxl");
    assert_halves(
        &decode_image(&bytes, None, DecodeSize::FULL).unwrap(),
        "jxl",
    );
    assert_halves(
        &super::rust_codecs::decode_jxl(&bytes, DecodeSize::FULL).unwrap(),
        "jxl-oxide",
    );
}

#[cfg(target_os = "macos")]
#[test]
fn imageio_decodes_heic_and_avif() {
    assert_halves(
        &decode_image(&fixture("halves.heic"), None, DecodeSize::FULL).unwrap(),
        "heic",
    );
    assert_halves(
        &decode_image(&fixture("halves.avif"), None, DecodeSize::FULL).unwrap(),
        "avif",
    );
    let small = decode_image(&fixture("halves.heic"), None, DecodeSize { short_edge: 8 }).unwrap();
    assert_eq!(small.dimensions(), (16, 8));
}

/// libheif is optional (Linux distributions, Homebrew): the test checks
/// the binding when it is there.
#[test]
fn libheif_decodes_heic_when_installed() {
    #[cfg(not(windows))]
    match super::libheif::decode(&fixture("halves.heic"), DecodeSize::FULL) {
        Ok(image) => assert_halves(&image, "libheif"),
        Err(DecodeError::Unsupported) => {}
        Err(other) => panic!("libheif: {other}"),
    }
}

#[test]
fn display_p3_png_is_converted_to_srgb() {
    let icc = moxcms::ColorProfile::new_display_p3().encode().unwrap();
    let mut png = Vec::new();
    {
        use image::ImageEncoder;
        let mut encoder = image::codecs::png::PngEncoder::new(&mut png);
        encoder.set_icc_profile(icc).unwrap();
        encoder
            .write_image(&[0, 200, 0], 1, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
    }
    let image = decode_image(&png, None, DecodeSize::FULL).unwrap();
    let pixel = image.get_pixel(0, 0).0;
    // P3 green is more saturated than sRGB's: sRGB needs negative red to
    // reach it, so red clips to 0 and green rises past 200.
    assert!(pixel[1] > 200, "{pixel:?}");
    assert_eq!(pixel[3], 255);
}

fn assert_triangle_filtered<P>(
    shrunk: &image::RgbaImage,
    source: &image::ImageBuffer<P, Vec<u8>>,
    tolerance: u8,
) where
    P: image::Pixel<Subpixel = u8> + 'static,
{
    let (width, height) = shrunk.dimensions();
    let filter = image::imageops::FilterType::Triangle;
    let expected = image::imageops::resize(source, width, height, filter);
    for (x, y, pixel) in shrunk.enumerate_pixels() {
        let expected = expected.get_pixel(x, y).to_rgba();
        for (got, want) in pixel.0.iter().zip(expected.0) {
            assert!(
                got.abs_diff(want) <= tolerance,
                "({x}, {y}): {pixel:?}, the filter gives {expected:?}"
            );
        }
    }
}

fn png(picture: &image::DynamicImage) -> Vec<u8> {
    let mut bytes = std::io::Cursor::new(Vec::new());
    picture
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

fn busy_pixel(x: u32, y: u32) -> [u8; 3] {
    [
        (x / 4) as u8,
        (y / 4) as u8,
        ((x ^ y) % 3 * 100 + (x + y) % 7) as u8,
    ]
}

#[test]
fn a_shrunk_opaque_picture_matches_the_triangle_filter() {
    let picture = image::RgbImage::from_fn(900, 600, |x, y| image::Rgb(busy_pixel(x, y)));
    let bytes = png(&image::DynamicImage::ImageRgb8(picture.clone()));
    for short_edge in [400, 150, 37] {
        let shrunk = decode_image(&bytes, None, DecodeSize { short_edge }).unwrap();
        assert_eq!(shrunk.height(), short_edge);
        assert_triangle_filtered(&shrunk, &picture, 2);
    }
}

#[test]
fn a_shrunk_transparent_picture_matches_the_triangle_filter() {
    let picture = image::RgbaImage::from_fn(900, 600, |x, y| {
        let [r, g, b] = busy_pixel(x, y);
        image::Rgba([r, g, b, (x / 4 + y / 4) as u8])
    });
    let bytes = png(&image::DynamicImage::ImageRgba8(picture.clone()));
    let shrunk = decode_image(&bytes, None, DecodeSize { short_edge: 150 }).unwrap();
    assert_triangle_filtered(&shrunk, &picture, 2);
}

#[test]
fn a_shrunk_checkerboard_stays_flat() {
    let board =
        image::GrayImage::from_fn(1280, 960, |x, y| image::Luma([((x + y) % 2 * 255) as u8]));
    let bytes = png(&image::DynamicImage::ImageLuma8(board));
    let shrunk = decode_image(&bytes, None, DecodeSize { short_edge: 320 }).unwrap();
    for pixel in shrunk.pixels() {
        assert!((124..=131).contains(&pixel.0[0]), "{pixel:?}");
    }
}

#[test]
fn garbage_and_oversized_images_fail_cleanly() {
    assert!(decode_image(b"not an image at all", None, DecodeSize::FULL).is_err());
    let truncated = &fixture("halves-exif6.jpg")[..40];
    assert!(decode_image(truncated, None, DecodeSize::FULL).is_err());
    // A PNG header claiming 20000x20000 is refused before decoding.
    let mut png = Vec::new();
    {
        use image::ImageEncoder;
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[0; 3], 1, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
    }
    // IHDR width/height live at bytes 16..24; the CRC is not checked first.
    png.splice(
        16..24,
        20_000_u32
            .to_be_bytes()
            .into_iter()
            .chain(20_000_u32.to_be_bytes()),
    );
    assert!(decode_image(&png, None, DecodeSize::FULL).is_err());
}

#[test]
fn svg_renders_at_its_own_size() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="10" height="10" fill="#f00"/></svg>"##;
    let image = decode_image(svg, None, DecodeSize::FULL).unwrap();
    assert_eq!(image.dimensions(), (20, 10));
    assert_eq!(image.get_pixel(2, 2).0, [255, 0, 0, 255]);
    assert_eq!(image.get_pixel(15, 2).0[3], 0);
}

#[test]
fn unpremultiply_restores_straight_alpha() {
    let mut pixels = [64, 32, 0, 128, 10, 20, 30, 255, 0, 0, 0, 0];
    unpremultiply(&mut pixels);
    assert_eq!(pixels, [128, 64, 0, 128, 10, 20, 30, 255, 0, 0, 0, 0]);
}
