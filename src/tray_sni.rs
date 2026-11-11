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
//! from the 1s tray sync) picks the result up. Each D-Bus step the workers
//! wait on gives up after [`DBUS_LIMIT`], so a wedged bus leaves no thread
//! behind and the tray registers again. When no
//! `StatusNotifierWatcher` exists (stock GNOME without the AppIndicator
//! extension) registration fails, `available()` stays false, and the
//! close/minimize-to-tray switches stay hidden. Registration is retried
//! every [`RETRY_EVERY`], so a desktop that finishes starting later wins.
#![deny(clippy::indexing_slicing)]

use crate::icon_badge::LatestSlot;
use crate::locks::LockExt;
use crate::tray::{
    TrayAction, TrayToggles, notifications_label, render_tray_icon, rgba_to_argb32, sounds_label,
    tray_tooltip,
};
use ksni::{Handle, TrayMethods};
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Delay between registration attempts while the desktop has no tray host.
pub const RETRY_EVERY: Duration = Duration::from_secs(10);

/// How long a worker waits on the tray service before giving up.
const DBUS_LIMIT: Duration = crate::child_timeout::DESKTOP_QUERY_LIMIT;

/// Run `step` on this thread, or `None` when it has not finished within `limit`.
fn within<T>(limit: Duration, step: impl Future<Output = T>) -> Option<T> {
    async_io::block_on(futures_lite::future::or(
        async { Some(step.await) },
        async {
            async_io::Timer::after(limit).await;
            None
        },
    ))
}

/// Actions clicked in the tray menu, drained by [`take_actions`] on the UI
/// thread (ksni calls the menu callbacks on its own thread).
static ACTIONS: Mutex<Vec<TrayAction>> = Mutex::new(Vec::new());

fn push_action(action: TrayAction) {
    ACTIONS.lock_or_recover().push(action);
    crate::wake::CHROME.poke();
}

/// Drain pending tray menu / click actions.
pub fn take_actions() -> Vec<TrayAction> {
    std::mem::take(&mut *ACTIONS.lock_or_recover())
}

/// The StatusNotifierItem model ksni publishes over D-Bus.
struct SniTray {
    unread: u32,
    /// The notification toggles' labels, or none while Quill is locked.
    toggles: TrayToggles,
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
        crate::desktop_app_id::get().into()
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
        let mut items: Vec<ksni::MenuItem<Self>> = vec![
            StandardItem {
                label: "Open Quill".into(),
                activate: Box::new(|_| push_action(TrayAction::Open)),
                ..Default::default()
            }
            .into(),
        ];
        if let TrayToggles::Shown {
            notifications,
            sounds,
        } = self.toggles
        {
            items.push(
                StandardItem {
                    label: notifications_label(notifications).into(),
                    activate: Box::new(|_| push_action(TrayAction::ToggleNotifications)),
                    ..Default::default()
                }
                .into(),
            );
            items.push(
                StandardItem {
                    label: sounds_label(sounds).into(),
                    activate: Box::new(|_| push_action(TrayAction::ToggleSounds)),
                    ..Default::default()
                }
                .into(),
            );
        }
        items.extend([
            StandardItem {
                label: "Lock Quill".into(),
                activate: Box::new(|_| push_action(TrayAction::Lock)),
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
        ]);
        items
    }
}

enum Phase {
    /// No attempt in flight; the next one may start at the instant.
    Idle(Instant),
    /// A worker is registering with the watcher, with what the item is
    /// created showing (what it shows on landing).
    Connecting(Receiver<Option<Handle<SniTray>>>, Shown),
    /// Registered and showing.
    Ready {
        handle: Handle<SniTray>,
        publisher: Publisher,
        shown: Shown,
    },
}

type Shown = (u32, TrayToggles);

/// Hands tray changes to a worker. `Handle::update` waits for ksni's service
/// task, which waits on the session bus, so calling it here could hang the
/// UI thread on a wedged bus; the worker takes only the newest state.
struct Publisher {
    slot: Arc<LatestSlot<Option<Shown>>>,
    /// The worker stopped: the service is gone or did not answer in time.
    stopped: Arc<AtomicBool>,
}

impl Publisher {
    /// `apply` answers false once the tray service is gone; it and an
    /// `apply` still waiting after `limit` stop the worker.
    fn spawn<F>(limit: Duration, mut apply: impl FnMut(Shown) -> F + Send + 'static) -> Self
    where
        F: Future<Output = bool>,
    {
        let slot = Arc::new(LatestSlot::new());
        let stopped = Arc::new(AtomicBool::new(false));
        let worker = slot.clone();
        let done = stopped.clone();
        // A worker that cannot spawn leaves the slot undrained, so the icon
        // keeps its old count until the next registration.
        let _ = crate::thread_guard::spawn_guarded("quill-tray-update", move || {
            while let Some(shown) = worker.take() {
                if within(limit, apply(shown)) != Some(true) {
                    break;
                }
            }
            done.store(true, Ordering::Release);
        });
        Self { slot, stopped }
    }

    fn publish(&self, shown: Shown) {
        self.slot.put(Some(shown));
    }

    fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }
}

impl Drop for Publisher {
    fn drop(&mut self) {
        self.slot.put(None);
    }
}

fn spawn_publisher(handle: &Handle<SniTray>) -> Publisher {
    let handle = handle.clone();
    Publisher::spawn(DBUS_LIMIT, move |(unread, toggles)| {
        let handle = handle.clone();
        async move {
            handle
                .update(|tray| {
                    tray.unread = unread;
                    tray.toggles = toggles;
                })
                .await
                .is_some()
        }
    })
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

    /// "Show tray icon" turned off: unregister the item and stay idle. The
    /// next `poll` after the switch comes back on registers again.
    pub fn hide(&mut self) {
        if let Phase::Ready { handle, .. } = &self.phase {
            // The call sends the request; the awaiter only waits for it.
            drop(handle.shutdown());
        }
        if !matches!(self.phase, Phase::Idle(_)) {
            self.phase = Phase::Idle(Instant::now());
        }
    }

    /// Whether a registration attempt is still in flight. The window-start
    /// logic polls this to tell "no tray host" (reveal the window) from
    /// "tray still registering" without blocking the UI thread.
    pub fn registering(&self) -> bool {
        matches!(self.phase, Phase::Connecting(..))
    }

    /// Advance the lifecycle and publish `unread`.
    pub fn poll(&mut self, unread: u32, toggles: TrayToggles) {
        match &mut self.phase {
            Phase::Idle(not_before) => {
                if Instant::now() >= *not_before {
                    self.phase =
                        Phase::Connecting(start_registration(unread, toggles), (unread, toggles));
                }
            }
            Phase::Connecting(result, started_with) => match result.try_recv() {
                Ok(Some(handle)) => {
                    // The next poll publishes what changed while it registered.
                    self.phase = Phase::Ready {
                        publisher: spawn_publisher(&handle),
                        handle,
                        shown: *started_with,
                    }
                }
                Ok(None) | Err(TryRecvError::Disconnected) => {
                    self.phase = Phase::Idle(Instant::now() + RETRY_EVERY);
                }
                Err(TryRecvError::Empty) => {}
            },
            Phase::Ready {
                handle,
                publisher,
                shown,
            } => {
                if handle.is_closed() {
                    // The watcher went away (shell restart): register again.
                    self.phase = Phase::Idle(Instant::now());
                } else if publisher.stopped() {
                    // The service stopped answering: start a fresh one.
                    drop(handle.shutdown());
                    self.phase = Phase::Idle(Instant::now() + RETRY_EVERY);
                } else if *shown != (unread, toggles) {
                    publisher.publish((unread, toggles));
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

/// Give the registered handle to the UI thread. When nobody waits for it any
/// more ("Show tray icon" was turned off while this registered), the worker
/// shuts the item down itself: ksni keeps the icon until told to stop, and
/// a dropped handle does not.
fn hand_over<H>(
    sender: &std::sync::mpsc::Sender<Option<H>>,
    handle: Option<H>,
    shut_down: impl FnOnce(H),
) {
    if let Err(std::sync::mpsc::SendError(Some(unsent))) = sender.send(handle) {
        shut_down(unsent);
    }
}

fn start_registration(unread: u32, toggles: TrayToggles) -> Receiver<Option<Handle<SniTray>>> {
    let (sender, receiver) = channel();
    // If the thread cannot spawn, `sender` is dropped with the closure and
    // the receiver reports `Disconnected`, which schedules a retry.
    let _ = crate::thread_guard::spawn_guarded("quill-tray-sni", move || {
        let tray = SniTray { unread, toggles } // A Flatpak sandbox cannot own the well-known name ksni would take.
            .disable_dbus_name(crate::desktop_app_id::in_flatpak());
        let handle = within(DBUS_LIMIT, tray.spawn()).and_then(Result::ok);
        hand_over(&sender, handle, |orphan| drop(orphan.shutdown()));
    });
    receiver
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::sync_channel;

    const SHOWN: TrayToggles = TrayToggles::Shown {
        notifications: true,
        sounds: true,
    };
    const QUIET: TrayToggles = TrayToggles::Shown {
        notifications: false,
        sounds: true,
    };

    /// A registration that finishes after the tray was hidden shuts its own
    /// icon down; one somebody waits for is handed over.
    #[test]
    fn a_registration_nobody_waits_for_shuts_itself_down() {
        let (sender, receiver) = channel::<Option<u8>>();
        let mut shut = Vec::new();
        hand_over(&sender, Some(7), |orphan| shut.push(orphan));
        assert!(shut.is_empty(), "handed to the waiting receiver");
        assert_eq!(receiver.try_recv().ok(), Some(Some(7)));
        drop(receiver);
        hand_over(&sender, Some(8), |orphan| shut.push(orphan));
        assert_eq!(shut, [8]);
        hand_over(&sender, None, |orphan| shut.push(orphan));
        assert_eq!(shut, [8], "a failed registration has nothing to shut down");
    }

    #[test]
    fn publishing_never_waits_for_the_tray_service() {
        let (seen_tx, seen_rx) = channel();
        let (release_tx, release_rx) = sync_channel::<()>(0);
        let publisher = Publisher::spawn(Duration::MAX, move |shown| {
            seen_tx.send(shown).unwrap();
            std::future::ready(release_rx.recv().is_ok())
        });
        publisher.publish((1, SHOWN));
        // The worker is now stuck inside the first update, like ksni on a
        // wedged bus; the UI thread still publishes without waiting.
        assert_eq!(seen_rx.recv().unwrap(), (1, SHOWN));
        publisher.publish((2, SHOWN));
        publisher.publish((3, QUIET));
        release_tx.send(()).unwrap();
        assert_eq!(seen_rx.recv().unwrap(), (3, QUIET));
        release_tx.send(()).unwrap();
        drop(publisher);
        assert!(
            seen_rx.recv().is_err(),
            "the worker stops with its publisher"
        );
    }

    /// A service that takes the update and never answers (a wedged bus)
    /// stops the worker instead of keeping its thread, and the tray sees it.
    #[test]
    fn a_tray_service_that_never_answers_leaves_no_thread_behind() {
        let (seen_tx, seen_rx) = channel();
        let publisher = Publisher::spawn(Duration::from_millis(1), move |shown| {
            seen_tx.send(shown).unwrap();
            std::future::pending()
        });
        publisher.publish((1, SHOWN));
        assert_eq!(seen_rx.recv().unwrap(), (1, SHOWN));
        // The worker gave up and dropped `apply` with it.
        assert!(seen_rx.recv().is_err());
        assert!(publisher.stopped());
    }
}
