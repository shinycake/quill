use super::*;

#[test]
fn source_ids_round_trip() {
    assert_eq!(
        Target::parse(&SourceId("x11-screen:1920,0,2560,1440".into())),
        Some(Target::Screen(Rect {
            x: 1920,
            y: 0,
            width: 2560,
            height: 1440,
        }))
    );
    assert_eq!(
        Target::parse(&SourceId("x11-window:4194311".into())),
        Some(Target::Window(4194311))
    );
    for bad in [
        "x11-screen:1,2,3",
        "x11-screen:a,b,c,d",
        "x11-screen:0,0,70000,10",
        "x11-window:-1",
        "portal",
        "",
    ] {
        assert_eq!(Target::parse(&SourceId(bad.into())), None, "{bad}");
    }
}

#[test]
fn a_screen_is_clamped_to_the_root() {
    let root = Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    };
    let inside = Rect {
        x: 100,
        y: 50,
        width: 800,
        height: 600,
    };
    assert_eq!(inside.clamped(&root), Some(inside));
    let over = Rect {
        x: 1600,
        y: -20,
        width: 800,
        height: 600,
    };
    assert_eq!(
        over.clamped(&root),
        Some(Rect {
            x: 1600,
            y: 0,
            width: 320,
            height: 580,
        })
    );
    let outside = Rect {
        x: 1920,
        y: 0,
        width: 100,
        height: 100,
    };
    assert_eq!(outside.clamped(&root), None);
}

#[test]
fn the_cursor_blends_over_the_picture() {
    // A 4 x 2 grey picture; a 2 x 2 cursor at (3, 1): one pixel lands.
    let mut picture = vec![100u8; 4 * 2 * 4];
    let cursor = [
        0xff_ff_00_00u32, // opaque red
        0x80_00_00_80,    // half-transparent blue (premultiplied)
        0x00_00_00_00,
        0x00_00_00_00,
    ];
    blend_cursor(&mut picture, (4, 2), &cursor, (2, 2), (3, 1));
    let at = (4 + 3) * 4;
    assert_eq!(&picture[at..at + 3], &[0, 0, 255], "BGR of opaque red");
    assert!(picture[..at].iter().all(|&b| b == 100));
    // Half-transparent blue at (0, 0).
    let mut picture = vec![100u8; 4 * 2 * 4];
    blend_cursor(&mut picture, (4, 2), &cursor[1..2], (1, 1), (0, 0));
    assert_eq!(&picture[..3], &[128 + 49, 49, 49]);
    // Off the picture on every side: nothing changes, nothing panics.
    let mut picture = vec![7u8; 4 * 2 * 4];
    for at in [(-5, -5), (10, 0), (0, 10), (-1, 1)] {
        blend_cursor(&mut picture, (4, 2), &[0xff_ff_ff_ff; 4], (2, 2), at);
    }
    assert!(
        picture
            .iter()
            .enumerate()
            .all(|(i, &b)| b == 7 || i / 4 == 4)
    );
    // A short cursor image is read as far as it goes.
    blend_cursor(&mut picture, (4, 2), &[0xff_00_00_00], (2, 2), (0, 0));
}

/// Needs an X server without a window manager, e.g. `Xvfb :99` and
/// `DISPLAY=:99`. The test plays the window manager: it sets `WM_STATE`
/// and the window type on its own windows.
#[test]
#[ignore]
fn lists_captures_pauses_and_ends_on_a_real_x_server() {
    use x11rb::protocol::xproto::{ConfigureWindowAux, CreateWindowAux, PropMode, WindowClass};
    use x11rb::wrapper::ConnectionExt as _;
    let (conn, screen) = x11rb::connect(None).expect("an X display");
    let root = conn.setup().roots[screen].root;
    let intern = |name: &[u8]| conn.intern_atom(false, name).unwrap().reply().unwrap().atom;
    let wm_state = intern(b"WM_STATE");
    let window_type = intern(b"_NET_WM_WINDOW_TYPE");
    let normal = intern(b"_NET_WM_WINDOW_TYPE_NORMAL");
    let dock = intern(b"_NET_WM_WINDOW_TYPE_DOCK");
    let make = |title: &str, colour: u32, kind: u32| {
        let window = conn.generate_id().unwrap();
        conn.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            root,
            20,
            20,
            300,
            200,
            0,
            WindowClass::INPUT_OUTPUT,
            x11rb::COPY_FROM_PARENT,
            &CreateWindowAux::new().background_pixel(colour),
        )
        .unwrap();
        conn.change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            title.as_bytes(),
        )
        .unwrap();
        conn.change_property32(
            PropMode::REPLACE,
            window,
            window_type,
            AtomEnum::ATOM,
            &[kind],
        )
        .unwrap();
        conn.change_property32(PropMode::REPLACE, window, wm_state, wm_state, &[1, 0])
            .unwrap();
        conn.map_window(window).unwrap();
        window
    };
    let red = make("Quill test red", 0x00ff_0000, normal);
    let _panel = make("Quill test panel", 0x0000_ff00, dock);
    let blue = make("Quill test blue", 0x0000_00ff, normal);
    conn.configure_window(red, &ConfigureWindowAux::new().x(400))
        .unwrap();
    conn.flush().unwrap();
    std::thread::sleep(Duration::from_millis(200));

    let listed = sources().expect("sources");
    let titles: Vec<&str> = listed.iter().map(|s| s.title.as_str()).collect();
    eprintln!("{titles:?}");
    assert_eq!(listed.first().map(|s| s.kind), Some(SourceKind::Screen));
    let windows: Vec<&ScreenSource> = listed
        .iter()
        .filter(|s| s.kind == SourceKind::Window)
        .collect();
    assert_eq!(
        windows.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(),
        ["Quill test blue", "Quill test red"],
        "top first, no dock"
    );

    let thumb = thumbnail(windows[1], (60, 40)).expect("a picture");
    assert_eq!((thumb.width, thumb.height), (60, 40));
    assert_eq!(
        &thumb.rgba[..4],
        &[255, 0, 0, 255],
        "the covered red window"
    );
    let screen = thumbnail(&listed[0], (64, 64)).expect("the screen");
    assert!(screen.width <= 64 && screen.height <= 64);

    // Capture the red window, minimise it, restore it, close it.
    let sink = FrameSink::new(4);
    let mut capture = start(
        &windows[1].id,
        ScreenOptions::default(),
        sink.clone(),
        Arc::default(),
    )
    .expect("start");
    let frame = sink.pop_timeout(Duration::from_secs(5)).expect("a frame");
    assert_eq!((frame.width, frame.height), (300, 200));
    // Red in BT.601 video range.
    assert_eq!(frame.i420[0], 82);
    conn.change_property32(PropMode::REPLACE, red, wm_state, wm_state, &[3, 0])
        .unwrap();
    conn.flush().unwrap();
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(sink.take_events(), vec![CaptureEvent::Paused]);
    conn.change_property32(PropMode::REPLACE, red, wm_state, wm_state, &[1, 0])
        .unwrap();
    conn.flush().unwrap();
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(sink.take_events(), vec![CaptureEvent::Resumed]);
    conn.destroy_window(red).unwrap();
    conn.flush().unwrap();
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(sink.take_events(), vec![CaptureEvent::Ended]);
    capture.stop();
    conn.destroy_window(blue).unwrap();
    conn.flush().unwrap();
}

/// Cost of one 1920 x 1080 screen grab and its I420 conversion
/// (`--release` for a meaningful number), on a real X server.
#[test]
#[ignore]
fn measures_a_1080p_grab() {
    let x11 = X11::connect().expect("an X display");
    let target = Target::Screen(Rect {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
    });
    let rounds = 30u32;
    let (mut grab_time, mut convert_time) = (Duration::ZERO, Duration::ZERO);
    for _ in 0..rounds {
        let at = Instant::now();
        let grab = x11.grab(&target, true).expect("grab");
        grab_time += at.elapsed();
        let at = Instant::now();
        let frame = convert(&grab, ScreenOptions::default(), at).expect("frame");
        convert_time += at.elapsed();
        assert_eq!(frame.i420.len(), 1920 * 1080 * 3 / 2);
    }
    eprintln!(
        "grab {:?} / frame, convert {:?} / frame",
        grab_time / rounds,
        convert_time / rounds
    );
}
