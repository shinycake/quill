// Added by the Quill project (2026). See third_party/gpui-pre/QUILL-CHANGES.md.
//! QUILL: snapshot transforms, the scene's side
//! (docs/decisions/snapshot-transitions.md).
//!
//! A capture is a range of paint operations, between a `StartCapture` and
//! an `EndCapture` operation, whose primitives are drawn into a texture of
//! their own instead of into the frame. The operations stay in
//! `Scene::paint_operations`, so a cached view that painted them replays
//! them, markers included, as it replays anything else; only
//! `Scene::push_primitive` diverts them, by the index of their operation.
//! A view painted inside a capture replays its own range, which holds no
//! marker, and the capture around it takes it. At the end of the frame
//! the window settles each capture: its primitives become a
//! [`SnapshotScene`] for the renderer, go back into the frame, or are
//! dropped because the texture already holds them. The frame draws the
//! texture with a [`SnapshotQuad`].

use std::ops::Range;

use super::{DrawOrder, PaddedBool32, PaintOperation, PaintSurface, Primitive, Scene, SlotSpace};
use crate::{
    AtlasTextureId, AtlasTextureKind, AtlasTile, Bounds, ContentMask, Corners, DevicePixels,
    Hsla, Point, PolychromeSprite, ScaledPixels, Size, TileId,
};

/// Names a snapshot's texture. The window hands them out; the renderer
/// keeps one texture per live slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SnapshotSlot(pub u32);

/// A snapshot's texture drawn in the frame: the texels `(0, 0)..size`
/// stretched over `bounds`, at `opacity`, clipped by `content_mask`.
#[derive(Clone, Debug)]
pub struct SnapshotQuad {
    /// The quad's place in the scene's drawing order.
    pub order: DrawOrder,
    /// Where the texels land, device pixels.
    pub bounds: Bounds<ScaledPixels>,
    /// The clip in force where it was painted.
    pub content_mask: ContentMask<ScaledPixels>,
    /// Multiplies the texture's alpha.
    pub opacity: f32,
    /// The texture.
    pub slot: SnapshotSlot,
    /// How many of the texture's texels hold the snapshot, from its top left.
    pub size: Size<DevicePixels>,
}

impl SnapshotQuad {
    /// The quad as the polychrome sprite pipeline draws it, its tile the
    /// snapshot's texels. Its flags say the texture holds premultiplied
    /// colour, which the shader divides out before blending.
    pub fn sprite(&self) -> PolychromeSprite {
        PolychromeSprite {
            order: self.order,
            pad: 0,
            tile: AtlasTile {
                texture_id: AtlasTextureId {
                    index: self.slot.0,
                    kind: AtlasTextureKind::Polychrome,
                },
                tile_id: TileId(self.slot.0),
                padding: 0,
                bounds: Bounds {
                    origin: Point::default(),
                    size: self.size,
                },
            },
            grayscale: PaddedBool32::PREMULTIPLIED,
            opacity: self.opacity,
            bounds: self.bounds,
            content_mask: self.content_mask,
            corner_radii: Corners::default(),
        }
    }
}

impl From<SnapshotQuad> for Primitive {
    fn from(quad: SnapshotQuad) -> Self {
        Primitive::Snapshot(quad)
    }
}

/// A scene the renderer draws into a snapshot's texture before the frame
/// that holds this list: `scene` is in the texture's texels, (0, 0) at its
/// top left.
pub struct SnapshotScene {
    /// The texture.
    pub slot: SnapshotSlot,
    /// The texels the scene covers.
    pub size: Size<DevicePixels>,
    /// What the texture is cleared to: an opaque colour, or transparent.
    pub backing: Option<Hsla>,
    /// What is drawn.
    pub scene: Scene,
}

/// Whether a capture's texture must be drawn again even when nothing in
/// it was painted afresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Redraw {
    /// The snapshot holds nothing of this size and scale yet.
    Needed,
    /// The texture holds what a replay of last frame's capture paints.
    IfPainted,
}

/// Where a capture goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CaptureTarget {
    pub(crate) slot: SnapshotSlot,
    /// The texture's top left in the frame, whole device pixels.
    pub(crate) origin: Point<ScaledPixels>,
    pub(crate) size: Size<DevicePixels>,
    pub(crate) backing: Option<Hsla>,
    pub(crate) redraw: Redraw,
}

impl CaptureTarget {
    /// The target of a replay of the capture: the texture holds what the
    /// replay paints unless something in it is painted afresh.
    pub(crate) fn replayed(&self) -> Self {
        Self {
            redraw: Redraw::IfPainted,
            ..*self
        }
    }
}

/// What reached a capture this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaptureContent {
    /// Only whole ranges replayed from the last frame, which painted them.
    Replayed,
    /// Something was inserted afresh.
    Painted,
    /// A native view, which can't be drawn into a texture: the capture
    /// fails and its primitives draw live.
    Uncapturable,
}

/// The operations a capture spans.
#[derive(Clone, Debug, PartialEq, Eq)]
enum CaptureOps {
    /// Being painted: everything from here on.
    Open { from: usize },
    Closed(Range<usize>),
}

impl CaptureOps {
    fn contains(&self, op: usize) -> bool {
        match self {
            CaptureOps::Open { from } => op >= *from,
            CaptureOps::Closed(range) => range.contains(&op),
        }
    }
}

pub(crate) struct Capture {
    pub(crate) target: CaptureTarget,
    ops: CaptureOps,
    pub(crate) content: CaptureContent,
    /// The diverted primitives, each with its operation's index.
    primitives: Vec<(usize, Primitive)>,
}

/// What the window decided for a capture at the end of the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CaptureFate {
    /// Draw its primitives into the texture.
    Draw,
    /// The texture already holds them: drop them.
    Keep,
    /// Draw them in the frame instead.
    Live,
}

impl Scene {
    /// Starts diverting what is painted from here on into `target`.
    pub(crate) fn begin_capture(&mut self, target: CaptureTarget) {
        self.capture_stack.push(self.captures.len());
        self.captures.push(Capture {
            target,
            ops: CaptureOps::Open {
                from: self.paint_operations.len(),
            },
            content: CaptureContent::Replayed,
            primitives: Vec::new(),
        });
        self.paint_operations
            .push(PaintOperation::StartCapture(target));
    }

    /// Ends the innermost capture begun.
    pub(crate) fn end_capture(&mut self) {
        self.paint_operations.push(PaintOperation::EndCapture);
        let end = self.paint_operations.len();
        if let Some(capture) = self
            .capture_stack
            .pop()
            .and_then(|index| self.captures.get_mut(index))
            && let CaptureOps::Open { from } = capture.ops
        {
            capture.ops = CaptureOps::Closed(from..end);
        }
    }

    /// Every open capture now holds something painted afresh.
    pub(super) fn note_painted(&mut self) {
        for &index in &self.capture_stack {
            if let Some(capture) = self.captures.get_mut(index)
                && capture.content == CaptureContent::Replayed
            {
                capture.content = CaptureContent::Painted;
            }
        }
    }

    /// The innermost capture among the first `among` whose operations hold `op`.
    fn capture_holding(&self, op: usize, among: usize) -> Option<usize> {
        self.captures
            .get(..among)?
            .iter()
            .rposition(|capture| capture.ops.contains(op))
    }

    /// Takes `primitive` (of operation `op`) into the capture that holds
    /// `op`, if any; gives it back otherwise. A lens stays in the frame,
    /// where it reads what is under it; a native view stays too, and fails
    /// its capture.
    pub(super) fn divert(&mut self, op: usize, primitive: Primitive) -> Option<Primitive> {
        if self.captures.is_empty() {
            return Some(primitive);
        }
        let Some(index) = self.capture_holding(op, self.captures.len()) else {
            return Some(primitive);
        };
        let capture = self.captures.get_mut(index)?;
        match primitive {
            Primitive::Lens(_) => Some(primitive),
            Primitive::NativeView(_) => {
                capture.content = CaptureContent::Uncapturable;
                Some(primitive)
            }
            primitive => {
                capture.primitives.push((op, primitive));
                None
            }
        }
    }

    /// Records a capture for each pair of markers among the operations a
    /// replay copied to `first_operation..`, before their primitives are
    /// pushed. A marker left open at the range's end closes there.
    pub(super) fn add_replayed_captures(&mut self, first_operation: usize) {
        let end = self.paint_operations.len();
        let mut open: Vec<(CaptureTarget, usize)> = Vec::new();
        let mut found = Vec::new();
        for (op, operation) in self
            .paint_operations
            .get(first_operation..)
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let op = first_operation + op;
            match operation {
                PaintOperation::StartCapture(target) => open.push((*target, op)),
                PaintOperation::EndCapture => {
                    if let Some((target, from)) = open.pop() {
                        found.push((target, from..op + 1));
                    }
                }
                _ => {}
            }
        }
        found.extend(open.into_iter().rev().map(|(target, from)| (target, from..end)));
        // By where they begin, as painting them records them: inner after outer.
        found.sort_by_key(|(_, ops)| ops.start);
        for (target, ops) in found {
            self.captures.push(Capture {
                target,
                ops: CaptureOps::Closed(ops),
                content: CaptureContent::Replayed,
                primitives: Vec::new(),
            });
        }
    }

    /// Settles every capture of the frame, inner ones first, with what
    /// `decide` says: its scene goes to `snapshot_scenes`, its primitives
    /// back into the frame (or the capture around it), or nowhere.
    pub(crate) fn settle_captures(
        &mut self,
        mut decide: impl FnMut(&CaptureTarget, CaptureContent) -> CaptureFate,
    ) {
        debug_assert!(self.capture_stack.is_empty(), "unbalanced captures");
        for index in (0..self.captures.len()).rev() {
            let Some(capture) = self.captures.get_mut(index) else {
                continue;
            };
            let target = capture.target;
            let content = capture.content;
            let primitives = std::mem::take(&mut capture.primitives);
            match decide(&target, content) {
                CaptureFate::Draw => {
                    let scene = captured_scene(&target, primitives);
                    self.snapshot_scenes.push(SnapshotScene {
                        slot: target.slot,
                        size: target.size,
                        backing: target.backing,
                        scene,
                    });
                }
                CaptureFate::Keep => {}
                CaptureFate::Live => {
                    for (op, primitive) in primitives {
                        let order = primitive.order();
                        match self.capture_holding(op, index) {
                            Some(outer) => {
                                if let Some(outer) = self.captures.get_mut(outer) {
                                    outer.primitives.push((op, primitive));
                                }
                            }
                            None => self.push_into_frame(primitive, order),
                        }
                    }
                }
            }
        }
    }

    /// QUILL: what the finished scene draws in `region` (whole device
    /// pixels), as a scene `density` texels per device pixel with (0, 0) at
    /// the region's top left. `None` when the region holds a native view or
    /// glass, which a texture can't hold.
    pub(crate) fn region_snapshot(&self, region: Bounds<ScaledPixels>, density: f32) -> Option<Scene> {
        let reaches = |bounds: &Bounds<ScaledPixels>, mask: &ContentMask<ScaledPixels>| {
            let clipped = bounds.intersect(&mask.bounds);
            !clipped.is_empty() && clipped.intersects(&region)
        };
        if self
            .native_views
            .iter()
            .any(|view| reaches(&view.bounds, &view.content_mask))
            || self
                .lenses
                .iter()
                .any(|lens| reaches(&lens.bounds, &lens.content_mask))
        {
            return None;
        }
        let space = slot_space(region.origin, density, region.size);
        let mut scene = Scene::default();
        let push = |scene: &mut Scene, primitive: Primitive| {
            let order = primitive.order();
            if let Some(mapped) = map_primitive(&space, &primitive) {
                scene.push_into_frame(mapped, order);
            }
        };
        for shadow in &self.shadows {
            let reach = ScaledPixels(shadow.blur_radius.0 * 3.0);
            if reaches(&shadow.bounds.dilate(reach), &shadow.content_mask) {
                push(&mut scene, Primitive::Shadow(*shadow));
            }
        }
        for quad in &self.quads {
            if reaches(&quad.bounds, &quad.content_mask) {
                push(&mut scene, Primitive::Quad(*quad));
            }
        }
        for path in &self.paths {
            if reaches(&path.bounds, &path.paint.content_mask) {
                push(&mut scene, Primitive::Path(path.clone()));
            }
        }
        for underline in &self.underlines {
            if reaches(&underline.bounds, &underline.content_mask) {
                push(&mut scene, Primitive::Underline(*underline));
            }
        }
        // A transformed sprite may reach anywhere in its mask.
        for sprite in &self.monochrome_sprites {
            if reaches(&sprite.content_mask.bounds, &sprite.content_mask) {
                push(&mut scene, Primitive::MonochromeSprite(*sprite));
            }
        }
        for sprite in &self.subpixel_sprites {
            if reaches(&sprite.content_mask.bounds, &sprite.content_mask) {
                push(&mut scene, Primitive::SubpixelSprite(*sprite));
            }
        }
        for sprite in &self.polychrome_sprites {
            if reaches(&sprite.bounds, &sprite.content_mask) {
                push(&mut scene, Primitive::PolychromeSprite(*sprite));
            }
        }
        for surface in &self.surfaces {
            if reaches(&surface.bounds, &surface.content_mask) {
                push(&mut scene, Primitive::Surface(surface.clone()));
            }
        }
        for quad in &self.snapshots {
            if reaches(&quad.bounds, &quad.content_mask) {
                push(&mut scene, Primitive::Snapshot(quad.clone()));
            }
        }
        scene.finish();
        Some(scene)
    }
}

fn slot_space(origin: Point<ScaledPixels>, density: f32, size: Size<ScaledPixels>) -> SlotSpace {
    SlotSpace {
        origin,
        density,
        size: Size {
            width: ScaledPixels((size.width.0 * density).ceil()),
            height: ScaledPixels((size.height.0 * density).ceil()),
        },
    }
}

/// A capture's primitives in its texture's texels, in the order the frame
/// drew them.
fn captured_scene(target: &CaptureTarget, primitives: Vec<(usize, Primitive)>) -> Scene {
    let size = Size {
        width: ScaledPixels(target.size.width.0 as f32),
        height: ScaledPixels(target.size.height.0 as f32),
    };
    let space = slot_space(target.origin, 1.0, size);
    let mut scene = Scene::default();
    for (_, primitive) in primitives {
        let order = primitive.order();
        if let Some(mapped) = map_primitive(&space, &primitive) {
            scene.push_into_frame(mapped, order);
        }
    }
    scene.finish();
    scene
}

/// `primitive` in `space`. An animated or inked label keeps its pose and
/// colour of this frame; glass and native views have no place in a texture.
fn map_primitive(space: &SlotSpace, primitive: &Primitive) -> Option<Primitive> {
    Some(match primitive {
        Primitive::Shadow(shadow) => Primitive::Shadow(space.shadow(shadow)),
        Primitive::Quad(quad) => Primitive::Quad(space.quad(quad)),
        Primitive::Path(path) => Primitive::Path(space.path(path)),
        Primitive::Underline(underline) => Primitive::Underline(space.underline(underline)),
        Primitive::MonochromeSprite(sprite) => Primitive::MonochromeSprite(space.monochrome(sprite)),
        Primitive::SubpixelSprite(sprite) => Primitive::SubpixelSprite(space.subpixel(sprite)),
        Primitive::PolychromeSprite(sprite) => Primitive::PolychromeSprite(space.polychrome(sprite)),
        Primitive::Surface(surface) => Primitive::Surface(PaintSurface {
            bounds: space.bounds(surface.bounds),
            content_mask: space.mask(surface.content_mask),
            ..surface.clone()
        }),
        Primitive::Snapshot(quad) => Primitive::Snapshot(SnapshotQuad {
            bounds: space.bounds(quad.bounds),
            content_mask: space.mask(quad.content_mask),
            ..quad.clone()
        }),
        Primitive::NativeView(_) | Primitive::Lens(_) => return None,
    })
}
