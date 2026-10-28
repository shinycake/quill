use super::*;
use std::io::Write;
use std::path::PathBuf;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/screenshots/fixtures")
}

#[test]
fn stickers_animate_with_transparent_backgrounds() {
    let cancelled = AtomicBool::new(false);
    let path = fixtures().join("demo-sticker.tgs");
    let tgs = decode_tgs(&path, &cancelled).unwrap();
    assert!((60..=61).contains(&tgs.frames.len()));
    assert_eq!(tgs.fps, 30.0);
    assert_ne!(tgs.frames[0], tgs.frames[15]);
    assert_eq!(tgs.frames[0][3], 0);
    assert!(tgs.frames[0].as_chunks::<4>().0.iter().any(|p| p[3] == 255));
    cancelled.store(true, Ordering::SeqCst);
    assert!(decode_tgs(&path, &cancelled).is_err());
}

#[test]
#[ignore = "requires ffmpeg; run scripts/sticker-fixtures.py first"]
fn video_stickers_animate_with_transparent_backgrounds() {
    let cancelled = AtomicBool::new(false);
    let cache = std::env::temp_dir().join(format!("quill-webm-sticker-{}", std::process::id()));
    let child = std::sync::Arc::new(Mutex::new(None));
    let webm = decode_webm(
        &fixtures().join("demo-sticker.webm"),
        &cache,
        &child,
        &cancelled,
    )
    .unwrap();
    assert!(webm.frames.len() > 1 && webm.frames.len() <= MAX_STICKER_FRAMES);
    assert_ne!(webm.frames[0], webm.frames[10]);
    assert_eq!(webm.frames[0][3], 0);
    assert!(child.lock().unwrap().is_none());
    assert!(!cache.exists());
    let mut legacy = serde_json::to_value(crate::settings::MediaPrefs::default()).unwrap();
    legacy
        .as_object_mut()
        .unwrap()
        .remove("loop_animated_stickers");
    assert!(
        serde_json::from_value::<crate::settings::MediaPrefs>(legacy)
            .unwrap()
            .loop_animated_stickers
    );
}

/// A 64 px, one-frame document whose only layer draws `shapes`.
fn one_layer(shapes: &str) -> String {
    format!(
        r#"{{"v":"5.7.0","fr":30,"ip":0,"op":2,"w":64,"h":64,"assets":[],"layers":[{{"ty":4,"ip":0,"op":2,"st":0,"ks":{{"o":{{"a":0,"k":100}},"r":{{"a":0,"k":0}},"p":{{"a":0,"k":[0,0,0]}},"a":{{"a":0,"k":[0,0,0]}},"s":{{"a":0,"k":[100,100,100]}}}},"shapes":[{shapes}]}}]}}"#
    )
}

fn first_frame(json: &str, fitz: FitzType) -> Vec<u8> {
    let path = fixture_text(json);
    let decoded = decode_tgs_sized(&path, 64, 1, f64::INFINITY, fitz, &AtomicBool::new(false));
    let _ = std::fs::remove_file(path);
    decoded.unwrap().frames.remove(0)
}

fn pixel(frame: &[u8], x: usize, y: usize) -> [u8; 4] {
    let at = (y * 64 + x) * 4;
    frame[at..at + 4].try_into().unwrap()
}

const SQUARE: &str =
    r#"{"ty":"rc","p":{"a":0,"k":[32,32]},"s":{"a":0,"k":[64,64]},"r":{"a":0,"k":0}}"#;

#[test]
fn frames_are_straight_alpha_bgra() {
    // Half-transparent orange: premultiplied output would halve the colour.
    let frame = first_frame(
        &one_layer(&format!(
            r#"{SQUARE},{{"ty":"fl","c":{{"a":0,"k":[1,0.5,0,1]}},"o":{{"a":0,"k":50}},"r":1}}"#
        )),
        FitzType::None,
    );
    let [b, g, r, a] = pixel(&frame, 32, 32);
    assert!((127..=129).contains(&a), "alpha {a}");
    assert!(
        r >= 254 && (126..=129).contains(&g) && b <= 1,
        "{b} {g} {r}"
    );
}

#[test]
fn a_skin_tone_recolours_the_rendered_emoji() {
    let json = one_layer(&format!(
        r#"{SQUARE},{{"ty":"fl","c":{{"a":0,"k":[1,0,0,1]}},"o":{{"a":0,"k":100}},"r":1}}"#
    ))
    .replacen(
        r#""assets""#,
        r#""fitz":[{"o":16711680,"f12":65280,"f3":255,"f4":0,"f5":0,"f6":0}],"assets""#,
        1,
    );
    assert_eq!(
        pixel(&first_frame(&json, FitzType::None), 32, 32),
        [0, 0, 255, 255]
    );
    assert_eq!(
        pixel(&first_frame(&json, FitzType::Type12), 32, 32),
        [0, 255, 0, 255]
    );
    assert_eq!(
        pixel(&first_frame(&json, FitzType::Type3), 32, 32),
        [255, 0, 0, 255]
    );
}

#[test]
fn animated_paths_are_all_drawn() {
    // Sixteen animated four-point bars: the previous renderer dropped the
    // front bars of files like this, as it dropped stickers' faces and hands
    // (docs/decisions/thorvg-sticker-renderer.md).
    let bar = |x: usize| {
        let shape = format!(
            r#"{{"i":[[0,0],[0,0],[0,0],[0,0]],"o":[[0,0],[0,0],[0,0],[0,0]],"v":[[{x},0],[{},0],[{},64],[{x},64]],"c":true}}"#,
            x + 3,
            x + 3
        );
        format!(
            r#"{{"ty":"sh","ks":{{"a":1,"k":[{{"t":0,"i":{{"x":0.5,"y":0.5}},"o":{{"x":0.5,"y":0.5}},"s":[{shape}],"e":[{shape}]}},{{"t":1,"s":[{shape}]}}]}}}}"#
        )
    };
    let bars: Vec<String> = (0..16).map(|i| bar(i * 4)).collect();
    let frame = first_frame(
        &one_layer(&format!(
            r#"{},{{"ty":"fl","c":{{"a":0,"k":[1,1,1,1]}},"o":{{"a":0,"k":100}},"r":1}}"#,
            bars.join(",")
        )),
        FitzType::None,
    );
    let painted: Vec<usize> = (0..16)
        .filter(|bar| pixel(&frame, bar * 4 + 1, 32)[3] == 255)
        .collect();
    assert_eq!(painted, (0..16).collect::<Vec<_>>());
}

#[test]
fn playback_samples_at_most_the_rate_cap() {
    // A 3 s, 60 fps sticker: 90 frames at 30 fps.
    assert_eq!(sampled_frame_count(180, 60.0, 120, 30.0), 90);
    // A 2 s, 60 fps sticker: 60 frames (was all 120).
    assert_eq!(sampled_frame_count(120, 60.0, 120, 30.0), 60);
    // Already at or under the cap: every frame, up to the frame cap.
    assert_eq!(sampled_frame_count(60, 30.0, 120, 30.0), 60);
    assert_eq!(sampled_frame_count(200, 24.0, 120, 30.0), 120);
    // Custom emoji: the frame cap binds first.
    assert_eq!(sampled_frame_count(180, 60.0, 36, 30.0), 36);
    // No cap.
    assert_eq!(sampled_frame_count(120, 60.0, 120, f64::INFINITY), 120);
    assert_eq!(sampled_frame_count(0, 60.0, 120, 30.0), 1);
}

fn fixture(value: &serde_json::Value) -> PathBuf {
    fixture_text(&value.to_string())
}

fn fixture_text(json: &str) -> PathBuf {
    use std::sync::atomic::AtomicUsize;
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "quill-sticker-{}-{}.tgs",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let mut gzip = flate2::write::GzEncoder::new(
        std::fs::File::create(&path).unwrap(),
        flate2::Compression::default(),
    );
    gzip.write_all(json.as_bytes()).unwrap();
    gzip.finish().unwrap();
    path
}

#[test]
fn tgs_validation_rejects_external_resources_and_bad_bounds() {
    let base = serde_json::json!({"w":512,"h":512,"fr":60,"ip":0,"op":120,"assets":[],"layers":[]});
    let path = fixture(&base);
    assert!(tgs_json(&path, FitzType::None).is_ok());
    for patch in [
        serde_json::json!({"assets":[{"p":"/etc/passwd"}]}),
        serde_json::json!({"w":0}),
        serde_json::json!({"fr":0}),
        serde_json::json!({"op":7201}),
    ] {
        let mut value = base.clone();
        for (key, replacement) in patch.as_object().unwrap() {
            value[key] = replacement.clone();
        }
        let patched = fixture(&value);
        assert!(tgs_json(&patched, FitzType::None).is_err());
        let _ = std::fs::remove_file(patched);
    }
    std::fs::write(&path, b"bad gzip").unwrap();
    assert!(tgs_json(&path, FitzType::None).is_err());
    let _ = std::fs::remove_file(path);
}
