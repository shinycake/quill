//! Opening the Picture-in-Picture window where tdesktop would
//! (`PipPanel::setPosition`): where the last one was left, on whichever
//! screen that was, else the default spot on the main window's screen;
//! and saving the position each time a move or resize settles.

use super::super::super::VideoPlayer;
use super::{PipOpen, PipWindow, VideoPip, place};
use gpui_kit::component::Root;
use gpui_kit::*;
use quill::media_viewer::pip::panel::{
    MINIMAL_SIZE, PipPosition, PipSize, aspect, place_on_screen,
};
use quill::settings::SavedPipPosition;
use std::rc::Rc;

/// Open the window on `open.player`. `None` when no window could be made
/// (the player is dropped then).
pub(crate) fn open_pip_window(open: PipOpen, cx: &mut App) -> Option<PipWindow> {
    let PipOpen {
        player,
        dimensions,
        hooks,
        last_volume,
        display,
        live,
    } = open;
    let displays = cx.displays();
    let main = display
        .and_then(|id| displays.iter().find(|d| d.id() == id).cloned())
        .or_else(|| cx.primary_display())?;
    let ratio = picture_ratio(&player, dimensions);
    let saved = load_position().and_then(|saved| {
        let screen = displays
            .iter()
            .find(|display| saved_on(&saved, display.as_ref()))?
            .clone();
        Some((screen, saved.position))
    });
    let (screen, position) = saved.unwrap_or_else(|| {
        let full = place::to_rect(main.bounds());
        (main, PipPosition::default_on(full))
    });
    let available = place::to_rect(screen.visible_bounds());
    let placement = place_on_screen(&position, available, ratio);
    let slot = Rc::new(std::cell::RefCell::new(None));
    let slot_in = slot.clone();
    let window = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(place::to_bounds(placement.geometry))),
                // Borderless (`Qt::FramelessWindowHint`).
                titlebar: None,
                // tdesktop's panel never takes focus from the window the
                // user works in (`Qt::WindowDoesNotAcceptFocus`).
                focus: false,
                show: true,
                // An `NSPanel` at the floating level on macOS. Elsewhere a
                // plain window: GPUI makes a floating one transient for
                // the focused window, which tdesktop undoes
                // (`ClearTransientParent`); it is kept on top and off the
                // taskbar by `place::configure`.
                kind: if cfg!(target_os = "macos") {
                    WindowKind::Floating
                } else {
                    WindowKind::Normal
                },
                // Moves and resizes are the window's own (`PipPanel`).
                is_movable: false,
                // Wayland and X11 window-manager resizes go through the
                // system, which needs it.
                is_resizable: cfg!(target_os = "linux"),
                is_minimizable: false,
                display_id: Some(screen.id()),
                // The smallest minimum of any aspect (120 on each side):
                // GPUI can't change it later, and the aspect's own minimum
                // is kept by `on_bounds_changed`.
                window_min_size: Some(size(px(MINIMAL_SIZE as f32), px(MINIMAL_SIZE as f32))),
                window_decorations: Some(WindowDecorations::Client),
                // GPUI caps an unfocused window at 30 frames a second;
                // this one is unfocused by design and plays at the
                // clip's own rate.
                inactive_frame_interval: None,
                ..Default::default()
            },
            move |window, cx| {
                place::configure(window);
                let view = cx.new(|cx| {
                    VideoPip::new(
                        player,
                        dimensions,
                        hooks,
                        last_volume,
                        live,
                        ratio,
                        placement,
                        window,
                        cx,
                    )
                });
                *slot_in.borrow_mut() = Some(view.downgrade());
                let close_view = view.downgrade();
                // The system's close (Alt+F4, the window menu) closes it
                // like the button.
                window.on_window_should_close(cx, move |_, cx| {
                    let _ = close_view.update(cx, |pip, cx| pip.release_player(cx));
                    true
                });
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .ok()?;
    let view = slot.borrow_mut().take()?;
    Some(PipWindow { window, view })
}

/// Whether `saved` was on `display`: the same uuid when both have one,
/// else the same bounds (tdesktop compares the screen's geometry).
fn saved_on(saved: &SavedPipPosition, display: &dyn PlatformDisplay) -> bool {
    let uuid = display.uuid().ok().map(|uuid| uuid.to_string());
    match (&saved.screen_id, uuid) {
        (Some(saved), Some(current)) => *saved == current,
        _ => saved.position.screen == place::to_rect(display.bounds()),
    }
}

/// The aspect: the picture's when there is one, else the message's.
pub(super) fn picture_ratio(player: &VideoPlayer, dimensions: Option<(u32, u32)>) -> PipSize {
    let (width, height) = player
        .picture_size()
        .filter(|(w, h)| *w > 0 && *h > 0)
        .or(dimensions)
        .unwrap_or((1, 1));
    aspect(width, height)
}

#[cfg(not(test))]
fn load_position() -> Option<SavedPipPosition> {
    quill::settings::load_video_pip_position()
}

/// Tests never read the user's saved position.
#[cfg(test)]
fn load_position() -> Option<SavedPipPosition> {
    None
}

/// Save off the UI thread (tdesktop `saveSettingsDelayed`). Writes run one
/// at a time and only the newest is written, so an older one finishing
/// late never wins.
#[cfg(not(test))]
pub(super) fn save_position(saved: SavedPipPosition, cx: &mut App) {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};
    static LATEST: AtomicU64 = AtomicU64::new(0);
    static WRITING: Mutex<()> = Mutex::new(());
    let generation = LATEST.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    cx.background_executor()
        .spawn(async move {
            let _writing = quill::locks::lock_or_recover(&WRITING);
            if LATEST.load(Ordering::SeqCst) == generation {
                let _ = quill::settings::save_video_pip_position(&saved);
            }
        })
        .detach();
}

/// Tests never write the user's settings.
#[cfg(test)]
pub(super) fn save_position(_: SavedPipPosition, _: &mut App) {}
