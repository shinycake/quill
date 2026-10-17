//! The AppKit side of the Touch Bar: three `NSTouchBar`s (main, player,
//! media viewer) built once with their items, kept current from the app's
//! [`Bar`], and set on the main window as the app's state asks.
//!
//! Touches turn into [`Command`]s sent through a channel that a task on the
//! app's window drains, so no AppKit callback runs app code directly.

mod bars;
mod draw;
mod picker;
mod pinned;
#[cfg(feature = "demo-capture")]
mod preview;

use super::PickerContents;
use super::model::{Accessory, Bar, Command};
use crate::ui::app::QuillApp;
use gpui_kit::*;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{NSView, NSWindow};
use quill::advanced::experimental;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use std::task::{Poll, Waker};
use std::time::Duration;

/// At most this often after a change (the app notifies on every frame of
/// an animation).
const SYNC_THROTTLE: Duration = Duration::from_millis(33);

thread_local! {
    static HOST: RefCell<Option<bars::Host>> = const { RefCell::new(None) };
    /// Touches not yet run, and the task waiting for them.
    static INBOX: RefCell<(VecDeque<Command>, Option<Waker>)> =
        const { RefCell::new((VecDeque::new(), None)) };
    static SYNC_SCHEDULER: RefCell<Option<Rc<dyn Fn(&mut Context<QuillApp>)>>> =
        const { RefCell::new(None) };
}

pub(super) fn request_sync(cx: &mut Context<QuillApp>) {
    let schedule = SYNC_SCHEDULER.with(|slot| slot.try_borrow().ok().and_then(|slot| slot.clone()));
    if let Some(schedule) = schedule {
        schedule(cx);
    }
}

/// Hand a touch to the app.
pub(super) fn send(command: Command) {
    let waker = INBOX.with(|inbox| {
        let Ok(mut inbox) = inbox.try_borrow_mut() else {
            return None;
        };
        inbox.0.push_back(command);
        inbox.1.take()
    });
    if let Some(waker) = waker {
        waker.wake();
    }
}

/// The next touch, once there is one.
async fn next_command() -> Command {
    std::future::poll_fn(|cx| {
        INBOX.with(|inbox| {
            let Ok(mut inbox) = inbox.try_borrow_mut() else {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            };
            match inbox.0.pop_front() {
                Some(command) => Poll::Ready(command),
                None => {
                    inbox.1 = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
        })
    })
    .await
}

/// Close the picker popover (after a sticker was sent).
pub(super) fn dismiss_picker() {
    let item = HOST.with(|host| {
        host.try_borrow()
            .ok()
            .and_then(|host| host.as_ref().map(|host| host.picker_item()))
    });
    if let Some(item) = item {
        // SAFETY: a main-thread call on a live popover item.
        unsafe { item.dismissPopover(None) };
    }
}

/// The main window's `NSWindow`.
fn ns_window(window: &Window) -> Option<Retained<NSWindow>> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = HasWindowHandle::window_handle(window).ok()?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: GPUI's AppKit handle points at the window's live NSView.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    view.window()
}

struct SyncState {
    pending: Cell<bool>,
    /// The recent emoji the emoji list was built from, and that list.
    emoji: RefCell<(Option<Vec<String>>, Rc<Vec<super::model::PickerItem>>)>,
}

/// Demo capture asked for a picture of the bars.
fn preview_requested() -> bool {
    #[cfg(feature = "demo-capture")]
    return preview::requested().is_some();
    #[cfg(not(feature = "demo-capture"))]
    false
}

/// Whether the bar should be shown.
fn enabled() -> bool {
    preview_requested() || !experimental::touchbar_disabled()
}

pub(super) fn install(demo: bool, window: &mut Window, cx: &mut Context<QuillApp>) {
    if demo && !preview_requested() {
        return;
    }
    let handle = window.window_handle();
    let state = Rc::new(SyncState {
        pending: Cell::new(false),
        emoji: RefCell::new((None, Rc::default())),
    });
    // Touches run on the app in its window.
    QuillApp::spawn_main_in(window, cx, async move |this, cx| {
        loop {
            let command = next_command().await;
            let alive = this
                .update_in(cx, |this, window, cx| {
                    this.run_touch_bar_command(command, window, cx);
                })
                .is_ok();
            if !alive {
                break;
            }
        }
    })
    .detach();
    let schedule = {
        let state = state.clone();
        move |cx: &mut Context<QuillApp>| {
            if state.pending.get() {
                return;
            }
            let has_host = HOST.with(|host| host.try_borrow().is_ok_and(|host| host.is_some()));
            if !enabled() && !has_host {
                return;
            }
            state.pending.set(true);
            let state = state.clone();
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(SYNC_THROTTLE).await;
                state.pending.set(false);
                let _ = handle.update(cx, |_, window, cx| {
                    let _ = this.update(cx, |app, cx| sync(app, window, cx, &state));
                });
            })
            .detach();
        }
    };
    let shared: Rc<dyn Fn(&mut Context<QuillApp>)> = Rc::new(schedule.clone());
    SYNC_SCHEDULER.with(|slot| {
        if let Ok(mut slot) = slot.try_borrow_mut() {
            *slot = Some(shared);
        }
    });
    let on_app = schedule.clone();
    cx.observe_self(move |_, cx| on_app(cx)).detach();
    // The app is mid-update here, so its composer is read once it is not.
    let on_composer = schedule.clone();
    cx.defer_in(window, move |app, _, cx| {
        let Some(composer) = app.in_window_scope(None, cx, |app, _| app.composer.clone()) else {
            return;
        };
        cx.observe(&composer, move |_, _, cx| on_composer(cx))
            .detach();
    });
    let on_focus = schedule;
    cx.observe_window_activation(window, move |_, _, cx| on_focus(cx))
        .detach();
}

/// Mirror the app into the Touch Bar.
fn sync(app: &mut QuillApp, window: &mut Window, cx: &mut Context<QuillApp>, state: &SyncState) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    if !enabled() {
        let host = HOST.with(|host| host.try_borrow_mut().ok().and_then(|mut host| host.take()));
        if let Some(host) = host {
            host.remove();
        }
        return;
    }
    let bar = app.touch_bar_model(window, cx);
    let picker = matches!(
        bar,
        Bar::Main {
            accessory: Accessory::Picker,
            ..
        }
    )
    .then(|| picker_contents(app, state));
    let palette = QuillApp::touch_bar_palette(cx);
    let created = HOST.with(|host| host.try_borrow().is_ok_and(|host| host.is_some()));
    if !created {
        let Some(ns_window) = ns_window(window) else {
            return;
        };
        let icons = bars::Icons {
            stickers: kit_icon(cx, "icons/sticker.svg"),
            emoji: kit_icon(cx, "icons/face-slightly-smiling.svg"),
        };
        let host = bars::Host::new(ns_window, icons, mtm);
        HOST.with(|slot| {
            if let Ok(mut slot) = slot.try_borrow_mut() {
                *slot = Some(host);
            }
        });
    }
    // Clone the host's handles out so no borrow is held while AppKit runs.
    let host = HOST.with(|host| host.try_borrow().ok().and_then(|host| host.clone()));
    if let Some(host) = host {
        host.apply(&bar, picker.as_ref(), palette);
        #[cfg(feature = "demo-capture")]
        if let Some(path) = preview::requested() {
            preview::write(
                host.preview_rows(&bar, picker.as_ref(), palette),
                &path,
                mtm,
            );
        }
    }
}

/// A kit icon as a picker segment image (tdesktop `kSegmentIconSize`
/// at its 0.7 scale).
fn kit_icon(cx: &App, path: &str) -> Option<Retained<objc2_app_kit::NSImage>> {
    let data = cx.asset_source().load(path).ok()??;
    draw::svg_template(&data, 25. * 0.7)
}

/// The picker's lists; the emoji list is rebuilt only when the recent
/// emoji change.
fn picker_contents(app: &QuillApp, state: &SyncState) -> PickerContents {
    let mut contents = app.touch_bar_picker_stickers();
    let recent = app
        .session()
        .map(|session| session.settings.media_prefs.recent_emoji.clone())
        .unwrap_or_default();
    if let Ok(mut cache) = state.emoji.try_borrow_mut() {
        if cache.0.as_ref() != Some(&recent) {
            cache.1 = Rc::new(super::model::emoji_items(&recent));
            cache.0 = Some(recent);
        }
        contents.emoji = cache.1.clone();
    }
    contents
}
