//! Display changes, from GPUI's display events instead of polling.
//!
//! - The main window's display (where notification windows go by default,
//!   tdesktop `NotificationDisplayRect`) is noted when the window moves to
//!   another display (`observe_window_display`), not looked up every frame.
//! - A display added, removed or resized (`observe_displays`) puts Quill's
//!   notification windows back in place, shows notifications queued while
//!   there was no display (tdesktop `screenAdded` → `showNextFromQueue`),
//!   and redraws the settings' display list.
//!
//! Background: `docs/decisions/display-change-events.md`.

use super::app::QuillApp;
use gpui_kit::*;

/// What one display event asks of Quill.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct DisplayReaction {
    /// Put the notification windows back in place and show queued ones.
    pub(super) place_notifications: bool,
    /// Redraw: the settings list the displays by size.
    pub(super) redraw: bool,
}

/// A display connected or disconnected, or its bounds changed: the
/// notification windows move and the settings list changes. A new refresh
/// rate alone changes neither (video reads it from the window as it draws).
pub(super) fn reaction(event: DisplayEvent) -> DisplayReaction {
    let moved = match event {
        DisplayEvent::Added(_) | DisplayEvent::Removed(_) => true,
        DisplayEvent::Changed { changes, .. } => changes.contains(DisplayChanges::BOUNDS),
        _ => true,
    };
    DisplayReaction {
        place_notifications: moved,
        redraw: moved,
    }
}

/// Note the main window's display and follow display changes. Called once
/// while the main window's `QuillApp` is built.
pub(super) fn install(app: &mut QuillApp, window: &mut Window, cx: &mut Context<QuillApp>) {
    app.notify.windows.main_display = window.display(cx).map(|display| display.id());
    cx.observe_window_display(window, |this, window, cx| {
        this.notify.windows.main_display = window.display(cx).map(|display| display.id());
    })
    .detach();
    let app = cx.weak_entity();
    cx.observe_displays(move |event, cx| {
        let reaction = reaction(event);
        let _ = app.update(cx, |this, cx| {
            if reaction.place_notifications {
                this.custom_notify_displays_changed(cx);
            }
            if reaction.redraw {
                cx.notify();
            }
        });
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::{DisplayReaction, reaction};
    use gpui_kit::{DisplayChanges, DisplayEvent, DisplayId};

    const MOVED: DisplayReaction = DisplayReaction {
        place_notifications: true,
        redraw: true,
    };

    #[test]
    fn a_display_added_or_removed_moves_notifications_and_redraws() {
        let id = DisplayId::new(2);
        assert_eq!(reaction(DisplayEvent::Added(id)), MOVED);
        assert_eq!(reaction(DisplayEvent::Removed(id)), MOVED);
    }

    #[test]
    fn new_bounds_move_notifications_and_a_new_refresh_rate_alone_does_nothing() {
        let id = DisplayId::new(1);
        let changed = |changes| DisplayEvent::Changed { id, changes };
        assert_eq!(reaction(changed(DisplayChanges::BOUNDS)), MOVED);
        assert_eq!(
            reaction(changed(
                DisplayChanges::BOUNDS | DisplayChanges::REFRESH_INTERVAL
            )),
            MOVED
        );
        assert_eq!(
            reaction(changed(DisplayChanges::REFRESH_INTERVAL)),
            DisplayReaction::default()
        );
    }

    /// With the test platform: the main window's display is noted when the
    /// app is built, and a display plugged in redraws the app.
    #[cfg(feature = "demo-capture")]
    #[gpui_kit::test]
    fn the_app_notes_its_display_and_redraws_when_one_is_added(cx: &mut gpui_kit::TestAppContext) {
        use crate::ui::app::QuillApp;
        use crate::ui::screenshot_demo::ScreenshotDemo;
        use gpui_kit::AppContext;
        use gpui_kit::component::Root;
        use gpui_kit::{px, size};
        use std::cell::Cell;
        use std::rc::Rc;

        cx.update(gpui_kit::init);
        let slot = Rc::new(std::cell::RefCell::new(None));
        let slot_in = slot.clone();
        cx.open_window(size(px(1100.), px(800.)), move |window, cx| {
            let view = cx.new(|cx| {
                QuillApp::new_with_demo(
                    window,
                    cx,
                    None,
                    Some(ScreenshotDemo::named("ready-chats")),
                )
            });
            *slot_in.borrow_mut() = Some(view.clone());
            Root::new(view, window, cx)
        });
        let app = slot.borrow().clone().unwrap();
        cx.run_until_parked();
        let noted = app.read_with(cx, |app, _| app.notify.windows.main_display);
        assert!(noted.is_some(), "the test window is on a display");

        let redraws = Rc::new(Cell::new(0));
        let _watch = cx.update({
            let redraws = redraws.clone();
            |cx| cx.observe(&app, move |_, _| redraws.set(redraws.get() + 1))
        });
        cx.simulate_display_added(DisplayId::new(7), None);
        assert!(redraws.get() > 0, "a new display redraws the app");
        // Its own display did not change.
        let still = app.read_with(cx, |app, _| app.notify.windows.main_display);
        assert_eq!(still, noted);
    }
}
