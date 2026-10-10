use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn ffmpeg_can_extract(src: &Path) -> bool {
    Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(src)
        .args(["-frames:v", "1", "-f", "null", "-"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[test]
fn non_video_is_rejected() {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("quill-video-still-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    let png = dir.join("still.png");
    std::fs::write(&png, b"not-a-real-png").unwrap();
    let err = playback_frames(&png, "image/png", &dir.join("cache"), 0).unwrap_err();
    assert!(err.contains("unsupported"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn video_frame_cache_passes_display_sandbox_random_temp_does_not() {
    use crate::local_path::sandboxed_display_path;
    // Creates `{temp}/quill-media-cache/{account}/video-frames`.
    let _guard = crate::local_path::lock_shared_media_cache();

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let account = std::env::temp_dir().join(format!("quill-video-account-{nanos}"));
    std::fs::create_dir_all(&account).unwrap();
    let roots = with_video_frame_cache(vec![account.clone()]);
    let frame = video_frame_cache_dir(44).join("frame-01.png");
    std::fs::create_dir_all(frame.parent().unwrap()).unwrap();
    std::fs::write(&frame, b"png").unwrap();
    let shown = sandboxed_display_path(frame.to_str().unwrap(), &roots);
    assert_eq!(
        shown.as_deref(),
        Some(std::fs::canonicalize(&frame).unwrap().as_path())
    );

    let stray = std::env::temp_dir().join(format!("quill-video-stray-{nanos}.png"));
    std::fs::write(&stray, b"no").unwrap();
    assert!(sandboxed_display_path(stray.to_str().unwrap(), &roots).is_none());

    let gif_layout = std::env::temp_dir().join(format!("quill-gif-frames-{nanos}"));
    std::fs::create_dir_all(&gif_layout).unwrap();
    let gif_frame = gif_layout.join("frame-01.png");
    std::fs::write(&gif_frame, b"gif").unwrap();
    assert!(sandboxed_display_path(gif_frame.to_str().unwrap(), &roots).is_none());

    discard_frame_cache(44);
    let _ = std::fs::remove_dir_all(&account);
    let _ = std::fs::remove_file(&stray);
    let _ = std::fs::remove_dir_all(&gif_layout);
}

#[test]
fn format_duration_is_used_when_stream_duration_is_na() {
    let missing = serde_json::json!({
        "streams": [{ "width": 640, "height": 360 }],
        "format": { "duration": "2.400000" }
    });
    let probe = probe_from_ffprobe_json(&missing, false).unwrap();
    assert_eq!(probe.duration, 2);
    assert_eq!(probe.width, 640);
    assert_eq!(probe.height, 360);
    assert!(!probe.supports_streaming);

    let na = serde_json::json!({
        "streams": [{ "width": 640, "height": 360, "duration": "N/A" }],
        "format": { "duration": "3.2" }
    });
    assert_eq!(probe_from_ffprobe_json(&na, false).unwrap().duration, 3);

    let neither = serde_json::json!({
        "streams": [{ "width": 640, "height": 360, "duration": "N/A" }],
        "format": { "duration": "N/A" }
    });
    assert!(probe_from_ffprobe_json(&neither, false).is_none());
}

#[test]
fn demo_clip_probe_matches_ffprobe_layout() {
    let clip =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots/fixtures/demo-clip.mp4");
    let probe = probe_local_video(&clip).expect("demo clip");
    assert_eq!(probe.duration, 1);
    assert_eq!(probe.width, 320);
    assert_eq!(probe.height, 180);
    assert!(probe.supports_streaming);
    let notes =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots/fixtures/demo-notes.txt");
    assert!(probe_local_video(&notes).is_err());
}

#[test]
fn square_note_probe_accepts_fixture_and_rejects_landscape() {
    let note = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs/screenshots/fixtures/demo-video-note.mp4");
    let probe = probe_local_video_note(&note).expect("square note");
    assert_eq!(probe.duration, 1);
    assert_eq!(probe.length, 240);
    let clip =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots/fixtures/demo-clip.mp4");
    assert!(probe_local_video_note(&clip).is_err());
}

#[test]
fn viewer_fps_adapts_to_duration_and_frame_budget() {
    assert_eq!(viewer_fps_for_duration(0), 8.0);
    assert_eq!(viewer_fps_for_duration(75), 8.0);
    assert_eq!(viewer_fps_for_duration(76), 600.0 / 76.0);
    assert_eq!(viewer_fps_for_duration(300), 2.0);
    assert_eq!(viewer_fps_for_duration(600), 1.0);
    assert_eq!(viewer_fps_for_duration(601), 1.0);
}

#[test]
fn viewer_frames_cover_full_clip_with_adaptive_fps() {
    let clip = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs/screenshots/fixtures/demo-clip-12s.mp4");
    if !ffmpeg_can_extract(&clip) {
        eprintln!("skipping ffmpeg smoke test: ffmpeg cannot extract the demo clip");
        return;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let cache = std::env::temp_dir().join(format!("quill-viewer-test-{nanos}"));
    // 12 s at 8 fps = 96 frames, under the 600-frame cap.
    let vf = viewer_playback_frames(&clip, "video/mp4", &cache, 0, 12).expect("frames");
    assert_eq!(vf.fps, 8.0);
    assert_eq!(vf.frames.len(), 96);
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn cancelable_extraction_cancel_before_publish_aborts_without_orphan() {
    // The UI can abandon the run before ffmpeg publishes its child
    // (viewer closed between extraction start and spawn): the worker
    // must report cancellation and must not leave a running ffmpeg
    // behind.
    let clip = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("docs/screenshots/fixtures/demo-clip-12s.mp4");
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let cache = std::env::temp_dir().join(format!("quill-viewer-prepub-test-{nanos}"));
    let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
    let cancel = std::sync::atomic::AtomicBool::new(true);
    let err = viewer_playback_frames_cancelable(&clip, "video/mp4", &cache, 0, 12, &slot, &cancel)
        .unwrap_err();
    assert!(
        err.contains("cancelled"),
        "expected a cancellation error, got: {err}"
    );
    assert!(
        slot.lock().map(|guard| guard.is_none()).unwrap_or(true),
        "no orphaned ffmpeg child may remain in the slot"
    );
    let _ = std::fs::remove_dir_all(&cache);
}

#[test]
fn viewer_frame_cache_is_separate_from_row_preview_cache() {
    assert_ne!(video_frame_cache_dir(96), viewer_frame_cache_dir(96));
    assert!(viewer_frame_cache_dir(96).starts_with(viewer_frame_cache_root()));
}

#[test]
fn cancelable_extraction_kill_aborts_and_reports_cancelled() {
    let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
    let cancel = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        let handle = scope.spawn(|| {
            let mut command = Command::new("sleep");
            command.arg("30");
            wait_for_cancelable_child(&mut command, &slot, &cancel)
        });
        // Wait for the child to be published, then kill it the way the UI
        // does on viewer close/step.
        let mut published = false;
        for _ in 0..200 {
            if slot.lock().map(|guard| guard.is_some()).unwrap_or(false) {
                published = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(published, "child was published to the slot");
        let mut child = slot
            .lock()
            .ok()
            .and_then(|mut guard| guard.take())
            .expect("child present");
        child.kill().expect("kill extraction");
        child.wait().expect("reap extraction");
        let err = handle.join().expect("worker thread").unwrap_err();
        assert!(
            err.contains("cancelled"),
            "expected a cancellation error, got: {err}"
        );
    });
}

#[test]
fn sweep_removes_stale_viewer_frame_caches() {
    // Hold the shared cache lock through the absence asserts. Other
    // tests recreate the account base as soon as the sweep returns.
    let _guard = crate::local_path::lock_shared_media_cache();
    let dir = viewer_frame_cache_dir(424242);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("frame-001.png"), b"stale").unwrap();
    // Legacy pre-fix world-readable layout must be migrated away too.
    let legacy = crate::local_path::cache_temp_dir().join("quill-gif-frames");
    std::fs::create_dir_all(&legacy).unwrap();
    crate::local_path::sweep_media_caches();
    assert!(
        !viewer_frame_cache_root().exists(),
        "stale viewer frame caches are swept at startup"
    );
    assert!(
        !crate::local_path::media_cache_base().exists(),
        "whole account media cache base is swept"
    );
    assert!(!legacy.exists(), "legacy world-readable cache is removed");
}

#[test]
fn round_video_size_matches_tgx_constants() {
    // MED2: what the HQ setting is ultimately validating — TGX
    // `MAX_HQ_ROUND_RESOLUTION` (480) vs `MAX_ROUND_RESOLUTION` (280).
    assert_eq!(round_video_size(true), 480);
    assert_eq!(round_video_size(false), 280);
}

#[cfg(target_os = "linux")]
#[test]
fn video_note_capture_start_fails_honestly_without_camera() {
    // Environment-gated: only meaningful where no camera exists.
    if !crate::media_tools::v4l2_capture_devices().is_empty() {
        return;
    }
    let err = match VideoNoteCapture::start(false) {
        Ok(_) => panic!("expected no-camera error"),
        Err(err) => err,
    };
    // Without ffmpeg the install hint comes first; with it, the camera.
    let expected = if crate::media_tools::is_installed("ffmpeg") {
        "No camera found"
    } else {
        "need ffmpeg"
    };
    assert!(err.contains(expected), "got: {err}");
}

#[test]
fn round_capture_records_a_square_clip_and_streams_preview_frames() {
    // The capture's filter graph on a synthetic 640×480 camera and a
    // tone: a square clip at the round size with audio, plus whole
    // BGRA preview frames on stdout. Skipped where ffmpeg is missing.
    if Command::new("ffmpeg")
        .arg("-version")
        .output()
        .map(|out| !out.status.success())
        .unwrap_or(true)
    {
        return;
    }
    let dir = std::env::temp_dir().join(format!("quill-round-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let clip = dir.join("round.mp4");
    let input = crate::media_tools::CaptureInput {
        args: [
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=640x480:rate=25:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=duration=1",
        ]
        .map(String::from)
        .to_vec(),
        audio: "1:a",
        video: "0:v",
    };
    let out = Command::new("ffmpeg")
        .args(round_capture_args(&input, 280, &clip))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let frame = (ROUND_PREVIEW_SIDE * ROUND_PREVIEW_SIDE * 4) as usize;
    assert!(out.stdout.len() >= frame * 20, "{} bytes", out.stdout.len());
    assert_eq!(out.stdout.len() % frame, 0);
    let probe = probe_local_video(&clip).unwrap();
    assert_eq!((probe.width, probe.height), (280, 280));
    let rate = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries"])
        .args(["stream=r_frame_rate", "-of", "csv=p=0"])
        .arg(&clip)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&rate.stdout).trim(), "30/1");
    assert_eq!(probe.duration, 1);
    let _ = std::fs::remove_dir_all(&dir);
}
