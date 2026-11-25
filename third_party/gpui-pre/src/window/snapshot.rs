// Added by the Quill project (2026). See third_party/gpui-pre/QUILL-CHANGES.md.
//! QUILL: snapshot transforms, the window's side
//! (docs/decisions/snapshot-transitions.md).
//!
//! A [`Snapshot`] holds a subtree's pixels in a texture for one
//! transition. Whoever runs the transition owns it, in the state of the
//! run, and drops it when the run ends; the window learns of the drop
//! through the table the handle shares with it and returns the texture to
//! the renderer at its next frame.

use std::{
    cell::Cell,
    rc::{Rc, Weak},
};

use super::Window;
use crate::scene::{CaptureContent, CaptureFate, CaptureTarget, Redraw};
use crate::{
    Bounds, DevicePixels, Hsla, Pixels, Point, ScaledPixels, Size, SnapshotQuad, SnapshotScene,
    SnapshotSlot, point,
};

/// The largest side of a snapshot's texture, in texels.
const MAX_SIDE: i32 = 16384;
/// What a window's snapshots may hold together, in multiples of the
/// window's own device pixels.
const AREA_LIMIT: f64 = 1.25;

/// What a [`Snapshot`] holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SnapshotState {
    /// Nothing: never captured, or the capture failed.
    Empty,
    /// A picture of `size` texels, drawn at the window's scale factor `scale`.
    Ready {
        /// The texels the picture covers.
        size: Size<DevicePixels>,
        /// The window's scale factor when it was drawn.
        scale: f32,
    },
    /// The renderer or the scale factor changed under it: capture again.
    Lost,
}

/// A subtree's pixels, held for one transition. Not `Send`. Dropping it
/// releases its texture.
pub struct Snapshot(Rc<SnapshotHandle>);

struct SnapshotHandle {
    slot: SnapshotSlot,
    state: Cell<SnapshotState>,
    /// The frame (`Window::scenes_drawn`) whose capture the texture holds.
    settled_in: Cell<u64>,
    table: Weak<SnapshotTable>,
}

impl Snapshot {
    /// What the snapshot holds.
    pub fn state(&self) -> SnapshotState {
        self.0.state.get()
    }

    /// Whether the snapshot holds a picture.
    pub fn is_ready(&self) -> bool {
        matches!(self.state(), SnapshotState::Ready { .. })
    }

    /// The slot the renderer keeps the texture under.
    pub fn slot(&self) -> SnapshotSlot {
        self.0.slot
    }

    /// Another handle to the same snapshot, for an element that draws it.
    pub(crate) fn share(&self) -> Snapshot {
        Snapshot(self.0.clone())
    }

    /// Whether the snapshot belongs to the window whose table is `table`.
    fn of(&self, table: &Rc<SnapshotTable>) -> bool {
        Weak::ptr_eq(&self.0.table, &Rc::downgrade(table))
    }
}

impl std::fmt::Debug for Snapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Snapshot")
            .field("slot", &self.0.slot)
            .field("state", &self.0.state.get())
            .finish()
    }
}

impl Drop for SnapshotHandle {
    fn drop(&mut self) {
        if let Some(table) = self.table.upgrade() {
            table.release(self.slot);
        }
    }
}

/// How a snapshot is drawn: scaled by `scale` about `origin` (window
/// coordinates), then moved by `translation`, at `opacity`. Uniform scale
/// and translation only, so the drawn quad stays axis-aligned and the clip
/// in force cuts it exactly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapshotDraw {
    /// Uniform scale, above 0.
    pub scale: f32,
    /// The point the scale keeps in place.
    pub origin: Point<Pixels>,
    /// Applied after the scale.
    pub translation: Point<Pixels>,
    /// Multiplies the picture's alpha.
    pub opacity: f32,
}

impl Default for SnapshotDraw {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl SnapshotDraw {
    /// The picture where its subtree is, as it is: what hands back to the
    /// live elements with no visible step.
    pub const IDENTITY: Self = Self {
        scale: 1.0,
        origin: Point {
            x: Pixels::ZERO,
            y: Pixels::ZERO,
        },
        translation: Point {
            x: Pixels::ZERO,
            y: Pixels::ZERO,
        },
        opacity: 1.0,
    };

    /// Scaled by `scale` about `origin`.
    pub fn scaled_about(origin: Point<Pixels>, scale: f32) -> Self {
        Self {
            scale,
            origin,
            ..Self::IDENTITY
        }
    }

    /// Moved by `by`.
    pub fn moved(by: Point<Pixels>) -> Self {
        Self {
            translation: by,
            ..Self::IDENTITY
        }
    }

    /// The same, at `opacity`.
    pub fn faded(self, opacity: f32) -> Self {
        Self { opacity, ..self }
    }

    /// `bounds` (device pixels) drawn so; `scale_factor` turns the draw's
    /// pixels into device pixels.
    fn apply(&self, bounds: Bounds<ScaledPixels>, scale_factor: f32) -> Bounds<ScaledPixels> {
        let scale = if self.scale.is_finite() && self.scale > 0.0 {
            self.scale
        } else {
            1.0
        };
        let about = self.origin.scale(scale_factor);
        let by = self.translation.scale(scale_factor);
        let map = |v: f32, about: f32, by: f32| about + (v - about) * scale + by;
        Bounds {
            origin: point(
                ScaledPixels(map(bounds.origin.x.0, about.x.0, by.x.0)),
                ScaledPixels(map(bounds.origin.y.0, about.y.0, by.y.0)),
            ),
            size: Size {
                width: ScaledPixels(bounds.size.width.0 * scale),
                height: ScaledPixels(bounds.size.height.0 * scale),
            },
        }
    }
}

/// The window's live snapshots, shared with their handles.
#[derive(Default)]
pub(crate) struct SnapshotTable {
    next: Cell<u32>,
    live: Cell<Vec<Weak<SnapshotHandle>>>,
    /// Slots whose handles dropped since the last frame took them.
    released: Cell<Vec<SnapshotSlot>>,
    /// The scale factor and the renderer's loss count the live snapshots
    /// were drawn under, once a frame has set them.
    drawn_under: Cell<Option<(f32, u64)>>,
}

impl SnapshotTable {
    fn new_snapshot(table: &Rc<Self>) -> Snapshot {
        let slot = SnapshotSlot(table.next.get());
        table.next.set(table.next.get().wrapping_add(1));
        let handle = Rc::new(SnapshotHandle {
            slot,
            state: Cell::new(SnapshotState::Empty),
            settled_in: Cell::new(0),
            table: Rc::downgrade(table),
        });
        let mut live = table.live.take();
        live.retain(|handle| handle.strong_count() > 0);
        live.push(Rc::downgrade(&handle));
        table.live.set(live);
        Snapshot(handle)
    }

    fn release(&self, slot: SnapshotSlot) {
        let mut released = self.released.take();
        released.push(slot);
        self.released.set(released);
    }

    fn live(&self) -> Vec<Rc<SnapshotHandle>> {
        let live = self.live.take();
        let handles = live.iter().filter_map(Weak::upgrade).collect();
        self.live.set(live);
        handles
    }

    fn handle(&self, slot: SnapshotSlot) -> Option<Rc<SnapshotHandle>> {
        self.live().into_iter().find(|handle| handle.slot == slot)
    }

    /// Texels held by the live snapshots but `except`.
    fn held_texels(&self, except: Option<SnapshotSlot>) -> f64 {
        self.live()
            .iter()
            .filter(|handle| Some(handle.slot) != except)
            .map(|handle| match handle.state.get() {
                SnapshotState::Ready { size, .. } => texels(size),
                SnapshotState::Empty | SnapshotState::Lost => 0.0,
            })
            .sum()
    }

    /// Marks every live picture lost; returns whether there was one.
    fn lose_all(&self) -> bool {
        let mut any = false;
        for handle in self.live() {
            if let SnapshotState::Ready { .. } = handle.state.get() {
                handle.state.set(SnapshotState::Lost);
                any = true;
            }
        }
        any
    }

    /// The end of a frame's capture into `target`: what becomes of it.
    fn settle(
        &self,
        target: &CaptureTarget,
        content: CaptureContent,
        scale: f32,
        frame: u64,
    ) -> CaptureFate {
        let Some(handle) = self.handle(target.slot) else {
            return CaptureFate::Live;
        };
        if content == CaptureContent::Uncapturable {
            handle.state.set(SnapshotState::Empty);
            self.release(target.slot);
            return CaptureFate::Live;
        }
        let ready = SnapshotState::Ready {
            size: target.size,
            scale,
        };
        let held = handle.state.get() == ready && handle.settled_in.get().wrapping_add(1) == frame;
        handle.state.set(ready);
        handle.settled_in.set(frame);
        if held && target.redraw == Redraw::IfPainted && content == CaptureContent::Replayed {
            CaptureFate::Keep
        } else {
            CaptureFate::Draw
        }
    }
}

fn texels(size: Size<DevicePixels>) -> f64 {
    f64::from(size.width.0.max(0)) * f64::from(size.height.0.max(0))
}

/// `bounds` scaled to device pixels and grown to whole ones.
fn snapped(bounds: Bounds<Pixels>, scale_factor: f32) -> (Bounds<ScaledPixels>, Size<DevicePixels>) {
    let scaled = bounds.scale(scale_factor);
    let left = scaled.origin.x.0.floor();
    let top = scaled.origin.y.0.floor();
    let right = (scaled.origin.x.0 + scaled.size.width.0).ceil();
    let bottom = (scaled.origin.y.0 + scaled.size.height.0).ceil();
    let width = (right - left).max(0.0);
    let height = (bottom - top).max(0.0);
    (
        Bounds {
            origin: point(ScaledPixels(left), ScaledPixels(top)),
            size: Size {
                width: ScaledPixels(width),
                height: ScaledPixels(height),
            },
        },
        Size {
            width: DevicePixels(width.min(i32::MAX as f32) as i32),
            height: DevicePixels(height.min(i32::MAX as f32) as i32),
        },
    )
}

impl Window {
    /// QUILL: whether this window can draw snapshots. When it can't, every
    /// owner of a transition takes its fallback.
    pub fn snapshots_supported(&self) -> bool {
        self.platform_window.snapshots_supported()
    }

    /// QUILL: a new, empty snapshot of this window.
    pub fn new_snapshot(&self) -> Snapshot {
        SnapshotTable::new_snapshot(&self.snapshots)
    }

    /// QUILL: paints what `f` paints into `snapshot` and not into the
    /// frame. `bounds` is the region captured, grown to whole device
    /// pixels; `backing` clears the texture to an opaque colour first (so
    /// that subpixel text stays correct), `None` leaves it transparent.
    ///
    /// When the snapshot can't be taken (no support, too large, the
    /// window's limit), `f` paints live and the snapshot is left `Empty`;
    /// a native view in `f` fails the capture the same way, at the end of
    /// the frame. Call [`Self::paint_snapshot`] after this to show it.
    pub fn capture_snapshot<R>(
        &mut self,
        snapshot: &Snapshot,
        bounds: Bounds<Pixels>,
        backing: Option<Hsla>,
        f: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.invalidator.debug_assert_paint();
        let Some(target) = self.snapshot_target(snapshot, bounds, backing) else {
            if snapshot.state() != SnapshotState::Empty {
                snapshot.0.state.set(SnapshotState::Empty);
                self.snapshots.release(snapshot.slot());
            }
            return f(self);
        };
        self.next_frame.scene.begin_capture(target);
        let result = f(self);
        self.next_frame.scene.end_capture();
        snapshot.0.state.set(SnapshotState::Ready {
            size: target.size,
            scale: self.scale_factor(),
        });
        result
    }

    fn snapshot_target(
        &self,
        snapshot: &Snapshot,
        bounds: Bounds<Pixels>,
        backing: Option<Hsla>,
    ) -> Option<CaptureTarget> {
        if !self.snapshots_supported() || !snapshot.of(&self.snapshots) {
            return None;
        }
        let scale = self.scale_factor();
        let (device, size) = snapped(bounds, scale);
        if !self.snapshot_fits(size, Some(snapshot.slot())) {
            return None;
        }
        let redraw = match snapshot.state() {
            SnapshotState::Ready { size: held, scale: at } if held == size && at == scale => {
                Redraw::IfPainted
            }
            _ => Redraw::Needed,
        };
        Some(CaptureTarget {
            slot: snapshot.slot(),
            origin: device.origin,
            size,
            backing,
            redraw,
        })
    }

    /// Whether a picture of `size` fits beside what the window's other
    /// snapshots hold.
    fn snapshot_fits(&self, size: Size<DevicePixels>, except: Option<SnapshotSlot>) -> bool {
        let sides = |side: i32| side > 0 && side <= MAX_SIDE;
        if !sides(size.width.0) || !sides(size.height.0) {
            return false;
        }
        let window = self.viewport_size.scale(self.scale_factor());
        let limit = f64::from(window.width.0) * f64::from(window.height.0) * AREA_LIMIT;
        self.snapshots.held_texels(except) + texels(size) <= limit
    }

    /// QUILL: draws `snapshot` over `bounds` (the bounds it was captured
    /// at, or the region of a last-frame snapshot) as `draw` says, clipped
    /// by the content mask in force. Draws nothing unless the snapshot is
    /// `Ready`. At [`SnapshotDraw::IDENTITY`] a captured snapshot lands
    /// texel for pixel on what it captured.
    pub fn paint_snapshot(&mut self, snapshot: &Snapshot, bounds: Bounds<Pixels>, draw: SnapshotDraw) {
        self.invalidator.debug_assert_paint();
        let SnapshotState::Ready { size, .. } = snapshot.state() else {
            return;
        };
        if !snapshot.of(&self.snapshots) {
            return;
        }
        let scale_factor = self.scale_factor();
        let (device, _) = snapped(bounds, scale_factor);
        let opacity = if draw.opacity.is_finite() {
            draw.opacity.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let quad = SnapshotQuad {
            order: 0,
            bounds: draw.apply(device, scale_factor),
            content_mask: self.content_mask().scale(scale_factor),
            opacity: opacity * self.element_opacity(),
            slot: snapshot.slot(),
            size,
        };
        self.next_frame.scene.insert_primitive(quad);
    }

    /// QUILL: `bounds` as the last presented frame drew it, drawn into a
    /// new snapshot now, outside a frame, `scale` (at most 1) times as
    /// many texels as device pixels; at half scale when the full one would
    /// not fit. `None` where snapshots aren't supported, the region holds a
    /// native view or glass, or the picture doesn't fit.
    pub fn snapshot_last_frame(&mut self, bounds: Bounds<Pixels>, scale: f32) -> Option<Snapshot> {
        if !self.snapshots_supported() || !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let scale = scale.min(1.0);
        let (region, _) = snapped(bounds, self.scale_factor());
        let texel_size = |density: f32| Size {
            width: DevicePixels((region.size.width.0 * density).ceil() as i32),
            height: DevicePixels((region.size.height.0 * density).ceil() as i32),
        };
        let density = [scale, 0.5]
            .into_iter()
            .filter(|density| *density <= scale)
            .find(|density| self.snapshot_fits(texel_size(*density), None))?;
        let scene = self.rendered_frame.scene.region_snapshot(region, density)?;
        let snapshot = self.new_snapshot();
        let size = texel_size(density);
        let drawn = self.platform_window.draw_snapshot(&SnapshotScene {
            slot: snapshot.slot(),
            size,
            backing: None,
            scene,
        });
        if !drawn {
            return None;
        }
        snapshot.0.state.set(SnapshotState::Ready {
            size,
            scale: self.scale_factor(),
        });
        Some(snapshot)
    }

    /// QUILL: at the start of a frame, marks every live picture `Lost` when
    /// the scale factor changed or the renderer lost its textures since
    /// they were drawn, and draws everything again so that their owners
    /// capture anew.
    pub(super) fn check_snapshots(&mut self) {
        let now = (self.scale_factor(), self.platform_window.snapshot_losses());
        let before = self.snapshots.drawn_under.replace(Some(now));
        if before.is_some_and(|before| before != now) && self.snapshots.lose_all() {
            self.refresh();
        }
    }

    /// QUILL: at the end of painting, settles the frame's captures and
    /// hands the renderer the textures to release.
    pub(super) fn settle_snapshots(&mut self) {
        let table = self.snapshots.clone();
        let scale = self.scale_factor();
        let frame = self.scenes_drawn;
        let scene = &mut self.next_frame.scene;
        scene.settle_captures(|target, content| table.settle(target, content, scale, frame));
        scene.snapshot_releases = table.released.take();
    }

    /// QUILL: how many snapshots of this window are alive, for tests.
    #[cfg(any(test, feature = "test-support"))]
    pub fn live_snapshot_count(&self) -> usize {
        self.snapshots.live().len()
    }

    /// QUILL: on a test window, whether it reports snapshot support (off
    /// unless a test turns it on).
    #[cfg(any(test, feature = "test-support"))]
    pub fn set_test_snapshots_supported(&mut self, supported: bool) {
        if let Some(window) = self.platform_window.as_test() {
            window.set_snapshots_supported(supported);
        }
    }

    /// QUILL: on a test window, the renderer loses every snapshot texture.
    #[cfg(any(test, feature = "test-support"))]
    pub fn simulate_test_snapshot_loss(&mut self) {
        if let Some(window) = self.platform_window.as_test() {
            window.simulate_snapshot_loss();
        }
    }

    /// QUILL: on a test window, the snapshots drawn so far and the textures
    /// released, in order.
    #[cfg(any(test, feature = "test-support"))]
    pub fn test_snapshot_history(&mut self) -> (Vec<SnapshotSlot>, Vec<SnapshotSlot>) {
        match self.platform_window.as_test() {
            Some(window) => (window.drawn_snapshots(), window.released_snapshots()),
            None => (Vec::new(), Vec::new()),
        }
    }
}
