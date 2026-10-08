//! Linux system tray over the StatusNotifierItem D-Bus protocol (`ksni`).
//!
//! `tray-icon` cannot serve a GPUI process on Linux: it creates the tray
//! through libappindicator and GTK menus, which need `gtk::init` and a
//! running GTK main loop on the creating thread (its README says so), and
//! GPUI drives its own Wayland/X11 loop. Telegram Desktop's Linux tray
//! (`platform/linux/tray_linux.cpp`, `StatusNotifierItem` via DBusMenu)
//! speaks the same protocol, so does this module, in pure Rust on the
//! zbus/async-io stack GPUI already links.
//!
//! Registration happens on a short-lived worker thread so a slow or hung
//! session bus can never stall the UI thread; [`TrayState::poll`] (called
//! from the 1s tray sync) picks the result up. When no
//! `StatusNotifierWatcher` exists (stock GNOME without the AppIndicator
//! extension) registration fails, `available()` stays false, and the
//! close/minimize-to-tray switches stay hidden. Registration is retried
//! every [`RETRY_EVERY`], so a desktop that finishes starting later wins.

use crate::tray::{
    TrayAction, notifications_label, render_tray_icon, rgba_to_argb32, sounds_label, tray_tooltip,
};
use ksni::blocking::{Handle, TrayMethods};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, RecvTimeoutError, TryRecvError, channel};
use std::time::{Duration, Instant};

/// Delay between registration attempts while the desktop has no tray host.
pub const RETRY_EVERY: Duration = Duration::from_secs(10);

/// Actions clicked in the tray menu, drained by [`take_actions`] on the UI
/// thread (ksni calls the menu callbacks on its own thread).
static ACTIONS: Mutex<Vec<TrayAction>> = Mutex::new(Vec::new());

fn push_action(action: TrayAction) {
    ACTIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push(action);
}

/// Drain pending tray menu / click actions.
pub fn take_actions() -> Vec<TrayAction> {
    std::mem::take(
        &mut *ACTIONS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()),
    )
}

/// The StatusNotifierItem model ksni publishes over D-Bus.
struct SniTray {
    unread: u32,
    /// (desktop notifications on, notification sounds on) — menu labels.
    toggles: (bool, bool),
}

impl SniTray {
    fn pixmap(&self) -> ksni::Icon {
        let (rgba, width, height) = render_tray_icon(self.unread);
        ksni::Icon {
            width: width as i32,
            height: height as i32,
            data: rgba_to_argb32(&rgba),
        }
    }
}

impl ksni::Tray for SniTray {
    fn id(&self) -> String {
        "org.shinycake.quill".into()
    }

    fn title(&self) -> String {
        "Quill".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![self.pixmap()]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: tray_tooltip(self.unread),
            ..Default::default()
        }
    }

    /// Primary click on the icon: show the window, like tdesktop.
    fn activate(&mut self, _x: i32, _y: i32) {
        push_action(TrayAction::Open);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: "Open Quill".into(),
                activate: Box::new(|_| push_action(TrayAction::Open)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: notifications_label(self.toggles.0).into(),
                activate: Box::new(|_| push_action(TrayAction::ToggleNotifications)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: sounds_label(self.toggles.1).into(),
                activate: Box::new(|_| push_action(TrayAction::ToggleSounds)),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Quit Quill".into(),
                activate: Box::new(|_| push_action(TrayAction::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Flatpak/Snap-style sandboxes cannot own a well-known D-Bus name.
fn sandboxed() -> bool {
    std::path::Path::new("/.flatpak-info").exists()
}

enum Phase {
    /// No attempt in flight; the next one may start at the instant.
    Idle(Instant),
    /// A worker is registering with the watcher.
    Connecting(Receiver<Option<Handle<SniTray>>>),
    /// Registered and showing.
    Ready {
        handle: Handle<SniTray>,
        shown: (u32, (bool, bool)),
    },
}

/// Tray lifecycle owned by the UI thread (see `tray::sync_tray`).
pub struct TrayState {
    phase: Phase,
}

impl TrayState {
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle(Instant::now()),
        }
    }

    /// Whether a tray icon is registered and visible to the desktop.
    pub fn available(&self) -> bool {
        matches!(&self.phase, Phase::Ready { handle, .. } if !handle.is_closed())
    }

    /// First registration at startup: wait up to `wait` for the result so
    /// `--start-minimized` can tell "no tray host" (reveal the window) from
    /// "tray still registering". A healthy or tray-less desktop answers in
    /// milliseconds; a hung bus falls back to the polled `Connecting` phase.
    pub fn poll_startup(&mut self, unread: u32, toggles: (bool, bool), wait: Duration) {
        if !matches!(self.phase, Phase::Idle(_)) {
            return self.poll(unread, toggles);
        }
        let result = start_registration(unread, toggles);
        self.phase = match result.recv_timeout(wait) {
            Ok(Some(handle)) => Phase::Ready {
                handle,
                shown: (unread, toggles),
            },
            Ok(None) | Err(RecvTimeoutError::Disconnected) => {
                Phase::Idle(Instant::now() + RETRY_EVERY)
            }
            Err(RecvTimeoutError::Timeout) => Phase::Connecting(result),
        };
    }

    /// Advance the lifecycle and publish `unread`.
    pub fn poll(&mut self, unread: u32, toggles: (bool, bool)) {
        match &mut self.phase {
            Phase::Idle(not_before) => {
                if Instant::now() >= *not_before {
                    self.phase = Phase::Connecting(start_registration(unread, toggles));
                }
            }
            Phase::Connecting(result) => match result.try_recv() {
                Ok(Some(handle)) => {
                    self.phase = Phase::Ready {
                        handle,
                        shown: (unread, toggles),
                    }
                }
                Ok(None) | Err(TryRecvError::Disconnected) => {
                    self.phase = Phase::Idle(Instant::now() + RETRY_EVERY);
                }
                Err(TryRecvError::Empty) => {}
            },
            Phase::Ready { handle, shown } => {
                if handle.is_closed() {
                    // The watcher went away (shell restart): register again.
                    self.phase = Phase::Idle(Instant::now());
                } else if *shown != (unread, toggles)
                    && handle
                        .update(|tray| {
                            tray.unread = unread;
                            tray.toggles = toggles;
                        })
                        .is_some()
                {
                    *shown = (unread, toggles);
                }
            }
        }
    }
}

impl Default for TrayState {
    fn default() -> Self {
        Self::new()
    }
}

fn start_registration(unread: u32, toggles: (bool, bool)) -> Receiver<Option<Handle<SniTray>>> {
    let (sender, receiver) = channel();
    // If the thread cannot spawn, `sender` is dropped with the closure and
    // the receiver reports `Disconnected`, which schedules a retry.
    let _ = std::thread::Builder::new()
        .name("quill-tray-sni".to_string())
        .spawn(move || {
            let handle = SniTray { unread, toggles }
                .disable_dbus_name(sandboxed())
                .spawn()
                .ok();
            let _ = sender.send(handle);
        });
    receiver
}
