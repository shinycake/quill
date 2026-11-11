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
use crate::ui::on_screen::ScreenEpoch;
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
    /// Touches not yet run, each with the screen of the bar it came from,
    /// and the task waiting for them.
    static INBOX: RefCell<(VecDeque<(ScreenEpoch, Command)>, Option<Waker>)> =
        const { RefCell::new((VecDeque::new(), None)) };
    /// The screen the bar on the main window was built for: a touch is
    /// that screen's, whatever is on screen by the time it runs.
    static BAR_SCREEN: Cell<ScreenEpoch> = Cell::new(ScreenEpoch::default());
    /// What the host was last given; an identical bar is not applied again.
    static APPLIED: RefCell<Option<(Bar, Option<PickerContents>, super::Palette)>> =
        const { RefCell::new(None) };
    static SYNC_SCHEDULER: RefCell<Option<Rc<dyn Fn(&mut Context<QuillApp>)>>> =
        const { RefCell::new(None) };
    /// What mirrors the app into the bar of the main window it is bound
    /// to. A main window opened again (the GPU was lost) binds anew.
    static BINDING: RefCell<Option<Binding>> = const { RefCell::new(None) };
}

/// The main window the bar is bound to, with the task that runs its
/// touches and the observers that keep it current; they go with it.
struct Binding {
    window: WindowId,
    _touches: Task<()>,
    _observers: [Subscription; 2],
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
        inbox.0.push_back((BAR_SCREEN.with(Cell::get), command));
        inbox.1.take()
    });
    if let Some(waker) = waker {
        waker.wake();
    }
}

/// The next touch, once there is one.
async fn next_command() -> (ScreenEpoch, Command) {
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

/// The next sync hands the host the bar again, whatever it was given last.
fn forget_applied() {
    APPLIED.with(|applied| {
        if let Ok(mut applied) = applied.try_borrow_mut() {
            *applied = None;
        }
    });
}

/// The pinned order a drop asked for was refused: the panel shows the
/// model's order again at the next sync.
pub(super) fn reorder_refused(cx: &mut Context<QuillApp>) {
    let host = HOST.with(|host| host.try_borrow().ok().and_then(|host| host.clone()));
    if let Some(host) = host {
        host.reorder_refused();
    }
    forget_applied();
    request_sync(cx);
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
    /// The composer entity a selection change is followed on. Every account
    /// on screen has its own, so a sync that finds another one follows it.
    followed_composer: Cell<Option<EntityId>>,
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
    let id = handle.window_id();
    let bound = BINDING.with(|binding| {
        binding
            .try_borrow()
            .is_ok_and(|binding| binding.as_ref().is_some_and(|binding| binding.window == id))
    });
    if bound {
        return;
    }
    // Another main window: the host holds the old one's NSWindow and the
    // inbox its touches, which the new window must not inherit.
    let old_host = HOST.with(|host| host.try_borrow_mut().ok().and_then(|mut host| host.take()));
    if let Some(host) = old_host {
        host.remove();
    }
    INBOX.with(|inbox| {
        if let Ok(mut inbox) = inbox.try_borrow_mut() {
            inbox.0.clear();
        }
    });
    forget_applied();
    let state = Rc::new(SyncState {
        pending: Cell::new(false),
        followed_composer: Cell::new(None),
        emoji: RefCell::new((None, Rc::default())),
    });
    // Touches run on the app in its window.
    // screen-scope: a touch bar press acts on whichever account is on screen
    // when it is pressed; it carries no state of an earlier screen
    let touches = QuillApp::spawn_app_in(window, cx, async move |this, cx| {
        loop {
            let (screen, command) = next_command().await;
            let alive = this
                .update_in(cx, |this, window, cx| {
                    // A touch on the bar of an account that left the screen
                    // is not the account on screen's.
                    if this.screen_epoch() != screen {
                        return;
                    }
                    // The press is the touch bar's window's.
                    let from = this.window_scope_of(window);
                    this.as_asked_by(from, |this| this.run_touch_bar_command(command, window, cx));
                })
                .is_ok();
            if !alive {
                break;
            }
        }
    });
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
    let observe_app = cx.observe_self(move |_, cx| on_app(cx));
    let on_focus = schedule;
    let observe_focus = cx.observe_window_activation(window, move |_, _, cx| on_focus(cx));
    let binding = Binding {
        window: id,
        _touches: touches,
        _observers: [observe_app, observe_focus],
    };
    // The old binding is let go after the slot is released.
    let old = BINDING.with(|slot| {
        slot.try_borrow_mut()
            .ok()
            .and_then(|mut slot| slot.replace(binding))
    });
    drop(old);
}

/// Follow the composer on screen now: a selection change in it schedules a
/// sync (the formatting popover follows the selection). An account switch
/// builds a new composer, so this runs on every sync and observes the new
/// one; the old observation ended with its entity.
fn follow_composer(app: &mut QuillApp, cx: &mut Context<QuillApp>, state: &SyncState) {
    // screen-scope: only schedules a sync, which reads the screen's state
    // when it runs
    let Some(composer) = app.in_window_scope(None, cx, |app, _| app.composer.clone()) else {
        return;
    };
    if state.followed_composer.get() == Some(composer.entity_id()) {
        return;
    }
    let Some(schedule) = SYNC_SCHEDULER.with(|slot| slot.try_borrow().ok().and_then(|s| s.clone()))
    else {
        return;
    };
    state.followed_composer.set(Some(composer.entity_id()));
    cx.observe(&composer, move |_, _, cx| schedule(cx)).detach();
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
    // A window hidden to the tray shows no bar; its activation syncs again.
    if !window.is_visible() {
        return;
    }
    follow_composer(app, cx, state);
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
        // A host made now has been given nothing yet.
        forget_applied();
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
        // The app notifies on every frame of an animation: the same bar as
        // the last one is not handed to AppKit again.
        let unchanged = APPLIED.with(|applied| {
            applied.try_borrow().is_ok_and(|applied| {
                applied
                    .as_ref()
                    .is_some_and(|(last_bar, last_picker, last_palette)| {
                        *last_bar == bar && *last_picker == picker && *last_palette == palette
                    })
            })
        });
        if !unchanged || preview_requested() {
            host.apply(&bar, picker.as_ref(), palette);
            BAR_SCREEN.with(|screen| screen.set(app.screen_epoch()));
            APPLIED.with(|applied| {
                if let Ok(mut applied) = applied.try_borrow_mut() {
                    *applied = Some((bar.clone(), picker.clone(), palette));
                }
            });
        }
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
