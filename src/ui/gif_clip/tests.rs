use super::{
    ClipBuilder, GifClip, GifError, MAX_BYTES, MAX_CANVAS_SIDE, MAX_HELD, canvas_fits, collect,
    fitted, frame_delay,
};
use image::{Delay, Frame, ImageResult, Rgba, RgbaImage};
use std::cell::Cell;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

fn solid(color: [u8; 4]) -> RgbaImage {
    RgbaImage::from_pixel(8, 4, Rgba(color))
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
/// A cancel flag that is never set.
static GO: AtomicBool = AtomicBool::new(false);

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}

fn delayed(numer: u32, denom: u32) -> Frame {
    Frame::from_parts(solid(RED), 0, 0, Delay::from_numer_denom_ms(numer, denom))
}

/// An 8 x 4 GIF of `colors`, each shown 100 ms.
fn encode(colors: &[[u8; 4]]) -> Vec<u8> {
    encode_sized(8, 4, colors)
}

fn encode_sized(width: u32, height: u32, colors: &[[u8; 4]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        let frames = colors.iter().map(|color| {
            Frame::from_parts(
                RgbaImage::from_pixel(width, height, Rgba(*color)),
                0,
                0,
                Delay::from_numer_denom_ms(100, 1),
            )
        });
        encoder.encode_frames(frames).expect("encode");
    }
    bytes
}

/// `bytes` written to a file of its own under the temp directory.
struct TempFile(PathBuf);

impl TempFile {
    fn new(name: &str, bytes: &[u8]) -> Self {
        let dir =
            std::env::temp_dir().join(format!("quill-gif-clip-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let path = dir.join(name);
        std::fs::write(&path, bytes).expect("write");
        Self(path)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}

#[test]
fn a_repeated_picture_is_held_once_with_its_whole_duration() {
    let mut builder = ClipBuilder::default();
    for (color, delay) in [(RED, 100), (RED, 100), (RED, 50), (BLUE, 200), (RED, 100)] {
        assert!(builder.push(solid(color), ms(delay)));
    }
    let clip = builder.finish().expect("clip");
    assert_eq!(clip.held(), 3);
    assert_eq!(clip.total(), ms(550));
    let at = |t| clip.frame_at(ms(t)).expect("frame");
    // The first red shows for 250 ms, then blue until 450, then red.
    assert!(Arc::ptr_eq(&at(0), &at(249)));
    assert!(!Arc::ptr_eq(&at(249), &at(250)));
    assert!(Arc::ptr_eq(&at(250), &at(449)));
    assert!(!Arc::ptr_eq(&at(449), &at(450)));
    // It loops.
    assert!(Arc::ptr_eq(&at(0), &at(550)));
    assert!(Arc::ptr_eq(&at(250), &at(550 + 300)));
    // The shortest held picture (100 ms) sets the redraw rate.
    assert_eq!(clip.fps(), 10);
}

#[test]
fn a_decoded_gif_holds_repeated_frames_once() {
    let file = TempFile::new("held.gif", &encode(&[RED, RED, RED, BLUE, BLUE]));
    let clip = GifClip::decode(&file.0, 320, &GO, |_| {}).expect("decode");
    assert_eq!(clip.held(), 2);
    assert!(!clip.is_still());
    assert_eq!(clip.total(), ms(500));
}

#[test]
fn the_held_pictures_are_bounded_in_count() {
    let mut builder = ClipBuilder::default();
    let mut fitted_all = true;
    for n in 0..MAX_HELD + 5 {
        let shade = u8::try_from(n % 250).unwrap_or(0);
        let tint = u8::try_from(n / 250).unwrap_or(0);
        fitted_all &= builder.push(solid([shade, tint, 7, 255]), ms(40));
    }
    assert!(!fitted_all);
    assert_eq!(builder.held, MAX_HELD);
}

#[test]
fn the_held_pictures_are_bounded_in_bytes() {
    // 256 x 256 RGBA: 256 KiB each, so the byte bound comes before the
    // count bound.
    let side = 256u32;
    let each = (side * side * 4) as usize;
    assert!(MAX_BYTES / each < MAX_HELD);
    let mut builder = ClipBuilder::default();
    let mut pushed = 0usize;
    for n in 0..MAX_HELD {
        let shade = u8::try_from(n % 250).unwrap_or(0);
        let tint = u8::try_from(n / 250).unwrap_or(0);
        if !builder.push(
            RgbaImage::from_pixel(side, side, Rgba([shade, tint, 1, 255])),
            ms(40),
        ) {
            break;
        }
        pushed += 1;
    }
    assert_eq!(pushed, MAX_BYTES / each);
    assert!(builder.bytes <= MAX_BYTES);
    // A picture equal to the last still extends it past the bound.
    let n = pushed - 1;
    let shade = u8::try_from(n % 250).unwrap_or(0);
    let tint = u8::try_from(n / 250).unwrap_or(0);
    let last = RgbaImage::from_pixel(side, side, Rgba([shade, tint, 1, 255]));
    assert!(builder.push(last, ms(40)));
}

#[test]
fn frame_delays_follow_browsers() {
    // 10 ms and less (0 included) play at 100 ms.
    assert_eq!(frame_delay(&delayed(0, 1)), ms(100));
    assert_eq!(frame_delay(&delayed(10, 1)), ms(100));
    assert_eq!(frame_delay(&delayed(20, 1)), ms(20));
    assert_eq!(frame_delay(&delayed(70, 1)), ms(70));
    // A fractional delay rounds down to whole milliseconds.
    assert_eq!(frame_delay(&delayed(45, 2)), ms(22));
}

#[test]
fn pictures_are_fitted_down_never_up() {
    let fit = |w, h, edge| fitted(RgbaImage::new(w, h), edge).dimensions();
    assert_eq!(fit(640, 320, 320), (320, 160));
    assert_eq!(fit(320, 640, 160), (80, 160));
    // Already small enough: as is.
    assert_eq!(fit(100, 50, 320), (100, 50));
    // A sliver keeps one pixel.
    assert_eq!(fit(1, 1000, 320), (1, 320));
}

#[test]
fn a_gif_decodes_at_the_size_asked_for() {
    let file = TempFile::new("sized.gif", &encode_sized(64, 32, &[RED, BLUE]));
    // The 64 x 32 picture at a 32 px edge is 32 x 16.
    let small = GifClip::decode(&file.0, 32, &GO, |_| {});
    // Below the floor the floor applies; above the picture, no upscaling.
    let floor = GifClip::decode(&file.0, 1, &GO, |_| {});
    let large = GifClip::decode(&file.0, 4000, &GO, |_| {});
    assert_eq!(small.map(|clip| clip.edge()), Ok(32));
    assert_eq!(floor.map(|clip| clip.edge()), Ok(16));
    assert_eq!(large.map(|clip| clip.edge()), Ok(64));
}

#[test]
fn malformed_bytes_do_not_panic() {
    let gif = encode(&[RED, BLUE, RED]);
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("empty.gif", Vec::new()),
        ("short.gif", b"GIF8".to_vec()),
        ("header.gif", b"GIF89a".to_vec()),
        (
            "junk.gif",
            b"GIF89a\xff\xff\xff\xff\x00\x00\x00junkjunkjunk".to_vec(),
        ),
        (
            "half.gif",
            gif.get(..gif.len() / 2).unwrap_or_default().to_vec(),
        ),
        (
            "tail.gif",
            gif.get(..gif.len().saturating_sub(3))
                .unwrap_or_default()
                .to_vec(),
        ),
    ];
    for (name, bytes) in cases {
        let file = TempFile::new(name, &bytes);
        // Any outcome but a panic: an error, or the frames before the
        // damage.
        let _ = GifClip::decode(&file.0, 320, &GO, |_| {});
    }
    // An MP4 labelled `image/gif` is handed to the video player.
    let mp4 = TempFile::new("clip.gif", b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00");
    assert_eq!(
        GifClip::decode(&mp4.0, 320, &GO, |_| {}).err(),
        Some(GifError::NotGif)
    );
}

#[test]
fn an_oversized_canvas_is_rejected() {
    assert!(canvas_fits(1, 1));
    assert!(canvas_fits(4000, 4000));
    assert!(!canvas_fits(0, 10));
    assert!(!canvas_fits(MAX_CANVAS_SIDE + 1, 1));
    assert!(!canvas_fits(1, MAX_CANVAS_SIDE + 1));
    // Inside the side bound, over the pixel bound.
    assert!(!canvas_fits(MAX_CANVAS_SIDE, MAX_CANVAS_SIDE));
    // A small GIF whose header claims an 8192 x 4 canvas: refused before
    // any frame.
    let mut bytes = encode(&[RED]);
    let width = bytes.get_mut(6..8).expect("logical screen width");
    width.copy_from_slice(&8192u16.to_le_bytes());
    let file = TempFile::new("wide.gif", &bytes);
    assert_eq!(
        GifClip::decode(&file.0, 320, &GO, |_| {}).err(),
        Some(GifError::TooLarge)
    );
}

/// Frames of distinct colors; `cancel` is set once `cancel_after` were
/// handed out, and frame `fail_at` (if any) fails.
fn frames<'a>(
    count: usize,
    fail_at: Option<usize>,
    cancel: Option<(&'a AtomicBool, usize)>,
    handed: &'a Cell<usize>,
) -> impl Iterator<Item = ImageResult<Frame>> + 'a {
    std::iter::from_fn(move || {
        let n = handed.get();
        if n >= count {
            return None;
        }
        handed.set(n + 1);
        if let Some((flag, after)) = cancel
            && n + 1 >= after
        {
            flag.store(true, Ordering::Relaxed);
        }
        if Some(n) == fail_at {
            return Some(Err(image::ImageError::IoError(std::io::Error::other(
                "damaged frame",
            ))));
        }
        let shade = u8::try_from(n % 250).unwrap_or(0);
        Some(Ok(Frame::from_parts(
            solid([shade, 0, 0, 255]),
            0,
            0,
            Delay::from_numer_denom_ms(40, 1),
        )))
    })
}

#[test]
fn cancelling_stops_decoding_between_frames() {
    let cancel = AtomicBool::new(false);
    let handed = Cell::new(0);
    let result = collect(
        frames(50, None, Some((&cancel, 3)), &handed),
        320,
        &cancel,
        |_| {},
    );
    assert_eq!(result.err(), Some(GifError::Cancelled));
    // No frame was decoded after the flag was set.
    assert_eq!(handed.get(), 3);
    // Set before the start: nothing is read.
    let handed = Cell::new(0);
    let result = collect(frames(50, None, None, &handed), 320, &cancel, |_| {});
    assert_eq!(result.err(), Some(GifError::Cancelled));
    assert_eq!(handed.get(), 0);
    // A real file too.
    let file = TempFile::new("cancel.gif", &encode(&[RED, BLUE]));
    assert_eq!(
        GifClip::decode(&file.0, 320, &cancel, |_| {}).err(),
        Some(GifError::Cancelled)
    );
}

#[test]
fn a_late_frame_that_fails_keeps_the_frames_before_it() {
    let handed = Cell::new(0);
    let clip =
        collect(frames(10, Some(4), None, &handed), 320, &GO, |_| {}).expect("the first four");
    assert_eq!(clip.held(), 4);
    assert_eq!(clip.total(), ms(160));
    // A first frame that fails leaves nothing to play.
    let handed = Cell::new(0);
    let result = collect(frames(10, Some(0), None, &handed), 320, &GO, |_| {});
    assert_eq!(result.err(), Some(GifError::Empty));
}

#[test]
fn reading_stops_at_the_frame_bound() {
    let handed = Cell::new(0);
    // Equal pictures merge, so the frame count bounds what is read.
    let same = std::iter::from_fn(|| {
        handed.set(handed.get() + 1);
        Some(Ok(Frame::from_parts(
            solid(RED),
            0,
            0,
            Delay::from_numer_denom_ms(20, 1),
        )))
    });
    let clip = collect(same, 320, &GO, |_| {}).expect("clip");
    assert_eq!(clip.held(), 1);
    assert_eq!(handed.get(), super::MAX_FRAMES);
    assert!(clip.total() <= super::MAX_TOTAL);
}

#[test]
fn a_gif_plays_from_its_first_picture_while_the_rest_decodes() {
    let handed = Cell::new(0);
    let read_at_start = Cell::new(None);
    let playing = std::cell::RefCell::new(None);
    let clip = collect(frames(10, None, None, &handed), 320, &GO, |clip| {
        read_at_start.set(Some(handed.get()));
        let first = clip.frame_at(ms(0)).expect("a picture");
        // Past what has decoded, the last picture holds; it does not loop.
        assert!(Arc::ptr_eq(&first, &clip.frame_at(ms(1000)).expect("held")));
        assert!(clip.fps() > 0, "redrawn while more pictures come");
        *playing.borrow_mut() = Some(clip);
    })
    .expect("clip");
    assert_eq!(read_at_start.get(), Some(1), "started after one frame");
    let playing = playing.borrow_mut().take().expect("started");
    assert!(Arc::ptr_eq(&playing, &clip), "the same clip fills in");
    assert_eq!(clip.held(), 10);
    // Read whole, it loops.
    let first = clip.frame_at(ms(0)).expect("a picture");
    assert!(Arc::ptr_eq(
        &first,
        &clip.frame_at(ms(400)).expect("looped")
    ));
}

/// Decoding ran slower than playback: the row waits at the last decoded
/// picture and goes on with the next one, instead of jumping back into the
/// loop once the clip is whole.
#[test]
fn a_gif_that_decodes_slower_than_it_plays_goes_on_where_it_waited() {
    const GREEN: [u8; 4] = [0, 255, 0, 255];
    const WHITE: [u8; 4] = [255, 255, 255, 255];
    let mut builder = ClipBuilder::default();
    for color in [RED, BLUE, GREEN] {
        assert!(builder.push(solid(color), ms(100)));
    }
    let clip = builder.started().expect("playing");
    let started = std::time::Instant::now();
    let third = clip.frame_for(Some(started), ms(250)).expect("third");
    // 500 ms in, only 300 ms have decoded: the third picture holds.
    let held = clip.frame_for(Some(started), ms(500)).expect("held");
    assert!(Arc::ptr_eq(&third, &held));
    assert!(builder.push(solid(WHITE), ms(100)));
    let clip = builder.finish().expect("whole");
    // The fourth picture follows, not the second.
    let next = clip.frame_for(Some(started), ms(520)).expect("next");
    let fourth = clip.frame_at(ms(350)).expect("fourth");
    assert!(Arc::ptr_eq(&next, &fourth));
    // A new play starts from the first picture.
    let first = clip.frame_at(ms(0)).expect("first");
    let replay = clip
        .frame_for(Some(started + ms(1)), ms(0))
        .expect("replay");
    assert!(Arc::ptr_eq(&replay, &first));
}
