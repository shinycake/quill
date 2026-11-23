// Added by the Quill project (2026). See third_party/gpui-pre/QUILL-CHANGES.md.
//! QUILL (backdrop glass, stage 5): one plan per frame for every lens in it, shared by the
//! renderers. See third_party/gpui-pre/QUILL-CHANGES.md.
//!
//! - Every lens of the frame gets its own atlas slot (glass over glass
//!   included), so a slot outlives the batch that filled it.
//! - [`LensReuse`] keeps last frame's slots and what lay under them. A lens
//!   whose padded copy from last frame still holds its footprint, over
//!   primitives that are the same as when they were copied, reuses the slot:
//!   no copy and no blur, only the lens draw. Any change under the copied
//!   region (a primitive added, removed, moved, recoloured or reordered, a
//!   video frame, an atlas tile freed) copies again. Glass under it counts
//!   as what it draws: the lens, the region it read and what lay there; a
//!   label inked from glass counts with what that glass holds.
//! - Labels painted with [`LensInk`] pick their colour from the slot their
//!   lens reads ([`lens_ink_instances`]).

use std::hash::{DefaultHasher, Hasher};

use crate::{
    BackdropLens, Bounds, DrawOrder, Hsla, LensCopy, LensPacking, LensRegion, MonochromeSprite,
    PolychromeSprite, Quad, Rgba, ScaledPixels, Scene, Shadow, SubpixelSprite,
    TransformationMatrix, Underline,
};

/// Device pixels a copy reaches past its lens on every side, so a lens that
/// moves or swells a little still lies inside last frame's copy.
pub const LENS_COPY_PAD: u32 = 24;

/// What a renderer does for one slot this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LensWork {
    /// Copy the region out of the target and blur it.
    Copy,
    /// Last frame's slot still holds the region as the frame has it now.
    Reuse,
    /// Copy it from the re-rendered backdrop (`Scene::sharp_backdrops`)
    /// and blur it.
    Render,
}

/// One lens's slot this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LensSlotPlan {
    /// Where the lens reads in the target and where its slot is.
    pub copy: LensCopy,
    /// What fills the slot.
    pub work: LensWork,
    /// What lies under the region, `None` when it can't be told (so the
    /// slot is never reused).
    under: Option<u64>,
}

/// Every lens's slot this frame, in `Scene::lenses` order, and the atlas
/// size they need.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LensFrame {
    /// One per lens of the scene.
    pub slots: Vec<LensSlotPlan>,
    /// The atlas width the slots need.
    pub width: u32,
    /// The atlas height the slots need.
    pub height: u32,
}

impl LensFrame {
    /// The atlas was made anew: nothing in it can be reused.
    pub fn copy_everything(&mut self) {
        for slot in &mut self.slots {
            if slot.work == LensWork::Reuse {
                slot.work = LensWork::Copy;
            }
        }
    }

    /// The slots of lenses `range` of the scene.
    pub fn batch(&self, range: std::ops::Range<usize>) -> &[LensSlotPlan] {
        self.slots.get(range).unwrap_or_default()
    }

    /// How many slots copy, reuse and render this frame.
    pub fn counts(&self) -> (usize, usize, usize) {
        self.slots
            .iter()
            .fold((0, 0, 0), |(c, u, r), slot| match slot.work {
                LensWork::Copy => (c + 1, u, r),
                LensWork::Reuse => (c, u + 1, r),
                LensWork::Render => (c, u, r + 1),
            })
    }
}

/// What a frame's slots depend on besides the scene: when any of it
/// changes, nothing is reused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LensFrameContext {
    /// The target's size, device pixels.
    pub target: (u32, u32),
    /// Bumped by the renderer whenever it makes a new atlas or drops it.
    pub atlas: u64,
    /// Bumped by the sprite atlas whenever it frees a tile (whose place a
    /// different picture may take).
    pub sprites: u64,
}

/// A slot kept from the last frame that drew glass.
#[derive(Clone, Copy, Debug)]
struct KeptSlot {
    copy: LensCopy,
    under: u64,
}

/// What a renderer keeps between frames so a lens over unchanged content
/// skips its copy and blur. See the module docs.
#[derive(Debug, Default)]
pub struct LensReuse {
    kept: Vec<KeptSlot>,
    context: Option<LensFrameContext>,
}

impl LensReuse {
    /// Plans the frame's slots; `None` when it has no lenses or they don't
    /// fit in an atlas of `max_side`. `renders` says whether the renderer
    /// can draw re-rendered backdrops; when it can't, those lenses copy.
    pub fn plan(
        &self,
        scene: &Scene,
        context: LensFrameContext,
        max_side: u32,
        renders: bool,
    ) -> Option<LensFrame> {
        if scene.lenses.is_empty() {
            return None;
        }
        let target = context.target;
        let kept: &[KeptSlot] = if self.context == Some(context) {
            &self.kept
        } else {
            &[]
        };
        let mut claimed = vec![false; kept.len()];
        let mut regions = Vec::with_capacity(scene.lenses.len());
        let mut plans = Vec::with_capacity(scene.lenses.len());
        // What each lens planned so far draws, for the lenses over it.
        let mut drawn: Vec<Option<u64>> = Vec::with_capacity(scene.lenses.len());
        for (index, lens) in scene.lenses.iter().enumerate() {
            let footprint = LensRegion::clipped(lens.bounds, target);
            if renders && let Some(sharp) = scene.sharp_backdrops.iter().find(|s| s.lens == index) {
                regions.push(LensRegion {
                    density: sharp.density,
                    ..LensRegion::clipped(sharp.region, target)
                });
                plans.push((LensWork::Render, None, None));
                drawn.push(None);
                continue;
            }
            let reuse = kept.iter().enumerate().find_map(|(k, slot)| {
                let region = LensRegion {
                    from: slot.copy.from,
                    size: slot.copy.size,
                    density: 1,
                };
                let free = !claimed.get(k).copied().unwrap_or(true);
                (free && slot.copy.density == 1 && region.contains(&footprint))
                    .then(|| {
                        let under = planned_key(scene, region.bounds(), lens.order, &drawn)?;
                        (under == slot.under).then_some((k, region, under))
                    })
                    .flatten()
            });
            match reuse {
                Some((k, region, under)) => {
                    if let Some(claim) = claimed.get_mut(k) {
                        *claim = true;
                    }
                    drawn.push(Some(lens_drawn_key(lens, &region, under)));
                    regions.push(region);
                    plans.push((LensWork::Reuse, Some(k), Some(under)));
                }
                None => {
                    let region = padded(lens.bounds, target);
                    let under = planned_key(scene, region.bounds(), lens.order, &drawn);
                    drawn.push(under.map(|under| lens_drawn_key(lens, &region, under)));
                    regions.push(region);
                    plans.push((LensWork::Copy, None, under));
                }
            }
        }
        let packing = LensPacking::pack_regions(&regions, target, max_side)?;
        let slots = packing
            .copies
            .iter()
            .zip(plans)
            .map(|(copy, (work, kept_index, under))| {
                let same_slot = kept_index
                    .and_then(|k| kept.get(k))
                    .is_some_and(|slot| slot.copy == *copy);
                let work = match work {
                    LensWork::Reuse if !same_slot => LensWork::Copy,
                    work => work,
                };
                LensSlotPlan {
                    copy: *copy,
                    work,
                    under,
                }
            })
            .collect();
        Some(LensFrame {
            slots,
            width: packing.width,
            height: packing.height,
        })
    }

    /// The frame was drawn as planned: its slots hold what they planned
    /// to, for the next frame to reuse.
    pub fn keep(&mut self, frame: &LensFrame, context: LensFrameContext) {
        self.kept.clear();
        self.kept.extend(frame.slots.iter().filter_map(|slot| {
            let under = slot.under.filter(|_| slot.work != LensWork::Render)?;
            Some(KeptSlot {
                copy: slot.copy,
                under,
            })
        }));
        self.context = Some(context);
    }

    /// Nothing in the atlas can be trusted (a frame drew flat or failed,
    /// or the atlas went away).
    pub fn forget(&mut self) {
        self.kept.clear();
        self.context = None;
    }
}

/// A lens's footprint with [`LENS_COPY_PAD`] around it, clipped to the
/// target.
fn padded(bounds: Bounds<ScaledPixels>, target: (u32, u32)) -> LensRegion {
    let pad = ScaledPixels(LENS_COPY_PAD as f32);
    LensRegion::clipped(bounds.dilate(pad), target)
}

/// The scene's GPU instance types, which are `#[repr(C)]` and made of
/// 4-byte fields with no padding (the renderers upload them as bytes).
trait GpuInstance: Copy {
    fn order_mut(&mut self) -> &mut DrawOrder;
}

macro_rules! gpu_instance {
    ($($t:ty),*) => {$(
        impl GpuInstance for $t {
            fn order_mut(&mut self) -> &mut DrawOrder {
                &mut self.order
            }
        }
    )*};
}
gpu_instance!(
    Quad,
    Shadow,
    Underline,
    MonochromeSprite,
    SubpixelSprite,
    PolychromeSprite,
    BackdropLens
);

/// Hashes everything about `value` but its draw order, which moves with
/// unrelated changes elsewhere in the frame.
fn hash_instance<T: GpuInstance>(value: &T) -> u64 {
    let mut value = *value;
    *value.order_mut() = 0;
    let mut hasher = DefaultHasher::new();
    // SAFETY: `T` is one of the scene's `#[repr(C)]` instance types listed
    // in `gpu_instance!`, made of 4-byte fields with no padding, so every
    // byte of it is initialised; the slice lives as long as `value`.
    let bytes = unsafe {
        std::slice::from_raw_parts((&value as *const T).cast::<u8>(), std::mem::size_of::<T>())
    };
    hasher.write(bytes);
    hasher.finish()
}

fn covers(coverage: Bounds<ScaledPixels>, region: &Bounds<ScaledPixels>) -> bool {
    !coverage.is_empty() && coverage.intersects(region)
}

fn sprite_coverage(
    bounds: Bounds<ScaledPixels>,
    mask: Bounds<ScaledPixels>,
    transformation: &TransformationMatrix,
) -> Bounds<ScaledPixels> {
    if *transformation == TransformationMatrix::unit() {
        bounds.intersect(&mask)
    } else {
        // Rotated or scaled: anywhere inside its mask.
        mask
    }
}

/// Draw-order rank of each primitive kind, as `PrimitiveKind` sorts them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Shadow,
    Quad,
    Path,
    Underline,
    MonochromeSprite,
    SubpixelSprite,
    PolychromeSprite,
    /// Glass under glass (only where the lens under it is known).
    Lens,
}

/// One primitive drawn before `order` whose pixels reach `region`, with
/// what to sort it by.
struct Under {
    order: DrawOrder,
    rank: Rank,
    index: usize,
    hash: u64,
}

/// One primitive under a lens: its kind and its index in that kind's list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnderItem {
    Shadow(usize),
    Quad(usize),
    Path(usize),
    Underline(usize),
    MonochromeSprite(usize),
    SubpixelSprite(usize),
    PolychromeSprite(usize),
    Lens(usize),
}

/// The primitives the frame draws before a lens at `before` whose pixels
/// reach `region`, in draw order; `None` when what lies there can't be
/// told from the scene (a surface, whose picture changes on its own, or
/// glass under glass, which bends pixels from outside the region).
pub(crate) fn primitives_under_ranked(
    scene: &Scene,
    region: Bounds<ScaledPixels>,
    before: DrawOrder,
) -> Option<Vec<UnderItem>> {
    let under = collect_under(scene, region, before, None)?;
    Some(
        under
            .into_iter()
            .map(|u| match u.rank {
                Rank::Shadow => UnderItem::Shadow(u.index),
                Rank::Quad => UnderItem::Quad(u.index),
                Rank::Path => UnderItem::Path(u.index),
                Rank::Underline => UnderItem::Underline(u.index),
                Rank::MonochromeSprite => UnderItem::MonochromeSprite(u.index),
                Rank::SubpixelSprite => UnderItem::SubpixelSprite(u.index),
                Rank::PolychromeSprite => UnderItem::PolychromeSprite(u.index),
                // `collect_under` without `drawn` refuses glass under glass.
                Rank::Lens => UnderItem::Lens(u.index),
            })
            .collect(),
    )
}

/// `drawn` is what the lenses planned so far draw (`None` where it can't
/// be told). Without it, glass under the region, and labels inked from
/// glass, can't be told.
fn collect_under(
    scene: &Scene,
    region: Bounds<ScaledPixels>,
    before: DrawOrder,
    drawn: Option<&[Option<u64>]>,
) -> Option<Vec<Under>> {
    // Other kinds at the lens's own order draw before it; lenses at it are
    // its batch, copied at the same moment.
    let drawn_before = |order: DrawOrder| order <= before;
    if scene.surfaces.iter().any(|s| {
        drawn_before(s.order) && covers(s.bounds.intersect(&s.content_mask.bounds), &region)
    }) {
        return None;
    }
    let mut lenses = Vec::new();
    for (index, lens) in scene.lenses.iter().enumerate() {
        if lens.order < before && covers(lens.bounds.intersect(&lens.content_mask.bounds), &region)
        {
            // Glass under glass draws what its own slot holds: known when
            // that lens was planned from a known backdrop.
            lenses.push((lens.order, index, drawn?.get(index).copied().flatten()?));
        }
    }
    let mut under = Vec::new();
    let mut push = |order: DrawOrder, rank: Rank, index: usize, hash: u64| {
        under.push(Under {
            order,
            rank,
            index,
            hash,
        })
    };
    for (order, index, hash) in lenses {
        push(order, Rank::Lens, index, hash);
    }
    for (index, shadow) in scene.shadows.iter().enumerate() {
        let reach = ScaledPixels(shadow.blur_radius.0 * 3.0);
        let coverage = shadow
            .bounds
            .dilate(reach)
            .intersect(&shadow.content_mask.bounds);
        if drawn_before(shadow.order) && covers(coverage, &region) {
            push(shadow.order, Rank::Shadow, index, hash_instance(shadow));
        }
    }
    for (index, quad) in scene.quads.iter().enumerate() {
        let coverage = quad.bounds.intersect(&quad.content_mask.bounds);
        if drawn_before(quad.order) && covers(coverage, &region) {
            push(quad.order, Rank::Quad, index, hash_instance(quad));
        }
    }
    for (index, path) in scene.paths.iter().enumerate() {
        let coverage = path.bounds.intersect(&path.content_mask.bounds);
        if drawn_before(path.order) && covers(coverage, &region) {
            push(path.order, Rank::Path, index, hash_path(path));
        }
    }
    for (index, underline) in scene.underlines.iter().enumerate() {
        let coverage = underline.bounds.intersect(&underline.content_mask.bounds);
        if drawn_before(underline.order) && covers(coverage, &region) {
            push(
                underline.order,
                Rank::Underline,
                index,
                hash_instance(underline),
            );
        }
    }
    for (index, sprite) in scene.monochrome_sprites.iter().enumerate() {
        let coverage = sprite_coverage(
            sprite.bounds,
            sprite.content_mask.bounds,
            &sprite.transformation,
        );
        if drawn_before(sprite.order) && covers(coverage, &region) {
            // A label's ink is chosen on the GPU from the slot of the glass
            // under it, so the same sprite can draw differently: hash its
            // ink and what that glass holds.
            let ink = ink_key(scene, sprite, drawn)?;
            let plain = MonochromeSprite { pad: 0, ..*sprite };
            push(
                sprite.order,
                Rank::MonochromeSprite,
                index,
                hash_instance(&plain) ^ ink,
            );
        }
    }
    for (index, sprite) in scene.subpixel_sprites.iter().enumerate() {
        let coverage = sprite_coverage(
            sprite.bounds,
            sprite.content_mask.bounds,
            &sprite.transformation,
        );
        if drawn_before(sprite.order) && covers(coverage, &region) {
            push(
                sprite.order,
                Rank::SubpixelSprite,
                index,
                hash_instance(sprite),
            );
        }
    }
    for (index, sprite) in scene.polychrome_sprites.iter().enumerate() {
        let coverage = sprite.bounds.intersect(&sprite.content_mask.bounds);
        if drawn_before(sprite.order) && covers(coverage, &region) {
            push(
                sprite.order,
                Rank::PolychromeSprite,
                index,
                hash_instance(sprite),
            );
        }
    }
    under.sort_by_key(|u| (u.order, u.rank, u.index));
    Some(under)
}

fn hash_path(path: &crate::Path<ScaledPixels>) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut f = |v: f32| hasher.write_u32(v.to_bits());
    let bounds = |b: &Bounds<ScaledPixels>, f: &mut dyn FnMut(f32)| {
        f(b.origin.x.0);
        f(b.origin.y.0);
        f(b.size.width.0);
        f(b.size.height.0);
    };
    bounds(&path.bounds, &mut f);
    bounds(&path.content_mask.bounds, &mut f);
    for vertex in &path.vertices {
        f(vertex.xy_position.x.0);
        f(vertex.xy_position.y.0);
        f(vertex.st_position.x);
        f(vertex.st_position.y);
        bounds(&vertex.content_mask.bounds, &mut f);
    }
    let color = path.color;
    let mut hasher2 = DefaultHasher::new();
    hasher2.write_u64(hasher.finish());
    hasher2.write(&background_bytes(&color));
    hasher2.finish()
}

fn background_bytes(color: &crate::Background) -> Vec<u8> {
    // SAFETY: `Background` is `#[repr(C)]` and made of 4-byte fields with an
    // explicit pad, so every byte is initialised; it is uploaded as bytes
    // inside `Quad`.
    unsafe {
        std::slice::from_raw_parts(
            (color as *const crate::Background).cast::<u8>(),
            std::mem::size_of::<crate::Background>(),
        )
    }
    .to_vec()
}

/// A key for what the frame has drawn into `region` when a lens at `before`
/// copies it: equal keys mean equal pixels there. `None` when it can't be
/// told (see [`primitives_under`]).
pub fn backdrop_key(scene: &Scene, region: Bounds<ScaledPixels>, before: DrawOrder) -> Option<u64> {
    let under = collect_under(scene, region, before, None)?;
    Some(hash_under(under))
}

/// [`backdrop_key`] while planning a frame: glass under the region and
/// labels inked from glass count, by what the lenses planned so far
/// (`drawn`, in `Scene::lenses` order) draw.
fn planned_key(
    scene: &Scene,
    region: Bounds<ScaledPixels>,
    before: DrawOrder,
    drawn: &[Option<u64>],
) -> Option<u64> {
    Some(hash_under(collect_under(
        scene,
        region,
        before,
        Some(drawn),
    )?))
}

/// A key for what `lens` draws: itself, the region it reads and what
/// lies there.
fn lens_drawn_key(lens: &BackdropLens, region: &LensRegion, under: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write_u64(hash_instance(lens));
    for v in [
        region.from.0,
        region.from.1,
        region.size.0,
        region.size.1,
        region.density,
    ] {
        hasher.write_u32(v);
    }
    hasher.write_u64(under);
    hasher.finish()
}

/// A sprite's ink, and while planning, what the glass it reads holds;
/// `None` when that can't be told.
fn ink_key(scene: &Scene, sprite: &MonochromeSprite, drawn: Option<&[Option<u64>]>) -> Option<u64> {
    let Some(entry) = sprite
        .pad
        .checked_sub(1)
        .and_then(|i| scene.inks.get(i as usize))
    else {
        return Some(0);
    };
    // Outside planning (a re-rendered backdrop), the ink is what it is.
    let Some(drawn) = drawn else {
        return Some(scene.ink_hash(sprite));
    };
    let glass = match usize::try_from(entry.lens) {
        Ok(lens) if lens < scene.lenses.len() => drawn.get(lens).copied().flatten()?,
        // An ink with no lens takes its flat colour.
        _ => 0,
    };
    let mut hasher = DefaultHasher::new();
    hasher.write_u64(scene.ink_hash(sprite));
    hasher.write_u64(glass);
    Some(hasher.finish())
}

fn hash_under(under: Vec<Under>) -> u64 {
    let mut hasher = DefaultHasher::new();
    hasher.write_usize(under.len());
    for u in under {
        hasher.write_u8(u.rank as u8);
        hasher.write_u64(u.hash);
    }
    hasher.finish()
}

/// QUILL (glass stage 5): label colours that follow the glass under them. The
/// renderer picks `on_light` where the glass under a glyph is light and
/// `on_dark` where it is dark, from the slot the lens reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensInk {
    /// The ink over light glass (usually dark).
    pub on_light: Hsla,
    /// The ink over dark glass (usually light).
    pub on_dark: Hsla,
}

/// One inked label's lens: `lens` indexes `Scene::lenses`, or
/// [`LensInkEntry::NO_LENS`] when no glass lies under the label.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LensInkEntry {
    /// The lens under the label.
    pub lens: u32,
    /// Its colours.
    pub ink: LensInk,
}

impl LensInkEntry {
    /// No glass under the label: it draws in its own colour.
    pub const NO_LENS: u32 = u32::MAX;
}

/// What a sprite shader needs to ink one label: where its lens's slot is
/// and how the glass colours it. Device pixels and atlas texels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
#[allow(missing_docs)]
pub struct LensInkInstance {
    pub copy_x: f32,
    pub copy_y: f32,
    pub copy_w: f32,
    pub copy_h: f32,
    pub slot_x: f32,
    pub slot_y: f32,
    pub density: f32,
    /// 1 = sample the backdrop from the atlas; 0 = only the glass colour.
    pub sampled: f32,
    /// Luminance of the glass's own colour.
    pub glass_luma: f32,
    /// How much of the glass's colour covers the backdrop (1 = all).
    pub cover: f32,
    /// 0 = no lens: the label keeps its own colour.
    pub active: f32,
    pub pad: f32,
    pub on_light: Hsla,
    pub on_dark: Hsla,
}

/// sRGB luminance, the source kit's measure for light and dark glass.
pub fn lens_luma(color: Hsla) -> f32 {
    let c = Rgba::from(color);
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b
}

/// The shader-side record of each `Scene::inks` entry, from this frame's
/// plan (`None` when the glass draws flat: labels then follow the glass's
/// flat colour).
pub fn lens_ink_instances(scene: &Scene, frame: Option<&LensFrame>) -> Vec<LensInkInstance> {
    scene
        .inks
        .iter()
        .map(|entry| {
            let lens = usize::try_from(entry.lens)
                .ok()
                .and_then(|i| scene.lenses.get(i).map(|lens| (i, lens)));
            let Some((index, lens)) = lens else {
                return LensInkInstance {
                    on_light: entry.ink.on_light,
                    on_dark: entry.ink.on_dark,
                    ..Default::default()
                };
            };
            let cover = if lens.material == 1 {
                1.0
            } else {
                lens.frost.clamp(0.0, 1.0)
            };
            let mut instance = LensInkInstance {
                glass_luma: lens_luma(lens.color),
                cover,
                active: 1.0,
                density: 1.0,
                on_light: entry.ink.on_light,
                on_dark: entry.ink.on_dark,
                ..Default::default()
            };
            if let Some(slot) = frame.and_then(|f| f.slots.get(index)) {
                instance.copy_x = slot.copy.from.0 as f32;
                instance.copy_y = slot.copy.from.1 as f32;
                instance.copy_w = slot.copy.size.0 as f32;
                instance.copy_h = slot.copy.size.1 as f32;
                instance.slot_x = slot.copy.to.0 as f32;
                instance.slot_y = slot.copy.to.1 as f32;
                instance.density = slot.copy.density.max(1) as f32;
                instance.sampled = if slot.copy.size.0 > 0 && slot.copy.size.1 > 0 {
                    1.0
                } else {
                    0.0
                };
            }
            instance
        })
        .collect()
}
