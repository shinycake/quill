use super::*;

fn source(kind: SourceKind, title: &str) -> ScreenSource {
    ScreenSource {
        id: SourceId(title.to_string()),
        kind,
        title: title.to_string(),
        size: (100, 100),
    }
}

#[test]
fn labels_number_screens_and_untitled_windows_like_tdesktop() {
    let sources = [
        source(SourceKind::Screen, ""),
        source(SourceKind::Screen, ""),
        source(SourceKind::Window, "Notes"),
        source(SourceKind::Window, "  "),
        source(SourceKind::Window, ""),
    ];
    assert_eq!(
        source_labels(&sources),
        vec![
            SourceLabel::Screen(1),
            SourceLabel::Screen(2),
            SourceLabel::Window("Notes".into()),
            SourceLabel::UntitledWindow(1),
            SourceLabel::UntitledWindow(2),
        ]
    );
}

#[test]
fn capture_size_keeps_aspect_never_upscales_and_stays_even() {
    assert_eq!(fit_capture_size(3840, 2160, 1920, 1080), (1920, 1080));
    assert_eq!(fit_capture_size(1280, 720, 1920, 1080), (1280, 720));
    assert_eq!(fit_capture_size(2880, 1800, 1920, 1080), (1728, 1080));
    assert_eq!(fit_capture_size(801, 601, 1920, 1080), (800, 600));
    assert_eq!(fit_capture_size(1, 1, 1920, 1080), (2, 2));
    assert_eq!(fit_capture_size(0, 0, 0, 0), (2, 2));
}

#[test]
fn nv12_splits_the_interleaved_chroma_into_u_and_v() {
    // 4 x 2 picture, Y stride 6 and UV stride 4 (padding is skipped).
    let y = [1, 2, 3, 4, 0, 0, 5, 6, 7, 8, 0, 0];
    let uv = [10, 20, 11, 21];
    let out = nv12_to_i420(&y, 6, &uv, 4, 4, 2).unwrap();
    assert_eq!(out, vec![1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 20, 21]);
    assert_eq!(out.len(), 4 * 2 * 3 / 2);
}

#[test]
fn nv12_rejects_odd_sizes_and_short_planes() {
    let y = [0u8; 16];
    let uv = [0u8; 8];
    assert!(nv12_to_i420(&y, 4, &uv, 4, 3, 2).is_none());
    assert!(nv12_to_i420(&y, 4, &uv, 4, 4, 0).is_none());
    assert!(nv12_to_i420(&y, 2, &uv, 4, 4, 2).is_none());
    assert!(nv12_to_i420(&y[..7], 4, &uv, 4, 4, 2).is_none());
    assert!(nv12_to_i420(&y, 4, &uv[..3], 4, 4, 2).is_none());
}

#[test]
fn preview_is_downscaled_rgba_with_video_range_colors() {
    // 4 x 2, black (Y 16) on the left and white (Y 235) on the right,
    // neutral chroma.
    let mut i420 = vec![16, 16, 235, 235, 16, 16, 235, 235];
    i420.extend_from_slice(&[128, 128, 128, 128]);
    let picture = i420_preview(&i420, 4, 2, 2).unwrap();
    assert_eq!((picture.width, picture.height), (2, 2));
    assert_eq!(picture.rgba.get(0..4), Some(&[0, 0, 0, 255][..]));
    assert_eq!(picture.rgba.get(4..8), Some(&[255, 255, 255, 255][..]));
    assert!(i420_preview(&i420[..10], 4, 2, 2).is_none());
}

#[test]
fn preview_slot_publishes_at_most_four_times_a_second() {
    let slot = PreviewSlot::default();
    let start = Instant::now();
    let frame = |at: Instant| VideoFrame {
        i420: vec![128; 4 * 4 * 3 / 2],
        width: 4,
        height: 4,
        captured_at: at,
    };
    assert!(slot.latest_after(0).is_none());
    slot.offer(&frame(start));
    assert_eq!(slot.latest_after(0).map(|(seq, _)| seq), Some(1));
    slot.offer(&frame(start + Duration::from_millis(100)));
    assert_eq!(slot.latest_after(0).map(|(seq, _)| seq), Some(1));
    slot.offer(&frame(start + Duration::from_millis(300)));
    assert_eq!(slot.latest_after(0).map(|(seq, _)| seq), Some(2));
    assert!(
        slot.latest_after(2).is_none(),
        "the seen preview is not copied again"
    );
}
