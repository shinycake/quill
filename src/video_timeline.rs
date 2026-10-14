//! The geometry and pointer rules of the video editor's controls (tdesktop
//! `Editor::VideoTimeline`, `Editor::VideoQualitySlider` and the editor's
//! `Crop`), kept apart from the UI so they can be tested: where a time sits
//! on the strip, which part of it a press grabs, how a drag moves the
//! range, the cover and the crop, and the size the edited file will have
//! (`Media::Encode::EstimateTranscodedSize`). The drawing is
//! `ui/video_editor_timeline.rs` and `ui/video_editor_tools.rs`.
#![deny(clippy::indexing_slicing)]

use crate::video_edit::{Crop, VideoEdits, VideoSource, scaled_size};

/// `videoTimelineStripHeight`: the row of frames.
pub const STRIP_HEIGHT: f32 = 54.0;
/// `videoTimelineLabelHeight`: the duration and size line above it.
pub const LABEL_HEIGHT: f32 = 18.0;
/// `videoTimelineLabelSkip`: room for the cover dot between the two.
pub const LABEL_SKIP: f32 = 17.0;
/// `videoTimelineRadius`.
pub const RADIUS: f32 = 6.0;
/// `videoTimelineHandleWidth`: each side of the selection frame.
pub const HANDLE_WIDTH: f32 = 11.0;
/// `videoTimelineHandleGripWidth`: the frame's top and bottom lines and the
/// grips on the handles.
pub const GRIP_WIDTH: f32 = 2.0;
/// `videoTimelineHandleGripHeight`.
pub const GRIP_HEIGHT: f32 = 16.0;
/// `videoTimelineHandleHitSlop`.
pub const HIT_SLOP: f32 = 12.0;
/// `videoTimelinePlayheadWidth`.
pub const PLAYHEAD_WIDTH: f32 = 3.0;
/// `videoTimelinePlayheadOutline`.
pub const PLAYHEAD_OUTLINE: f32 = 1.0;
/// `videoTimelinePlayheadOverflow`: how far the head sticks out of the strip.
pub const PLAYHEAD_OVERFLOW: f32 = 5.0;
/// `videoTimelineDotSize` / `videoTimelineDotActiveSize`.
pub const DOT_SIZE: f32 = 5.0;
pub const DOT_ACTIVE_SIZE: f32 = 9.0;
/// `videoTimelineDotSkip`.
pub const DOT_SKIP: f32 = 2.0;
/// `videoTimelineSizeSkip`: least gap between the duration and the size.
pub const SIZE_SKIP: f32 = 12.0;
/// The whole control (`VideoTimeline::resizeGetHeight`).
pub const HEIGHT: f32 =
    LABEL_HEIGHT + LABEL_SKIP + STRIP_HEIGHT + PLAYHEAD_OVERFLOW + PLAYHEAD_OUTLINE;
/// `kMaxFrames`: the most frames the strip shows.
pub const MAX_FRAMES: usize = 24;

/// The part of the timeline a press took hold of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grab {
    /// The start handle.
    Left,
    /// The end handle.
    Right,
    /// The playhead inside the selection: picks the cover frame.
    Head,
    /// Outside the selection: the whole window moves there.
    Window,
}

/// What a drag step changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Moved {
    Nothing,
    /// The range moved; the player should show this edge.
    Trim(i64),
    /// The cover moved to this time.
    Cover(i64),
}

/// The selection on the strip, in milliseconds of the source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Timeline {
    pub duration: i64,
    /// The longest selection (`maxDuration`), the clip's length when free.
    pub max_duration: i64,
    /// The shortest selection the data asks for (`minDuration`).
    pub min_duration: i64,
    pub from: i64,
    pub till: i64,
    pub cover: i64,
    /// Width of the whole control in px, handles included.
    pub width: f32,
}

impl Timeline {
    /// A timeline restored from earlier edits (`VideoTimeline` constructor):
    /// a zero `till` selects the whole allowed window.
    pub fn new(duration: i64, max_duration: i64, min_duration: i64, edits: &VideoEdits) -> Self {
        let duration = duration.max(1);
        let max_duration = if max_duration > 0 {
            max_duration.min(duration)
        } else {
            duration
        };
        let min_duration = min_duration.clamp(0, max_duration);
        let from = edits.from_ms.clamp(0, (duration - min_duration).max(0));
        let limit = (from + max_duration).min(duration);
        // A restored range keeps at least the minimum (`from` above leaves
        // room for it).
        let till = if edits.till_ms > from {
            edits.till_ms.max(from + min_duration).min(limit)
        } else {
            limit
        };
        Self {
            duration,
            max_duration,
            min_duration,
            from,
            till,
            cover: edits.cover_ms.clamp(from, till),
            width: 0.0,
        }
    }

    /// Left edge and width of the frame strip, inside the two handles.
    pub fn strip(&self) -> (f32, f32) {
        (HANDLE_WIDTH, (self.width - HANDLE_WIDTH * 2.0).max(1.0))
    }

    pub fn time_at(&self, x: f32) -> i64 {
        let (left, width) = self.strip();
        let shift = (x - left).clamp(0.0, width);
        (f64::from(shift) * self.duration as f64 / f64::from(width)).round() as i64
    }

    pub fn x_at(&self, time: i64) -> f32 {
        let (left, width) = self.strip();
        let time = time.clamp(0, self.duration);
        left + (time as f64 * f64::from(width) / self.duration as f64).round() as f32
    }

    /// `VideoTimeline::grabAt`: the handles reach a little outside the frame
    /// and, on a wide selection, a little inside it.
    pub fn grab_at(&self, x: f32) -> Grab {
        let (left, right) = (self.x_at(self.from), self.x_at(self.till));
        let span = (right - left).max(1.0);
        let inside = HIT_SLOP.min((span / 3.0).floor());
        if x >= left - HANDLE_WIDTH - HIT_SLOP && x <= left + inside {
            Grab::Left
        } else if x <= right + HANDLE_WIDTH + HIT_SLOP && x >= right - inside {
            Grab::Right
        } else if x > left && x < right {
            Grab::Head
        } else {
            Grab::Window
        }
    }

    /// The pointer's offset from the grabbed handle, kept while dragging so
    /// the handle doesn't jump under it.
    pub fn grab_shift(&self, grab: Grab, x: f32) -> f32 {
        match grab {
            Grab::Left => x - self.x_at(self.from),
            Grab::Right => x - self.x_at(self.till),
            Grab::Head | Grab::Window => 0.0,
        }
    }

    /// `minSelection`: a long clip still keeps the head reachable between
    /// the handles.
    pub fn min_selection(&self) -> i64 {
        let (_, width) = self.strip();
        let pixels = PLAYHEAD_WIDTH + HIT_SLOP;
        let by_pixels = if width > pixels {
            (f64::from(pixels) * self.duration as f64 / f64::from(width)).round() as i64
        } else {
            self.duration
        };
        self.min_duration.max(by_pixels).clamp(0, self.max_duration)
    }

    /// `applyGrab`: move what `grab` holds to `x` (the shift already taken
    /// off).
    pub fn drag(&mut self, grab: Grab, x: f32) -> Moved {
        let at = self.time_at(x);
        let minimum = self.min_selection();
        match grab {
            Grab::Left => {
                let highest = (self.till - minimum).max(0);
                self.from = at.clamp(0, highest);
                if self.till - self.from > self.max_duration {
                    self.till = self.from + self.max_duration;
                }
                self.cover = self.cover.clamp(self.from, self.till);
                Moved::Trim(self.from)
            }
            Grab::Right => {
                let lowest = (self.from + minimum).min(self.duration);
                self.till = at.clamp(lowest, self.duration);
                if self.till - self.from > self.max_duration {
                    self.from = self.till - self.max_duration;
                }
                self.cover = self.cover.clamp(self.from, self.till);
                Moved::Trim(self.till)
            }
            Grab::Head => {
                let cover = at.clamp(self.from, self.till);
                if cover == self.cover {
                    return Moved::Nothing;
                }
                self.cover = cover;
                Moved::Cover(cover)
            }
            Grab::Window => self.move_window_to(at),
        }
    }

    /// `moveWindowTo`: center the selection on `center`, same length.
    fn move_window_to(&mut self, center: i64) -> Moved {
        let span = self.till - self.from;
        let from = (center - span / 2).clamp(0, (self.duration - span).max(0));
        if from == self.from {
            return Moved::Nothing;
        }
        self.from = from;
        self.till = from + span;
        self.cover = self.cover.clamp(self.from, self.till);
        Moved::Trim(self.from)
    }

    /// The left edge of the playhead at `playback` (the cover when nothing
    /// played yet), kept between the handles.
    pub fn head_x(&self, playback: Option<i64>) -> f32 {
        let x = self.x_at(playback.unwrap_or(self.cover)) - PLAYHEAD_WIDTH / 2.0;
        let (left, right) = (self.x_at(self.from), self.x_at(self.till));
        x.clamp(left, (right - PLAYHEAD_WIDTH).max(left))
    }

    /// Center of the cover dot, from the control's top-left
    /// (`VideoTimeline::coverDot`).
    pub fn cover_dot(&self) -> (f32, f32) {
        (
            self.x_at(self.cover),
            LABEL_HEIGHT + LABEL_SKIP
                - PLAYHEAD_OVERFLOW
                - PLAYHEAD_OUTLINE
                - DOT_SKIP
                - DOT_ACTIVE_SIZE / 2.0,
        )
    }

    /// The duration line: the whole length when nothing is trimmed, else
    /// `00:01.2 – 00:05.0` (`paintDuration`).
    pub fn duration_label(&self) -> String {
        let stamp = |ms: i64| crate::video_edit::format_clock(ms);
        if self.till - self.from >= self.duration {
            stamp(self.till - self.from)
        } else {
            format!("{} – {}", stamp(self.from), stamp(self.till))
        }
    }

    /// Where the duration text of `text_width` starts: centered on the
    /// selection, kept inside `available` px from the left.
    pub fn duration_x(&self, text_width: f32, available: f32) -> f32 {
        let center = (self.x_at(self.from) + self.x_at(self.till)) / 2.0;
        (center - text_width / 2.0).clamp(0.0, (available - text_width).max(0.0))
    }

    /// The frames of the strip for a clip of `width`×`height` (`reloadFrames`):
    /// how many and at which times, each frame as wide as the clip's aspect
    /// allows at the strip's height.
    pub fn frame_times(&self, width: u32, height: u32) -> Vec<i64> {
        let (_, strip) = self.strip();
        if width == 0 || height == 0 || strip < 1.0 {
            return Vec::new();
        }
        let aspect = (STRIP_HEIGHT * width as f32 / height as f32)
            .round()
            .max(1.0);
        let count = ((strip / aspect).ceil() as usize).clamp(1, MAX_FRAMES);
        (0..count)
            .map(|i| ((i as f64 + 0.5) * self.duration as f64 / count as f64).round() as i64)
            .collect()
    }
}

/// `videoQualitySliderSideSkip`: the outer dots' distance from the edges.
pub const QUALITY_SIDE_SKIP: f32 = 18.0;
/// `videoQualitySliderHeight`.
pub const QUALITY_HEIGHT: f32 = 32.0;
/// `videoQualitySliderDotSize` / `videoQualitySliderDotActiveSize`.
pub const QUALITY_DOT: f32 = 7.0;
pub const QUALITY_DOT_ACTIVE: f32 = 11.0;
/// `videoQualitySliderLineWidth` / `videoQualitySliderLineGap`.
pub const QUALITY_LINE: f32 = 2.0;
pub const QUALITY_LINE_GAP: f32 = 4.0;
/// `videoQualitySliderLabelSkip`.
pub const QUALITY_LABEL_SKIP: f32 = 5.0;

/// `VideoQualitySlider::dotX`: the center of dot `index` of `count`.
pub fn quality_dot_x(index: usize, count: usize, width: f32) -> f32 {
    if count < 2 {
        return width / 2.0;
    }
    let available = (width - 2.0 * QUALITY_SIDE_SKIP).max(1.0);
    QUALITY_SIDE_SKIP + available * index as f32 / (count - 1) as f32
}

/// `VideoQualitySlider::indexAt`: the dot nearest to `x`.
pub fn quality_index_at(x: f32, count: usize, width: f32) -> usize {
    if count < 2 {
        return 0;
    }
    let available = (width - 2.0 * QUALITY_SIDE_SKIP).max(1.0);
    let shifted = (x - QUALITY_SIDE_SKIP).clamp(0.0, available);
    ((shifted * (count - 1) as f32 / available).round() as usize).min(count - 1)
}

/// The crop on the screen, as parts (0 to 1) of the turned, mirrored frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropArea {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl CropArea {
    pub const FULL: Self = Self {
        x: 0.0,
        y: 0.0,
        w: 1.0,
        h: 1.0,
    };

    pub fn of(crop: Option<Crop>) -> Self {
        let crop = crop.unwrap_or(Crop::FULL);
        let part = |value: u16| f32::from(value) / 1000.0;
        Self {
            x: part(crop.x),
            y: part(crop.y),
            w: part(crop.w),
            h: part(crop.h),
        }
    }

    /// Back to thousandths; `None` for the whole frame.
    pub fn to_crop(self) -> Option<Crop> {
        let part = |value: f32| (value.clamp(0.0, 1.0) * 1000.0).round() as u16;
        let crop = Crop {
            x: part(self.x),
            y: part(self.y),
            w: part(self.w),
            h: part(self.h),
        }
        .clamped();
        (crop != Crop::FULL).then_some(crop)
    }
}

/// What a press on the picture took hold of (the editor `Crop`'s edges).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CropGrab {
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
}

impl CropGrab {
    /// Inside the crop, away from its edges: the crop moves.
    pub fn moves(self) -> bool {
        !(self.left || self.top || self.right || self.bottom)
    }
}

/// `photoEditorCropPointSize`: the grab area around an edge, in px.
pub const CROP_POINT_SIZE: f32 = 10.0;
/// `photoEditorCropMinSize`: the smallest crop side, in px on the screen.
pub const CROP_MIN_SIZE: f32 = 20.0;

/// Which edges of `area` a press at `(x, y)` px holds, on a picture shown
/// at `shown` px; `None` outside the crop and its edges.
pub fn crop_grab_at(area: CropArea, (x, y): (f32, f32), shown: (f32, f32)) -> Option<CropGrab> {
    let (width, height) = shown;
    let (left, top) = (area.x * width, area.y * height);
    let (right, bottom) = (left + area.w * width, top + area.h * height);
    let reach = CROP_POINT_SIZE;
    if x < left - reach || x > right + reach || y < top - reach || y > bottom + reach {
        return None;
    }
    Some(CropGrab {
        left: (x - left).abs() <= reach,
        top: (y - top).abs() <= reach,
        right: (x - right).abs() <= reach,
        bottom: (y - bottom).abs() <= reach,
    })
}

/// `start` with the grabbed edges moved by `(dx, dy)` px on a picture shown
/// at `shown` px. `square` keeps the crop a square of pixels on a frame of
/// that pixel aspect (width over height), the edge that moved most leading.
pub fn crop_dragged(
    start: CropArea,
    grab: CropGrab,
    (dx, dy): (f32, f32),
    shown: (f32, f32),
    square: Option<f32>,
) -> CropArea {
    let (width, height) = (shown.0.max(1.0), shown.1.max(1.0));
    let (dx, dy) = (dx / width, dy / height);
    if grab.moves() {
        return CropArea {
            x: within(start.x + dx, 0.0, 1.0 - start.w),
            y: within(start.y + dy, 0.0, 1.0 - start.h),
            ..start
        };
    }
    let (min_w, min_h) = (
        (CROP_MIN_SIZE / width).min(1.0),
        (CROP_MIN_SIZE / height).min(1.0),
    );
    let (mut left, mut top) = (start.x, start.y);
    let (mut right, mut bottom) = (start.x + start.w, start.y + start.h);
    if grab.left {
        left = within(left + dx, 0.0, right - min_w);
    }
    if grab.right {
        right = within(right + dx, left + min_w, 1.0);
    }
    if grab.top {
        top = within(top + dy, 0.0, bottom - min_h);
    }
    if grab.bottom {
        bottom = within(bottom + dy, top + min_h, 1.0);
    }
    let Some(aspect) = square.filter(|aspect| *aspect > 0.0) else {
        return CropArea {
            x: left,
            y: top,
            w: right - left,
            h: bottom - top,
        };
    };
    // In frame-height units a square is `w * aspect == h`.
    let horizontal = grab.left || grab.right;
    let vertical = grab.top || grab.bottom;
    let side = match (horizontal, vertical) {
        (true, true) => ((right - left) * aspect).min(bottom - top),
        (true, false) => (right - left) * aspect,
        _ => bottom - top,
    }
    .min(aspect.min(1.0))
    .min(1.0);
    let (w, h) = (side / aspect, side);
    let (w, h) = (w.min(1.0), h.min(1.0));
    let x = if grab.left {
        right - w
    } else if grab.right {
        left
    } else {
        (left + right - w) / 2.0
    };
    let y = if grab.top {
        bottom - h
    } else if grab.bottom {
        top
    } else {
        (top + bottom - h) / 2.0
    };
    CropArea {
        x: within(x, 0.0, 1.0 - w),
        y: within(y, 0.0, 1.0 - h),
        w,
        h,
    }
}

/// `kMinBitrate` / `kMaxBitrate` of the encoder's quality mode.
const MIN_BITRATE: f64 = 600_000.0;
const MAX_BITRATE: f64 = 6_800_000.0;
/// `kRateControlFps`.
const RATE_CONTROL_FPS: f64 = 60.0;
/// The sound Quill's encoder writes (`-b:a 128k`).
const AUDIO_BITRATE: f64 = 128_000.0;
/// `kContainerBytes`, `kContainerBytesPerVideoFrame`,
/// `kContainerBytesPerAudioFrame` (48 kHz, 1024 samples a frame).
const CONTAINER_BYTES: f64 = 2_000.0;
const CONTAINER_PER_VIDEO_FRAME: f64 = 12.0;
const CONTAINER_PER_AUDIO_FRAME: f64 = 10.0;
const AUDIO_FRAMES_PER_SECOND: f64 = 48_000.0 / 1_024.0;

/// The size of the file the edits make (`EstimateTranscodedSize`), and
/// whether it is an estimate: an untouched file is sent as it is.
pub fn estimated_size(edits: &VideoEdits, source: &VideoSource) -> Option<(u64, bool)> {
    let effective = VideoEdits {
        gif: edits.gif && source.has_audio,
        ..*edits
    };
    if !effective.needs_transcode(source.duration_ms) {
        return (source.file_size > 0).then_some((source.file_size, false));
    }
    let (width, height) = effective.edited_size(source);
    let (width, height) = scaled_size(width, height, effective.quality);
    let seconds = effective.length_ms() as f64 / 1000.0;
    if width == 0 || height == 0 || seconds <= 0.0 {
        return None;
    }
    let fps = if source.fps_milli > 0 {
        (f64::from(source.fps_milli) / 1000.0).clamp(1.0, 120.0)
    } else {
        30.0
    };
    let wanted =
        (f64::from(width) * f64::from(height) * fps * 0.07).clamp(MIN_BITRATE, MAX_BITRATE);
    let bitrate = if source.video_bitrate > 0 {
        wanted.min(source.video_bitrate as f64)
    } else {
        wanted
    };
    let video = bitrate * (RATE_CONTROL_FPS / fps).min(1.0);
    let sound = source.has_audio && !effective.gif;
    let audio = if sound { AUDIO_BITRATE } else { 0.0 };
    let container = CONTAINER_BYTES
        + CONTAINER_PER_VIDEO_FRAME * seconds * fps
        + if sound {
            CONTAINER_PER_AUDIO_FRAME * seconds * AUDIO_FRAMES_PER_SECOND
        } else {
            0.0
        };
    let bytes = (video + audio) * seconds / 8.0 + container;
    Some((bytes.round().max(1.0) as u64, true))
}

#[cfg(test)]
#[path = "video_timeline_tests.rs"]
mod tests;

/// `value` kept in `low..=high`, with `low` winning when the range is
/// empty: `f32::clamp` panics on that, and a crop smaller than the
/// minimum handle size (a tiny picture) makes it empty.
fn within(value: f32, low: f32, high: f32) -> f32 {
    value.min(high).max(low)
}
