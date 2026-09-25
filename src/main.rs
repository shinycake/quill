#[cfg(feature = "ui")]
mod ui;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().skip(1).any(|a| a == "--connect-smoke") {
        std::process::exit(quill::connect_smoke::cli_exit_code());
    }

    #[cfg(feature = "ui")]
    {
        ui_main(&args);
    }

    #[cfg(not(feature = "ui"))]
    {
        eprintln!("quill: UI not compiled. Pass --connect-smoke, or rebuild with --features ui.");
        std::process::exit(2);
    }
}

#[cfg(feature = "ui")]
fn ui_main(args: &[String]) {
    use gpui_kit::*;

    if let Some(demo) = parse_screenshot_demo(args) {
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

#[cfg(feature = "ui")]
fn parse_screenshot_demo(args: &[String]) -> Option<(ui::ScreenshotDemo, std::path::PathBuf)> {
    use std::path::PathBuf;
    use ui::ScreenshotDemo;

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
                "wait-code" => ScreenshotDemo::WaitCode,
                "wait-password" => ScreenshotDemo::WaitPassword,
                "ready-chats" => ScreenshotDemo::ReadyChats,
                "ready-chats-composer" => ScreenshotDemo::ReadyChatsComposer,
                "ready-unread" => ScreenshotDemo::ReadyUnread,
                "ready-unread-read" => ScreenshotDemo::ReadyUnreadRead,
                "ready-media" => ScreenshotDemo::ReadyMedia,
                "ready-send-media" => ScreenshotDemo::ReadySendMedia,
                "ready-search" => ScreenshotDemo::ReadySearch,
                "ready-search-in-chat" => ScreenshotDemo::ReadySearchInChat,
                "ready-reply" => ScreenshotDemo::ReadyReply,
                "ready-edit-delete" => ScreenshotDemo::ReadyEditDelete,
                "ready-forward" => ScreenshotDemo::ReadyForward,
                "ready-reactions" => ScreenshotDemo::ReadyReactions,
                "ready-pin" => ScreenshotDemo::ReadyPin,
                "ready-mute-archive" => ScreenshotDemo::ReadyMuteArchive,
                "ready-typing" => ScreenshotDemo::ReadyTyping,
                _ => {
                    eprintln!(
                        "unknown screenshot demo '{kind}' (expected need-tdjson|wait-phone|wait-code|wait-password|ready-chats|ready-chats-composer|ready-unread|ready-unread-read|ready-media|ready-send-media|ready-search|ready-search-in-chat|ready-reply|ready-edit-delete|ready-forward|ready-reactions|ready-pin|ready-mute-archive|ready-typing)"
                    );
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
#[cfg(feature = "ui")]
fn run_screenshot_demo(demo: (ui::ScreenshotDemo, std::path::PathBuf)) {
    use gpui_kit::*;
    use std::time::Duration;
    use ui::ScreenshotDemo;

    let (kind, out_dir) = demo;
    let _ = std::fs::create_dir_all(&out_dir);
    let marker = out_dir.join(match kind {
        ScreenshotDemo::NeedTdjson => ".quill-ready-need-tdjson",
        ScreenshotDemo::WaitPhone => ".quill-ready-wait-phone",
        ScreenshotDemo::WaitCode => ".quill-ready-wait-code",
        ScreenshotDemo::WaitPassword => ".quill-ready-wait-password",
        ScreenshotDemo::ReadyChats => ".quill-ready-ready-chats",
        ScreenshotDemo::ReadyChatsComposer => ".quill-ready-ready-chats-composer",
        ScreenshotDemo::ReadyUnread => ".quill-ready-ready-unread",
        ScreenshotDemo::ReadyUnreadRead => ".quill-ready-ready-unread-read",
        ScreenshotDemo::ReadyMedia => ".quill-ready-ready-media",
        ScreenshotDemo::ReadySendMedia => ".quill-ready-ready-send-media",
        ScreenshotDemo::ReadySearch => ".quill-ready-ready-search",
        ScreenshotDemo::ReadySearchInChat => ".quill-ready-ready-search-in-chat",
        ScreenshotDemo::ReadyReply => ".quill-ready-ready-reply",
        ScreenshotDemo::ReadyEditDelete => ".quill-ready-ready-edit-delete",
        ScreenshotDemo::ReadyForward => ".quill-ready-ready-forward",
        ScreenshotDemo::ReadyReactions => ".quill-ready-ready-reactions",
        ScreenshotDemo::ReadyPin => ".quill-ready-ready-pin",
        ScreenshotDemo::ReadyMuteArchive => ".quill-ready-ready-mute-archive",
        ScreenshotDemo::ReadyTyping => ".quill-ready-ready-typing",
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
