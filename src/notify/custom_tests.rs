use super::{
    DELTA_X, DELTA_Y, NativeService, NotifyPlatform, NotifyQueue, Rect, ScreenCorner, WIDTH,
    clamp_count, display_checksum, display_label, ease_in_circ, hide_all_wanted, native_support,
    native_switch_shown, stack_shifts, use_custom, window_origin,
};

const SCREEN: Rect = Rect {
    x: 0.0,
    y: 25.0,
    width: 1440.0,
    height: 875.0,
};

#[test]
fn platforms_pick_the_backend_like_tdesktop() {
    let full = NativeService::Present { full: true };
    let mac = native_support(NotifyPlatform::MacOS, full, false);
    assert!(!use_custom(None, mac, full) && !use_custom(Some(false), mac, full));
    assert!(!native_switch_shown(mac));
    let windows = native_support(NotifyPlatform::Windows, full, false);
    assert!(
        use_custom(None, windows, full),
        "custom by default on Windows"
    );
    assert!(!use_custom(Some(true), windows, full));
    assert!(native_switch_shown(windows));
    let gnome = NativeService::Present { full: false };
    let x11 = native_support(NotifyPlatform::X11, gnome, false);
    assert!(use_custom(None, x11, gnome));
    let plasma = native_support(NotifyPlatform::X11, full, false);
    assert!(!use_custom(None, plasma, full));
    assert!(use_custom(Some(false), plasma, full));
    let missing = native_support(NotifyPlatform::X11, NativeService::Missing, false);
    assert!(use_custom(Some(true), missing, NativeService::Missing));
    assert!(!native_switch_shown(missing));
    let probing = native_support(NotifyPlatform::X11, NativeService::Unknown, false);
    assert!(!use_custom(None, probing, NativeService::Unknown));
    let wayland = native_support(NotifyPlatform::Wayland, gnome, true);
    assert!(!use_custom(Some(false), wayland, gnome), "never on Wayland");
}

#[test]
fn forcing_lifts_the_macos_enforcement() {
    let full = NativeService::Present { full: true };
    let mac = native_support(NotifyPlatform::MacOS, full, true);
    assert!(native_switch_shown(mac));
    assert!(use_custom(Some(false), mac, full));
    assert!(!use_custom(None, mac, full));
}

#[test]
fn windows_stack_from_the_chosen_corner() {
    let heights = [80.0, 125.0, 80.0];
    let (shifts, hide_all) = stack_shifts(&heights);
    assert_eq!(
        shifts,
        vec![3.0 * DELTA_Y + 205.0, 2.0 * DELTA_Y + 80.0, DELTA_Y]
    );
    assert_eq!(hide_all, 4.0 * DELTA_Y + 285.0);
    let right = SCREEN.x + SCREEN.width - WIDTH - DELTA_X;
    assert_eq!(
        window_origin(SCREEN, ScreenCorner::BottomRight, DELTA_Y, 80.0),
        (right, 25.0 + 875.0 - DELTA_Y - 80.0)
    );
    assert_eq!(
        window_origin(SCREEN, ScreenCorner::TopLeft, DELTA_Y, 80.0),
        (DELTA_X, 25.0 + DELTA_Y)
    );
    assert_eq!(
        window_origin(SCREEN, ScreenCorner::TopRight, 40.0, 80.0),
        (right, 65.0)
    );
}

#[test]
fn hide_all_and_count_rules() {
    assert!(!hide_all_wanted(1, 0));
    assert!(hide_all_wanted(2, 0));
    assert!(hide_all_wanted(1, 1));
    assert_eq!(clamp_count(0), 1);
    assert_eq!(clamp_count(3), 3);
    assert_eq!(clamp_count(9), 5);
}

#[test]
fn the_queue_releases_oldest_first_and_drops_a_read_chat() {
    let mut queue = NotifyQueue::default();
    queue.push(1, "a");
    queue.push(2, "b");
    queue.push(1, "c");
    queue.push(3, "d");
    queue.remove_key(&1);
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.take(1), vec![(2, "b")]);
    assert_eq!(queue.take(5), vec![(3, "d")]);
    assert!(queue.is_empty());
    assert!(queue.take(2).is_empty());
}

#[test]
fn helpers() {
    assert_eq!(ease_in_circ(0.0), 0.0);
    assert!((ease_in_circ(1.0) - 1.0).abs() < f32::EPSILON);
    assert!(ease_in_circ(0.5) < 0.5);
    assert_ne!(display_checksum(""), 0);
    assert_ne!(display_checksum("a"), display_checksum("b"));
    assert_eq!(display_label(1920.0, 1080.0), "Display (1920\u{00D7}1080)");
}
