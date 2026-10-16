//! The Windows live NV12 path as plain data: packing, the colour, the
//! buffer pool, the pause after a refusal and the frame's draw state. A
//! `SurfaceFrame` is plain data, so these run on every machine.
use super::super::backend_ffmpeg::NV12_RETRY;
use super::super::surface::gpui_color;
use super::{LIVE_COLOR, LiveNv12, POOL, fallback_image, fill};
use gpui_kit::{SurfaceDrawState, SurfaceFormat};
use quill::calls::live_video::convert::{
    I420, gradient_frame, i420_to_bgra, i420_to_nv12_packed, nv12_packed_len,
};
use quill::video_decode::{YuvColor, YuvMatrix, nv12_to_bgra};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn planes(width: u16, height: u16) -> Vec<u8> {
    gradient_frame(width, height, 7)
}

#[test]
fn a_frame_packs_into_an_nv12_surface_frame_with_the_live_colour() {
    let (w, h) = (64u16, 36u16);
    let planes = planes(w, h);
    let src = I420::new(&planes, w, h).expect("frame");
    let mut state = LiveNv12::default();
    let now = Instant::now();
    let bytes = state.buffer(64, 36, now).expect("an even frame takes NV12");
    assert_eq!(bytes.len(), 64 * 36 * 3 / 2);
    let picture = fill(&src, bytes).expect("filled");
    assert_eq!(picture.size(), (64, 36));
    assert_eq!(picture.frame.format(), SurfaceFormat::Nv12);
    assert_eq!(picture.frame.yuv(), gpui_color(LIVE_COLOR));
    assert_eq!(picture.frame.draw_state(), SurfaceDrawState::Pending);
    let mut expected = vec![0u8; nv12_packed_len(64, 36).expect("even")];
    assert!(i420_to_nv12_packed(&src, &mut expected));
    assert_eq!(picture.frame.bytes(), expected.as_slice());
}

#[test]
fn odd_sides_stay_bgra() {
    let state = LiveNv12::default();
    let now = Instant::now();
    for (w, h) in [(63, 36), (64, 35), (1, 1)] {
        assert!(!state.takes(w, h, now), "{w}x{h}");
    }
    assert!(state.takes(2, 2, now));
    // A buffer for another size is not filled.
    let planes = planes(64, 36);
    let src = I420::new(&planes, 64, 36).expect("frame");
    assert!(fill(&src, Arc::from(vec![0u8; 100])).is_none());
    // Nor a shared one: nothing may read it while it is written.
    let shared: Arc<[u8]> = Arc::from(vec![0u8; 64 * 36 * 3 / 2]);
    let _reader = shared.clone();
    assert!(fill(&src, shared).is_none());
}

/// The NV12 frame's colour on the GPU (and its CPU fallback image) must
/// be the BGRA path's colour (`i420_to_bgra`, BT.601 full range), at
/// every size: the heights where a BT.709 pick would apply too.
#[test]
fn the_colour_matches_the_bgra_path_at_every_size() {
    for (w, h) in [(640u16, 360u16), (1280, 720), (1920, 1080)] {
        let planes = planes(w, h);
        let src = I420::new(&planes, w, h).expect("frame");
        let bgra = i420_to_bgra(&src).expect("bgra");
        let mut nv12 = vec![0u8; nv12_packed_len(w.into(), h.into()).expect("even")];
        assert!(i420_to_nv12_packed(&src, &mut nv12));
        let diff = |color: YuvColor| {
            let back = nv12_to_bgra(&nv12, w.into(), h.into(), color).expect("converted");
            bgra.iter()
                .zip(&back)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap_or(0)
        };
        assert!(diff(LIVE_COLOR) <= 1, "{w}x{h}: {}", diff(LIVE_COLOR));
        let bt709 = YuvColor {
            matrix: YuvMatrix::Bt709,
            full_range: false,
        };
        assert!(diff(bt709) > 10, "{w}x{h}: BT.709 would show other colours");
    }
}

#[test]
fn the_fallback_image_is_the_frame_on_the_cpu() {
    let planes = planes(32, 18);
    let src = I420::new(&planes, 32, 18).expect("frame");
    let mut state = LiveNv12::default();
    let bytes = state.buffer(32, 18, Instant::now()).expect("nv12");
    let picture = fill(&src, bytes).expect("filled");
    let image = fallback_image(&picture.frame).expect("image");
    let size = image.size(0);
    assert_eq!((size.width.0, size.height.0), (32, 18));
    let bgra = i420_to_bgra(&src).expect("bgra");
    let shown = image.as_bytes(0).expect("bytes");
    assert_eq!(shown.len(), bgra.len());
    assert!(shown.iter().zip(&bgra).all(|(a, b)| a.abs_diff(*b) <= 1));
}

/// Show `bytes` packed from a 64 × 36 frame; the frame back.
fn show(state: &mut LiveNv12, bytes: Arc<[u8]>) -> Arc<gpui_kit::SurfaceFrame> {
    let planes = planes(64, 36);
    let src = I420::new(&planes, 64, 36).expect("frame");
    let (_, _, shown) = fill(&src, bytes)
        .expect("filled")
        .into_shown()
        .expect("shown");
    state.shown(Some(shown));
    state.shown_frame().expect("on screen")
}

#[test]
fn a_buffer_is_filled_again_only_once_no_frame_holds_it() {
    let mut state = LiveNv12::default();
    let now = Instant::now();
    let first = state.buffer(64, 36, now).expect("nv12");
    let first_at = Arc::as_ptr(&first).cast::<u8>();
    let frame = show(&mut state, first);
    // Its frame is on screen: a new buffer.
    let second = state.buffer(64, 36, now).expect("nv12");
    assert_ne!(Arc::as_ptr(&second).cast::<u8>(), first_at);
    drop(second);
    // The frame is gone everywhere: the same memory again.
    state.shown(None);
    drop(frame);
    let again = state.buffer(64, 36, now).expect("nv12");
    assert_eq!(Arc::as_ptr(&again).cast::<u8>(), first_at);
}

#[test]
fn the_pool_keeps_a_few_buffers_of_the_last_size() {
    let mut state = LiveNv12::default();
    let now = Instant::now();
    let mut frames = Vec::new();
    for _ in 0..POOL + 3 {
        let bytes = state.buffer(64, 36, now).expect("nv12");
        frames.push(show(&mut state, bytes));
    }
    assert_eq!(state.pooled().len(), POOL);
    // Another size: the old buffers leave the pool (and go with their
    // frames).
    let planes = planes(32, 18);
    let src = I420::new(&planes, 32, 18).expect("frame");
    let small = state.buffer(32, 18, now).expect("nv12");
    let (_, _, shown) = fill(&src, small)
        .expect("filled")
        .into_shown()
        .expect("shown");
    state.shown(Some(shown));
    assert_eq!(state.pooled().len(), 1);
    let pooled = state.pooled();
    drop(frames);
    state.release();
    assert!(state.shown_frame().is_none());
    assert!(state.pooled().is_empty());
    assert!(
        pooled.iter().all(|bytes| bytes.strong_count() == 0),
        "freed"
    );
}

#[test]
fn a_refused_frame_pauses_nv12_then_it_comes_back() {
    let mut state = LiveNv12::default();
    let now = Instant::now();
    let bytes = state.buffer(64, 36, now).expect("nv12");
    let frame = show(&mut state, bytes);
    // Not drawn yet: the element looks again.
    assert!(state.unsettled());
    frame.mark_drawn();
    assert!(!state.unsettled());
    frame.mark_refused();
    assert!(state.unsettled());
    let image = state.refused(now).expect("the same picture as an image");
    let size = image.size(0);
    assert_eq!((size.width.0, size.height.0), (64, 36));
    assert!(state.shown_frame().is_none(), "the refused frame goes");
    assert!(!state.unsettled());
    // Images for the file player's pause...
    assert!(!state.takes(64, 36, now));
    assert!(state.buffer(64, 36, now + NV12_RETRY / 2).is_none());
    assert!(
        state
            .buffer(
                64,
                36,
                now + NV12_RETRY.saturating_sub(Duration::from_millis(1))
            )
            .is_none()
    );
    // ...then NV12 again.
    assert!(state.buffer(64, 36, now + NV12_RETRY).is_some());
    // A refusal with nothing on screen still pauses, and makes nothing.
    assert!(state.refused(now + NV12_RETRY).is_none());
    assert!(!state.takes(64, 36, now + NV12_RETRY));
}

#[test]
fn an_ended_feed_lets_its_pool_go_and_keeps_its_picture() {
    let mut state = LiveNv12::default();
    let now = Instant::now();
    let bytes = state.buffer(64, 36, now).expect("nv12");
    let frame = show(&mut state, bytes);
    let pooled = state.pooled();
    assert_eq!(pooled.len(), 1);
    state.feed_ended();
    assert!(state.pooled().is_empty());
    assert!(state.shown_frame().is_some(), "the last picture stays");
    drop(frame);
    state.release();
    assert!(
        pooled.iter().all(|bytes| bytes.strong_count() == 0),
        "freed"
    );
}
