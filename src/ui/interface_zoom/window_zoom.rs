//! Per-window zoom state and the resize callback runs (`fire_resize`).

use super::*;

pub(super) type ResizeCallback = Box<dyn FnMut(Size<Pixels>, f32)>;

/// Where [`WindowZoom::fire_resize`] stands: a zoom change that arrives
/// while the callback runs is not lost, the callback runs once more.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Refire {
    #[default]
    Idle,
    Running,
    RunAgain,
}

/// Most times in a row one `fire_resize` re-runs the callback.
pub(super) const MAX_REFIRES: u8 = 4;

/// Per-window zoom state shared between the window wrapper, its callbacks
/// and [`set_zoom`].
pub(super) struct WindowZoom {
    pub(super) zoom: Cell<f32>,
    /// GPUI's resize callback; [`set_zoom`] calls it so GPUI re-reads the
    /// viewport and scale factor.
    pub(super) resize: RefCell<Option<ResizeCallback>>,
    pub(super) refire: Cell<Refire>,
    /// The platform's last reported content size and display scale.
    pub(super) raw_size: Cell<Size<Pixels>>,
    pub(super) raw_scale: Cell<f32>,
    /// macOS traffic-light position in GPUI pixels, and the zoom it was
    /// last applied at.
    pub(super) traffic_light: Cell<Option<Point<Pixels>>>,
    pub(super) traffic_light_zoom: Cell<f32>,
}

impl WindowZoom {
    /// State for a newly opened window, registered for [`set_zoom`].
    pub(super) fn register(
        zoom: f32,
        raw_size: Size<Pixels>,
        raw_scale: f32,
        traffic_light: Option<Point<Pixels>>,
    ) -> Rc<Self> {
        let state = Rc::new(WindowZoom {
            zoom: Cell::new(zoom),
            resize: RefCell::new(None),
            refire: Cell::new(Refire::Idle),
            raw_size: Cell::new(raw_size),
            raw_scale: Cell::new(raw_scale),
            traffic_light: Cell::new(traffic_light),
            traffic_light_zoom: Cell::new(zoom),
        });
        WINDOWS.with(|list| {
            if let Ok(mut list) = list.try_borrow_mut() {
                list.retain(|w| w.strong_count() > 0);
                list.push(Rc::downgrade(&state));
            }
        });
        state
    }

    pub(super) fn fire_resize(&self) {
        // Take the callback out while it runs: GPUI's handler reads the
        // window back through the wrapper, and a nested zoom change must
        // not find the cell borrowed. That nested change asks for another
        // run instead of being skipped.
        if self.refire.get() != Refire::Idle {
            self.refire.set(Refire::RunAgain);
            return;
        }
        let Some(callback) = self
            .resize
            .try_borrow_mut()
            .ok()
            .and_then(|mut slot| slot.take())
        else {
            return;
        };
        // Back to idle with the callback in place even if it panics, so the
        // next zoom change runs it again.
        let mut run = RunningResize {
            state: self,
            callback: Some(callback),
        };
        for _ in 0..=MAX_REFIRES {
            self.refire.set(Refire::Running);
            let zoom = self.zoom.get();
            if let Some(callback) = run.callback.as_mut() {
                callback(
                    size_in(self.raw_size.get(), zoom),
                    self.raw_scale.get() * zoom,
                );
            }
            if self.refire.get() != Refire::RunAgain {
                break;
            }
        }
    }
}

/// The resize callback while [`WindowZoom::fire_resize`] runs it; dropping
/// it (also on a panic) ends the run.
pub(super) struct RunningResize<'a> {
    state: &'a WindowZoom,
    callback: Option<ResizeCallback>,
}

impl Drop for RunningResize<'_> {
    fn drop(&mut self) {
        self.state.refire.set(Refire::Idle);
        if let Ok(mut slot) = self.state.resize.try_borrow_mut()
            && slot.is_none()
        {
            *slot = self.callback.take();
        }
    }
}
