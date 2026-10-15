//! `StreamTile`: the picture of a stream-mode group call (a channel's
//! RTMP live stream; PR 3's live stories reuse it with `Cover`).
//!
//! ntgcalls hands each decoded picture to the driver's per-chat stream
//! slot; the owner turns it into an image once per frame sequence and
//! passes it here. While the stream plays, the tile asks the frame clock
//! for its next redraw at the stream's rate (`request_video_frame`), so no
//! timer of its own runs and an idle window costs nothing. Before the
//! first picture it shows the placeholder: tdesktop's viewport draws the
//! tile peer's photo blurred with radius 15 while a track has no frame
//! (`Viewport::RendererSW::validateUserpicFrame`).
//!
//! The tile reports its width on every paint where it changed, so the
//! owner can pick the quality bucket (tdesktop `updateRequestedVideoChannels`).
#![deny(clippy::indexing_slicing)]

use crate::ui::frame_clock::{VideoFrameAsk, request_video_frame};
use crate::ui::video_player::FrameRequested;
use gpui_kit::*;
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Streams arrive at up to 30 pictures a second.
pub(crate) const STREAM_INTERVAL: Duration = Duration::from_micros(33_333);
/// tdesktop `kBlurRadius` (calls_group_viewport_raster.cpp).
pub(crate) const PHOTO_BLUR_RADIUS: u32 = 15;

/// Called with the tile's width in logical pixels when it changes.
pub(crate) type WidthReport = Rc<dyn Fn(f32, &mut App)>;

/// Per-tile state the owner keeps between renders.
#[derive(Clone, Default)]
pub(crate) struct StreamTileState {
    /// The frame request in flight (shared with the frame clock).
    pub(crate) requested: FrameRequested,
    /// The last width reported.
    pub(crate) width: Rc<Cell<f32>>,
}

#[derive(IntoElement)]
pub(crate) struct StreamTile {
    id: ElementId,
    picture: Option<Arc<RenderImage>>,
    placeholder: Option<Arc<RenderImage>>,
    fit: ObjectFit,
    radius: Pixels,
    background: Hsla,
    playing: bool,
    keep_alive: bool,
    state: StreamTileState,
    on_width: Option<WidthReport>,
}

impl StreamTile {
    pub(crate) fn new(id: impl Into<ElementId>, state: &StreamTileState) -> Self {
        Self {
            id: id.into(),
            picture: None,
            placeholder: None,
            fit: ObjectFit::Contain,
            radius: px(0.),
            background: hsla(0., 0., 0., 1.),
            playing: false,
            keep_alive: true,
            state: state.clone(),
            on_width: None,
        }
    }

    /// The newest stream picture.
    pub(crate) fn picture(mut self, picture: Option<Arc<RenderImage>>) -> Self {
        self.picture = picture;
        self
    }

    /// Shown until the first picture (the blurred photo).
    pub(crate) fn placeholder(mut self, placeholder: Option<Arc<RenderImage>>) -> Self {
        self.placeholder = placeholder;
        self
    }

    pub(crate) fn fit(mut self, fit: ObjectFit) -> Self {
        self.fit = fit;
        self
    }

    /// Corner radius and the color around a letterboxed picture.
    pub(crate) fn frame(mut self, radius: Pixels, background: Hsla) -> Self {
        self.radius = radius;
        self.background = background;
        self
    }

    /// The stream runs: keep asking for frames.
    pub(crate) fn playing(mut self, playing: bool) -> Self {
        self.playing = playing;
        self
    }

    pub(crate) fn on_width(mut self, report: WidthReport) -> Self {
        self.on_width = Some(report);
        self
    }
}

impl RenderOnce for StreamTile {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let target = window.current_view();
        let image = self.picture.or(self.placeholder).map(|image| {
            // Absolute: an in-flow image takes its own height from its
            // aspect ratio and spills past the tile.
            img(ImageSource::Render(image))
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .rounded(self.radius)
                .object_fit(self.fit)
                .into_any_element()
        });
        let playing = self.playing;
        let keep_alive = self.keep_alive;
        let state = self.state;
        let on_width = self.on_width;
        let pump = canvas(
            |_, _, _| {},
            move |bounds, (), window, cx| {
                let width = f32::from(bounds.size.width);
                if (width - state.width.get()).abs() >= 0.5 {
                    state.width.set(width);
                    if let Some(report) = on_width.clone() {
                        // Not while painting: the owner updates its state.
                        window.on_next_frame(move |_, cx| report(width, cx));
                    }
                }
                if playing {
                    let now = Instant::now();
                    request_video_frame(target, stream_ask(now, &state, keep_alive), window, cx);
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        div()
            .id(self.id)
            .relative()
            .size_full()
            .overflow_hidden()
            .rounded(self.radius)
            .bg(self.background)
            .children(image)
            .child(pump)
    }
}

/// The next redraw: one stream interval from now. The display interval is
/// not known here, so it is taken as the stream's, which wakes half an
/// interval early and then takes the next display frame.
fn stream_ask(now: Instant, state: &StreamTileState, keep_alive: bool) -> VideoFrameAsk {
    VideoFrameAsk {
        next_due: now.checked_add(STREAM_INTERVAL),
        interval: STREAM_INTERVAL,
        lead: Duration::ZERO,
        plays_sound: true,
        keep_alive,
        requested: state.requested.clone(),
    }
}

/// The photo blurred as tdesktop's tile placeholder: decoded, blurred with
/// radius 15 (lib_ui `BlurLargeImage` weights, `wallpaper_raster::blur`),
/// as a GPUI image (BGRA). Runs off the UI thread.
pub(crate) fn blurred_photo(path: &Path) -> Option<Arc<RenderImage>> {
    let rgba = image::open(path).ok()?.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    let mut bgra = rgba.into_raw();
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let mut raster = quill::wallpaper_raster::Raster {
        width,
        height,
        bgra,
    };
    quill::wallpaper_raster::blur(&mut raster, PHOTO_BLUR_RADIUS);
    let image = image::RgbaImage::from_raw(raster.width, raster.height, raster.bgra)?;
    Some(Arc::new(RenderImage::new(smallvec::SmallVec::from_buf([
        image::Frame::new(image),
    ]))))
}

#[cfg(test)]
mod tests {
    use super::{STREAM_INTERVAL, StreamTileState, blurred_photo, stream_ask};
    use std::time::Instant;

    #[test]
    fn the_tile_asks_for_the_next_stream_picture() {
        let now = Instant::now();
        let ask = stream_ask(now, &StreamTileState::default(), true);
        assert_eq!(ask.next_due, Some(now + STREAM_INTERVAL));
        assert!(ask.plays_sound);
        assert!(ask.keep_alive);
    }

    #[test]
    fn a_photo_blurs_and_a_missing_one_does_not() {
        let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("docs/screenshots/fixtures/showcase/av07.png");
        let image = blurred_photo(&fixture).expect("decodes");
        let size = image.size(0);
        assert!(size.width.0 > 0 && size.height.0 > 0);
        assert!(blurred_photo(std::path::Path::new("/nonexistent.png")).is_none());
    }
}
