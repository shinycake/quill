//! Boots real widgets from widget-sandbox's compatibility corpus through
//! the sandbox, the renderer and Quill's painter, on the test thread: no
//! timers, it waits for the runtime's own "rendered" (or "dead") report.
use super::backend::Resources;
use super::ffi::{Native, Shared};
use super::thread::{Canvas, paint_frame};
use serde_json::{Value, json};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

const BACKGROUND: [u8; 3] = [255, 255, 255];

fn corpus(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("third_party/widgets/sandbox/corpus/compat")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

/// The widget's first whole frame, straight RGBA at 2× of 320 CSS px.
fn first_frame(name: &str) -> (u32, u32, Vec<u8>) {
    let (tx, rx) = channel::<String>();
    let resources = Arc::new(Mutex::new(Resources::default()));
    let shared = Shared {
        events: Box::new(move |json| {
            let _ = tx.send(json);
        }),
        resources: resources.clone(),
    };
    let options = json!({
        "width": 320, "dpr": 2, "maxHeight": 600, "visible": true, "locale": "en",
        "theme": {"colorScheme": "light", "params": {"bg_color": "#ffffff", "text_color": "#000000",
                  "button_color": "#3390ec", "button_text_color": "#ffffff", "hint_color": "#707579"}},
        "context": {"isCopy": false, "canQuery": false, "backgroundQuery": false, "chatType": "private"},
    });
    let mut native = Native::create(&corpus(name), &options.to_string(), shared).unwrap();
    loop {
        let event: Value = serde_json::from_str(&rx.recv().unwrap()).unwrap();
        if event["type"] == "change" {
            assert!(event["dead"].is_null(), "{name} stopped: {}", event["dead"]);
            if event["rendered"] == true {
                break;
            }
        }
    }
    let mut canvas = Canvas::new(BACKGROUND);
    let frame = paint_frame(&mut native, &resources, &mut canvas).expect("a frame");
    let pixmap = canvas.pixmap.take().unwrap();
    assert_eq!(pixmap.width() as f32, frame.width * 2.0);
    (pixmap.width(), pixmap.height(), pixmap.data().to_vec())
}

/// Pixels that are not the bubble.
fn drawn(rgba: &[u8]) -> usize {
    rgba.as_chunks::<4>().0.iter()
        .filter(|px| px[..3] != BACKGROUND)
        .count()
}

#[test]
fn tabs_boot_and_draw_text_and_boxes() {
    let (width, height, rgba) = first_frame("tabs.html");
    assert_eq!(width, 640);
    assert!(height >= 100, "height {height}");
    let painted = drawn(&rgba);
    assert!(painted > 2_000, "only {painted} pixels drawn");
}

#[test]
fn a_canvas_chart_draws_its_bars() {
    let (_, _, rgba) = first_frame("canvas-bar-chart.html");
    // The bars are a saturated color; the bubble and text are not.
    let colored = rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|px| {
            let (max, min) = (px[..3].iter().max().copied().unwrap_or(0), px[..3].iter().min().copied().unwrap_or(0));
            max.saturating_sub(min) > 80
        })
        .count();
    assert!(colored > 1_000, "only {colored} colored pixels");
}
