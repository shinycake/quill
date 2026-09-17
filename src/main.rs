mod ui;

use gpui_kit::*;
use std::path::PathBuf;
use std::time::Duration;
use ui::ScreenshotDemo;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(demo) = parse_screenshot_demo(&args) {
        run_screenshot_demo(demo);
        return;
    }

    let credentials = quill::credentials::load();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            ui::bind_keys(cx);
            cx.spawn(async move |cx| {
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: point(px(20.), px(20.)),
                            size: size(px(1200.), px(740.)),
                        })),
                        app_id: Some("org.shinycake.quill".into()),
                        titlebar: Some(TitlebarOptions {
                            title: Some("Quill".into()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    move |window, cx| {
                        let view = cx.new(|cx| ui::QuillApp::new(window, cx, credentials.clone()));
                        cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
                    },
                )
                .expect("failed to open window");
            })
            .detach();
        });
}

fn parse_screenshot_demo(args: &[String]) -> Option<(ScreenshotDemo, PathBuf)> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--screenshot-demo" {
            let kind = iter.next().map(String::as_str)?;
            let out = iter
                .next()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("docs/screenshots"));
            let demo = match kind {
                "need-tdjson" => ScreenshotDemo::NeedTdjson,
                "wait-phone" => ScreenshotDemo::WaitPhone,
                _ => {
                    eprintln!("unknown screenshot demo '{kind}' (expected need-tdjson|wait-phone)");
                    std::process::exit(2);
                }
            };
            return Some((demo, out));
        }
    }
    None
}

/// Open a real GPUI window in the requested demo state, linger so an external
/// capture (ffmpeg x11grab) can snap docs/screenshots/*.png, then quit.
fn run_screenshot_demo(demo: (ScreenshotDemo, PathBuf)) {
    let (kind, out_dir) = demo;
    let _ = std::fs::create_dir_all(&out_dir);
    let marker = out_dir.join(match kind {
        ScreenshotDemo::NeedTdjson => ".quill-ready-need-tdjson",
        ScreenshotDemo::WaitPhone => ".quill-ready-wait-phone",
    });
    let _ = std::fs::remove_file(&marker);
    let marker_for_spawn = marker.clone();

    eprintln!("quill screenshot-demo: {kind:?} → {}", out_dir.display());

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            ui::bind_keys(cx);
            cx.spawn(async move |cx| {
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds {
                            origin: point(px(20.), px(20.)),
                            size: size(px(1200.), px(740.)),
                        })),
                        app_id: Some("org.shinycake.quill".into()),
                        titlebar: Some(TitlebarOptions {
                            title: Some("Quill".into()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    move |window, cx| {
                        let view =
                            cx.new(|cx| ui::QuillApp::new_with_demo(window, cx, None, Some(kind)));
                        cx.new(|cx| gpui_kit::component::Root::new(view, window, cx))
                    },
                )
                .expect("failed to open screenshot demo window");

                // Allow a couple of frames to paint, then signal the capture script.
                cx.background_executor()
                    .timer(Duration::from_millis(1500))
                    .await;
                let _ = std::fs::write(&marker_for_spawn, b"ready\n");
                cx.background_executor()
                    .timer(Duration::from_millis(3500))
                    .await;
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
}
