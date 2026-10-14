use super::*;
use crate::video_edit::{Crop, VideoEdits, VideoSource};

fn timeline(duration: i64, max: i64, min: i64) -> Timeline {
    let mut timeline = Timeline::new(duration, max, min, &VideoEdits::full(duration));
    timeline.width = 422.0;
    timeline
}

#[test]
fn a_new_timeline_selects_the_allowed_window() {
    let free = timeline(10_000, 0, 0);
    assert_eq!((free.from, free.till), (0, 10_000));
    // A sticker keeps three seconds at most.
    let sticker = timeline(10_000, 3_000, 200);
    assert_eq!((sticker.from, sticker.till), (0, 3_000));
    // Earlier edits come back, cut to the window.
    let edits = VideoEdits {
        from_ms: 2_000,
        till_ms: 9_000,
        cover_ms: 1_000,
        ..VideoEdits::full(10_000)
    };
    let restored = Timeline::new(10_000, 3_000, 0, &edits);
    assert_eq!(
        (restored.from, restored.till, restored.cover),
        (2_000, 5_000, 2_000)
    );
}

#[test]
fn times_and_positions_round_trip_inside_the_handles() {
    let timeline = timeline(10_000, 0, 0);
    assert_eq!(timeline.x_at(0), HANDLE_WIDTH);
    assert_eq!(timeline.x_at(10_000), 422.0 - HANDLE_WIDTH);
    assert_eq!(timeline.time_at(timeline.x_at(5_000)), 5_000);
    // Outside the strip clamps to the ends.
    assert_eq!(timeline.time_at(-50.0), 0);
    assert_eq!(timeline.time_at(1_000.0), 10_000);
}

#[test]
fn presses_take_the_handles_the_head_or_the_window() {
    let mut timeline = timeline(10_000, 0, 0);
    timeline.from = 2_000;
    timeline.till = 8_000;
    let (left, right) = (timeline.x_at(2_000), timeline.x_at(8_000));
    assert_eq!(timeline.grab_at(left - HANDLE_WIDTH), Grab::Left);
    assert_eq!(timeline.grab_at(left + 5.0), Grab::Left);
    assert_eq!(timeline.grab_at(right + HANDLE_WIDTH + 5.0), Grab::Right);
    assert_eq!(timeline.grab_at((left + right) / 2.0), Grab::Head);
    assert_eq!(
        timeline.grab_at(left - HANDLE_WIDTH - HIT_SLOP - 5.0),
        Grab::Window
    );
}

#[test]
fn dragging_a_handle_keeps_the_limits() {
    let mut sticker = timeline(10_000, 3_000, 200);
    // The end handle pulled far right drags the start along.
    let moved = sticker.drag(Grab::Right, sticker.x_at(9_000));
    assert_eq!(moved, Moved::Trim(9_000));
    assert_eq!((sticker.from, sticker.till), (6_000, 9_000));
    // The start handle can't pass the end minus the shortest selection.
    sticker.drag(Grab::Left, sticker.x_at(9_000));
    assert!(sticker.till - sticker.from >= sticker.min_selection());
    assert!(sticker.cover >= sticker.from && sticker.cover <= sticker.till);
}

#[test]
fn the_head_moves_the_cover_inside_the_selection() {
    let mut timeline = timeline(10_000, 0, 0);
    timeline.from = 2_000;
    timeline.till = 6_000;
    assert_eq!(
        timeline.drag(Grab::Head, timeline.x_at(9_000)),
        Moved::Cover(6_000)
    );
    assert_eq!(
        timeline.drag(Grab::Head, timeline.x_at(9_000)),
        Moved::Nothing
    );
}

#[test]
fn a_press_outside_moves_the_whole_window() {
    let mut timeline = timeline(10_000, 0, 0);
    timeline.from = 0;
    timeline.till = 2_000;
    assert_eq!(
        timeline.drag(Grab::Window, timeline.x_at(9_500)),
        Moved::Trim(8_000)
    );
    assert_eq!((timeline.from, timeline.till), (8_000, 10_000));
}

#[test]
fn the_shortest_selection_keeps_the_head_reachable() {
    let timeline = timeline(600_000, 0, 0);
    // 15 px of a 400 px strip of ten minutes.
    assert_eq!(timeline.min_selection(), 22_500);
}

#[test]
fn the_label_reads_the_range_or_the_length() {
    let mut timeline = timeline(10_000, 0, 0);
    assert_eq!(timeline.duration_label(), "00:10.0");
    timeline.from = 1_200;
    timeline.till = 5_000;
    assert_eq!(timeline.duration_label(), "00:01.2 – 00:05.0");
}

#[test]
fn the_strip_has_frames_of_the_clip_aspect() {
    let timeline = timeline(10_000, 0, 0);
    // 400 px of 96 px wide frames.
    let times = timeline.frame_times(1920, 1080);
    assert_eq!(times.len(), 5);
    assert_eq!(times[0], 1_000);
    // Very tall clips stop at the most frames.
    assert_eq!(timeline.frame_times(1, 1000).len(), MAX_FRAMES);
}

#[test]
fn quality_dots_spread_between_the_side_skips() {
    assert_eq!(quality_dot_x(0, 4, 236.0), QUALITY_SIDE_SKIP);
    assert_eq!(quality_dot_x(3, 4, 236.0), 236.0 - QUALITY_SIDE_SKIP);
    assert_eq!(quality_index_at(0.0, 4, 236.0), 0);
    assert_eq!(quality_index_at(90.0, 4, 236.0), 1);
    assert_eq!(quality_index_at(500.0, 4, 236.0), 3);
}

#[test]
fn crop_edges_and_moves_follow_the_pointer() {
    let area = CropArea::FULL;
    let shown = (400.0, 200.0);
    let corner = crop_grab_at(area, (2.0, 3.0), shown).unwrap();
    assert!(corner.left && corner.top && !corner.moves());
    let inside = crop_grab_at(area, (200.0, 100.0), shown).unwrap();
    assert!(inside.moves());
    assert!(crop_grab_at(area, (450.0, 100.0), shown).is_none());

    let cut = crop_dragged(area, corner, (100.0, 50.0), shown, None);
    assert_eq!(
        cut,
        CropArea {
            x: 0.25,
            y: 0.25,
            w: 0.75,
            h: 0.75
        }
    );
    // A move stays inside the frame.
    let moved = crop_dragged(cut, inside, (-400.0, 0.0), shown, None);
    assert_eq!(moved.x, 0.0);
    assert_eq!(moved.w, 0.75);
}

#[test]
fn a_square_crop_stays_square_in_pixels() {
    // A 2:1 frame: a pixel square is half as wide as it is high in parts.
    let start = CropArea {
        x: 0.25,
        y: 0.0,
        w: 0.5,
        h: 1.0,
    };
    let right = CropGrab {
        left: false,
        top: false,
        right: true,
        bottom: false,
    };
    let cut = crop_dragged(start, right, (-100.0, 0.0), (400.0, 200.0), Some(2.0));
    assert!((cut.w * 2.0 - cut.h).abs() < 1e-4);
    assert!(cut.h < 1.0);
}

#[test]
fn crop_areas_round_trip_through_thousandths() {
    assert_eq!(CropArea::FULL.to_crop(), None);
    let crop = Crop {
        x: 100,
        y: 200,
        w: 500,
        h: 400,
    };
    assert_eq!(CropArea::of(Some(crop)).to_crop(), Some(crop));
}

fn source() -> VideoSource {
    VideoSource {
        duration_ms: 10_000,
        width: 1920,
        height: 1080,
        has_audio: true,
        file_size: 12_000_000,
        video_bitrate: 9_000_000,
        fps_milli: 30_000,
    }
}

#[test]
fn an_untouched_file_keeps_its_size() {
    assert_eq!(
        estimated_size(&VideoEdits::full(10_000), &source()),
        Some((12_000_000, false))
    );
}

#[test]
fn an_edited_file_is_estimated_from_its_bitrate() {
    let half = VideoEdits {
        till_ms: 5_000,
        ..VideoEdits::full(10_000)
    };
    let (bytes, estimate) = estimated_size(&half, &source()).unwrap();
    assert!(estimate);
    // 1080p at 30 fps wants 4.35 Mbit/s, plus 128 kbit/s of sound, for
    // five seconds.
    assert!((2_790_000..2_820_000).contains(&bytes), "{bytes}");
    let small = VideoEdits {
        quality: 360,
        gif: true,
        ..half
    };
    let (bytes, _) = estimated_size(&small, &source()).unwrap();
    assert!(bytes < 500_000, "{bytes}");
}
