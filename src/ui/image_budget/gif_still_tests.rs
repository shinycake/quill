use super::{BoundedImageCache, CacheScope, Fit, begin_frame, sized_media};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, Render, Styled, TestAppContext, VisualTestContext,
    Window, img, px, size,
};
use image::{Delay, Frame, Rgba, RgbaImage};
use std::path::PathBuf;

const WIDTH: u32 = 160;
const HEIGHT: u32 = 120;
const FRAMES: u8 = 40;

/// Red, then a run of frames with no red in them.
fn many_frame_gif() -> Vec<u8> {
    let frames = (0..FRAMES).map(|index| {
        let color = match index {
            0 => [255, 0, 0, 255],
            later => [0, later.saturating_mul(6), 255, 255],
        };
        Frame::from_parts(
            RgbaImage::from_pixel(WIDTH, HEIGHT, Rgba(color)),
            0,
            0,
            Delay::from_numer_denom_ms(100, 1),
        )
    });
    let mut bytes = Vec::new();
    image::codecs::gif::GifEncoder::new(&mut bytes)
        .encode_frames(frames)
        .expect("encode the GIF");
    bytes
}

struct Picture {
    cache: Entity<BoundedImageCache>,
    path: PathBuf,
}

impl Render for Picture {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        begin_frame();
        let picture = img(sized_media(
            &self.path,
            (px(40.), px(30.)),
            Some((WIDTH as i32, HEIGHT as i32)),
            Fit::Cover,
        ))
        .size(px(40.));
        CacheScope::new(self.cache.clone(), picture)
    }
}

/// The picture's folder, removed however the test ends.
struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A `.gif` file shown as a picture costs one picture. Nothing draws its
/// other frames: the one GIF that plays decodes through `GifClip`.
#[gpui_kit::test]
fn a_many_frame_gif_loads_as_its_first_frame_at_display_size(cx: &mut TestAppContext) {
    cx.update(gpui_kit::init);
    let folder = Folder(std::env::temp_dir().join(format!("quill-gif-still-{}", std::process::id())));
    std::fs::create_dir_all(&folder.0).expect("a folder for the picture");
    let path = folder.0.join("many-frames.gif");
    std::fs::write(&path, many_frame_gif()).expect("write the GIF");

    let handle = cx.open_window(size(px(200.), px(200.)), move |_, cx| {
        let cache = cx.new(|_| BoundedImageCache::new(Default::default()));
        Picture { cache, path }
    });
    let view = handle.root(cx).expect("the view");
    let cache = view.read_with(cx, |view, _| view.cache.clone());
    let mut vcx = VisualTestContext::from_window(handle.into(), cx);
    for _ in 0..3 {
        vcx.run_until_parked();
        vcx.update(|window, cx| window.render_frame(cx));
    }
    vcx.run_until_parked();

    let images = cache.read_with(&vcx, |cache, _| cache.loaded_images());
    let [image] = images.as_slice() else {
        panic!("one image for the file, got {}", images.len());
    };
    assert_eq!(image.frame_count(), 1, "the frames held for one still");
    let shown = image.size(0);
    assert!(
        shown.width.0 < WIDTH as i32 && shown.height.0 <= 64,
        "decoded at {shown:?} for a 40 x 30 frame"
    );
    let [blue, green, red, alpha] = *image
        .as_bytes(0)
        .and_then(|pixels| pixels.first_chunk::<4>())
        .expect("a BGRA pixel");
    assert!(
        red > 200 && green < 60 && blue < 60 && alpha == 255,
        "the first frame's red, got {:?}",
        [red, green, blue, alpha]
    );
}
