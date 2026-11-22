//! QUILL: backdrop glass on Metal (`PrimitiveBatch::Lenses`).
//!
//! A lens batch ends the frame's render pass, copies each lens's footprint
//! out of the target into a slot of a pooled atlas, fills the atlas's mip
//! levels with dual-Kawase downsamples, reopens the pass over what was
//! drawn and draws the lenses from the atlas (`lens_fragment` in
//! shaders.metal). A target that can't be read (a framebuffer-only
//! drawable), a missing pipeline or an atlas that can't be made draws the
//! batch as its fallback quads instead. See
//! third_party/gpui-pre-apple/QUILL-CHANGES.md ("Backdrop glass").

use super::{
    InstanceBinding, InstanceBindings, InstanceBufferWriter, MetalRenderer,
    new_command_encoder_for_texture,
};
use anyhow::{Context as _, Result};
use gpui::{BackdropLens, DevicePixels, Size, size};
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_foundation::{NSRange, NSString};
use objc2_metal::{
    MTLBlitCommandEncoder, MTLCommandBuffer, MTLCommandEncoder, MTLDevice, MTLLibrary,
    MTLLoadAction, MTLOrigin, MTLPixelFormat, MTLPrimitiveType, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLSize,
    MTLStorageMode, MTLStoreAction, MTLTexture, MTLTextureDescriptor, MTLTextureType,
    MTLTextureUsage,
};
use std::{mem, ops::Range, ptr};

/// Mip levels of the atlas: level k is blurred over about 2^k pixels.
const LEVELS: usize = 5;
/// Slots start on multiples of this, so each level's slot starts on a
/// whole texel.
const ALIGN: u32 = 1 << (LEVELS - 1);
/// Room between slots; no tap crosses it (the shaders clamp to the slot).
const GUTTER: u32 = 16;
/// The largest atlas side every Mac GPU supports.
const MAX_SIDE: u32 = 16384;

/// Where one lens's backdrop lies in the target and in the atlas, device
/// pixels. `lens_fragment` reads it next to its `BackdropLens`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct LensSlot {
    pub copy_x: f32,
    pub copy_y: f32,
    pub copy_w: f32,
    pub copy_h: f32,
    pub slot_x: f32,
    pub slot_y: f32,
    pub pad_x: f32,
    pub pad_y: f32,
}

/// One slot of one downsample pass: the texels it writes in the level
/// being filled, and the texels of the level above it may read.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct LensDownsampleSlot {
    pub dst_x: f32,
    pub dst_y: f32,
    pub dst_w: f32,
    pub dst_h: f32,
    pub src_x0: f32,
    pub src_y0: f32,
    pub src_x1: f32,
    pub src_y1: f32,
}

#[repr(C)]
pub enum LensInputIndex {
    Vertices = 0,
    Lenses = 1,
    ViewportSize = 2,
    Slots = 3,
    AtlasSize = 4,
    Atlas = 5,
}

#[repr(C)]
pub enum LensDownsampleIndex {
    Vertices = 0,
    Slots = 1,
    TargetSize = 2,
    Source = 3,
}

/// One footprint to copy: from the target at `from` to the atlas at `to`,
/// `size` pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Copy {
    from: (u32, u32),
    to: (u32, u32),
    size: (u32, u32),
}

/// Where a batch's footprints go in the atlas, and the atlas size they need.
#[derive(Debug, PartialEq, Eq)]
struct Packing {
    copies: Vec<Copy>,
    width: u32,
    height: u32,
}

fn align_up(value: u32) -> u32 {
    value.div_ceil(ALIGN).saturating_mul(ALIGN)
}

/// Shelves the footprints of `lenses`, clipped to the target, into an
/// atlas; `None` when they don't fit in [`MAX_SIDE`].
fn pack(lenses: &[BackdropLens], target: (u32, u32)) -> Option<Packing> {
    let clipped: Vec<((u32, u32), (u32, u32))> = lenses
        .iter()
        .map(|lens| {
            let bounds = lens.bounds;
            let clamp_x = |v: f32| v.clamp(0.0, target.0 as f32);
            let clamp_y = |v: f32| v.clamp(0.0, target.1 as f32);
            let x0 = clamp_x(bounds.origin.x.0.floor());
            let y0 = clamp_y(bounds.origin.y.0.floor());
            let x1 = clamp_x((bounds.origin.x.0 + bounds.size.width.0).ceil());
            let y1 = clamp_y((bounds.origin.y.0 + bounds.size.height.0).ceil());
            let size = ((x1 - x0).max(0.0) as u32, (y1 - y0).max(0.0) as u32);
            ((x0 as u32, y0 as u32), size)
        })
        .collect();
    let widest = clipped
        .iter()
        .map(|(_, (w, _))| align_up(*w))
        .max()
        .unwrap_or(0);
    let one_row: u32 = clipped
        .iter()
        .map(|(_, (w, _))| align_up(*w).saturating_add(GUTTER))
        .fold(0, u32::saturating_add);
    let width = one_row
        .min(align_up(target.0).saturating_add(GUTTER))
        .max(widest)
        .max(ALIGN);
    if width > MAX_SIDE {
        return None;
    }
    let (mut x, mut y, mut row) = (0u32, 0u32, 0u32);
    let mut copies = Vec::with_capacity(clipped.len());
    for (from, size) in clipped {
        let slot_w = align_up(size.0);
        if x > 0 && x.saturating_add(slot_w) > width {
            x = 0;
            y = y.saturating_add(row).saturating_add(GUTTER);
            row = 0;
        }
        copies.push(Copy {
            from,
            to: (x, y),
            size,
        });
        x = x.saturating_add(slot_w).saturating_add(GUTTER);
        row = row.max(align_up(size.1));
    }
    let height = y.saturating_add(row).max(ALIGN);
    (height <= MAX_SIDE).then_some(Packing {
        copies,
        width,
        height,
    })
}

/// The pooled atlas: a mip-chained texture in the target's format, and a
/// view of each level (a pass renders one level while it samples the one
/// above).
pub(super) struct LensAtlas {
    texture: Retained<ProtocolObject<dyn MTLTexture>>,
    levels: Vec<Retained<ProtocolObject<dyn MTLTexture>>>,
    format: MTLPixelFormat,
}

impl LensAtlas {
    fn new(
        device: &ProtocolObject<dyn MTLDevice>,
        format: MTLPixelFormat,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        // Rounded up, so a lens that grows a little keeps its atlas.
        let width = width.next_multiple_of(256).min(MAX_SIDE) as usize;
        let height = height.next_multiple_of(256).min(MAX_SIDE) as usize;
        let descriptor = MTLTextureDescriptor::new();
        // SAFETY: positive sizes within the device limit and a mip count
        // the sizes allow (each side is at least 256).
        unsafe {
            descriptor.setWidth(width);
            descriptor.setHeight(height);
            descriptor.setMipmapLevelCount(LEVELS);
        }
        descriptor.setPixelFormat(format);
        descriptor.setStorageMode(MTLStorageMode::Private);
        descriptor.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        let texture = device
            .newTextureWithDescriptor(&descriptor)
            .context("allocating the lens atlas")?;
        let levels = (0..LEVELS)
            .map(|level| {
                // SAFETY: the level is below the texture's mip count and
                // slice 0 is its only slice.
                unsafe {
                    texture.newTextureViewWithPixelFormat_textureType_levels_slices(
                        format,
                        MTLTextureType::Type2D,
                        NSRange {
                            location: level,
                            length: 1,
                        },
                        NSRange {
                            location: 0,
                            length: 1,
                        },
                    )
                }
                .context("viewing a lens atlas level")
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            texture,
            levels,
            format,
        })
    }

    fn fits(&self, format: MTLPixelFormat, width: u32, height: u32) -> bool {
        self.format == format
            && self.texture.width() >= width as usize
            && self.texture.height() >= height as usize
    }

    fn size(&self) -> Size<DevicePixels> {
        size(
            DevicePixels(self.texture.width() as i32),
            DevicePixels(self.texture.height() as i32),
        )
    }
}

#[derive(Clone)]
struct LensPipelines {
    lens: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    downsample: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
}

impl LensPipelines {
    fn build(
        device: &ProtocolObject<dyn MTLDevice>,
        library: &ProtocolObject<dyn MTLLibrary>,
        format: MTLPixelFormat,
    ) -> Result<Self> {
        Ok(Self {
            lens: super::build_pipeline_state(
                device,
                library,
                "lenses",
                "lens_vertex",
                "lens_fragment",
                format,
            )?,
            downsample: build_downsample_pipeline_state(device, library, format)?,
        })
    }
}

fn build_downsample_pipeline_state(
    device: &ProtocolObject<dyn MTLDevice>,
    library: &ProtocolObject<dyn MTLLibrary>,
    format: MTLPixelFormat,
) -> Result<Retained<ProtocolObject<dyn MTLRenderPipelineState>>> {
    let function = |name: &str| {
        library
            .newFunctionWithName(&NSString::from_str(name))
            .with_context(|| format!("locating function {name}"))
    };
    let descriptor = MTLRenderPipelineDescriptor::new();
    descriptor.setLabel(Some(&NSString::from_str("lens_downsample")));
    descriptor.setVertexFunction(Some(&*function("lens_downsample_vertex")?));
    descriptor.setFragmentFunction(Some(&*function("lens_downsample_fragment")?));
    // SAFETY: Index zero is within the pipeline's color attachment array.
    let color_attachment = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
    color_attachment.setPixelFormat(format);
    color_attachment.setBlendingEnabled(false);
    device
        .newRenderPipelineStateWithDescriptor_error(&descriptor)
        .context("creating lens_downsample render pipeline")
}

/// What drawing glass keeps between frames: the pipelines, built the first
/// time a lens is drawn in a format (`None`: they did not build, so that
/// format draws fallback quads without trying again), and the atlas.
#[derive(Default)]
pub(super) struct LensState {
    pipelines: Vec<(MTLPixelFormat, Option<LensPipelines>)>,
    atlas: Option<LensAtlas>,
}

impl LensState {
    /// Frees the atlas (the window went idle).
    pub(super) fn release(&mut self) {
        self.atlas = None;
    }

    fn pipelines(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        library: &ProtocolObject<dyn MTLLibrary>,
        format: MTLPixelFormat,
    ) -> Option<LensPipelines> {
        if let Some((_, built)) = self.pipelines.iter().find(|(known, _)| *known == format) {
            return built.clone();
        }
        let built = LensPipelines::build(device, library, format)
            .inspect_err(|error| log::error!("glass draws flat in {format:?}: {error:#}"))
            .ok();
        self.pipelines.push((format, built.clone()));
        built
    }

    fn atlas(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        format: MTLPixelFormat,
        width: u32,
        height: u32,
    ) -> Result<&LensAtlas> {
        if let Some(atlas) = self
            .atlas
            .take_if(|atlas| atlas.fits(format, width, height))
        {
            return Ok(self.atlas.insert(atlas));
        }
        // The old atlas goes first, so the two never live at once.
        self.atlas = None;
        let atlas = LensAtlas::new(device, format, width, height)?;
        Ok(self.atlas.insert(atlas))
    }
}

/// Everything one lens batch draws with, gathered before the pass ends.
struct LensPlan {
    pipelines: LensPipelines,
    atlas: Retained<ProtocolObject<dyn MTLTexture>>,
    levels: Vec<Retained<ProtocolObject<dyn MTLTexture>>>,
    atlas_size: Size<DevicePixels>,
    copies: Vec<Copy>,
    lenses: InstanceBinding,
    slots: InstanceBinding,
    /// Per level from 1: the slots that pass writes.
    downsample: Vec<InstanceBinding>,
    count: usize,
}

impl MetalRenderer {
    /// Draws one lens batch, glass when the target can be read, else its
    /// fallback quads. May end `command_encoder` and put a new one over
    /// the same target in its place.
    pub(super) fn draw_lenses(
        &mut self,
        lenses: &[BackdropLens],
        range: Range<usize>,
        instance_bindings: &InstanceBindings,
        writer: &mut InstanceBufferWriter,
        texture: &ProtocolObject<dyn MTLTexture>,
        viewport_size: Size<DevicePixels>,
        command_buffer: &ProtocolObject<dyn MTLCommandBuffer>,
        command_encoder: &mut Retained<ProtocolObject<dyn MTLRenderCommandEncoder>>,
    ) -> Result<()> {
        let plan = match self.plan_lenses(lenses, texture, writer) {
            Ok(plan) => plan,
            Err(error) => {
                log::error!("glass draws flat: {error:#}");
                None
            }
        };
        let Some(plan) = plan else {
            self.draw_quads(
                range,
                &instance_bindings.lens_fallbacks,
                viewport_size,
                command_encoder,
            );
            return Ok(());
        };
        command_encoder.endEncoding();
        let copied = self.copy_backdrop(&plan, texture, command_buffer);
        *command_encoder =
            new_command_encoder_for_texture(command_buffer, texture, viewport_size, None)?;
        match copied {
            Ok(()) => self.draw_glass(&plan, viewport_size, command_encoder),
            Err(error) => {
                log::error!("glass draws flat: {error:#}");
                self.draw_quads(
                    range,
                    &instance_bindings.lens_fallbacks,
                    viewport_size,
                    command_encoder,
                );
            }
        }
        Ok(())
    }

    /// The batch's plan, or `None` when it must draw flat: the target is
    /// framebuffer-only, or the pipelines or the atlas can't be had.
    fn plan_lenses(
        &mut self,
        lenses: &[BackdropLens],
        texture: &ProtocolObject<dyn MTLTexture>,
        writer: &mut InstanceBufferWriter,
    ) -> Result<Option<LensPlan>> {
        if lenses.is_empty() || texture.isFramebufferOnly() {
            return Ok(None);
        }
        let format = texture.pixelFormat();
        let Some(pipelines) = self.lens.pipelines(&self.device, &self.library, format) else {
            return Ok(None);
        };
        let target = (texture.width() as u32, texture.height() as u32);
        let Some(packing) = pack(lenses, target) else {
            return Ok(None);
        };
        let atlas = self
            .lens
            .atlas(&self.device, format, packing.width, packing.height)?;
        let (atlas_texture, levels, atlas_size) =
            (atlas.texture.clone(), atlas.levels.clone(), atlas.size());

        let slots: Vec<LensSlot> = packing
            .copies
            .iter()
            .map(|copy| LensSlot {
                copy_x: copy.from.0 as f32,
                copy_y: copy.from.1 as f32,
                copy_w: copy.size.0 as f32,
                copy_h: copy.size.1 as f32,
                slot_x: copy.to.0 as f32,
                slot_y: copy.to.1 as f32,
                pad_x: 0.0,
                pad_y: 0.0,
            })
            .collect();
        let downsample = (1..LEVELS)
            .map(|level| {
                let at = |v: u32, level: usize| v as f32 / (1u32 << level) as f32;
                let level_slots: Vec<LensDownsampleSlot> = packing
                    .copies
                    .iter()
                    .map(|copy| LensDownsampleSlot {
                        dst_x: at(copy.to.0, level),
                        dst_y: at(copy.to.1, level),
                        dst_w: at(copy.size.0, level).ceil(),
                        dst_h: at(copy.size.1, level).ceil(),
                        src_x0: at(copy.to.0, level - 1),
                        src_y0: at(copy.to.1, level - 1),
                        src_x1: at(copy.to.0, level - 1) + at(copy.size.0, level - 1).ceil(),
                        src_y1: at(copy.to.1, level - 1) + at(copy.size.1, level - 1).ceil(),
                    })
                    .collect();
                writer.write(&level_slots)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(LensPlan {
            pipelines,
            atlas: atlas_texture,
            levels,
            atlas_size,
            copies: packing.copies,
            lenses: writer.write(lenses)?,
            slots: writer.write(&slots)?,
            downsample,
            count: lenses.len(),
        }))
    }

    /// Copies the footprints into the atlas and fills its mip levels.
    fn copy_backdrop(
        &self,
        plan: &LensPlan,
        texture: &ProtocolObject<dyn MTLTexture>,
        command_buffer: &ProtocolObject<dyn MTLCommandBuffer>,
    ) -> Result<()> {
        let blit = command_buffer
            .blitCommandEncoder()
            .context("creating the lens copy encoder")?;
        for copy in plan
            .copies
            .iter()
            .filter(|copy| copy.size.0 > 0 && copy.size.1 > 0)
        {
            // SAFETY: both textures are live and retained by the command
            // buffer; the source region lies inside the target (`pack`
            // clipped it) and the destination inside the atlas (`pack`
            // sized it), and both share the target's pixel format.
            unsafe {
                blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                    texture,
                    0,
                    0,
                    MTLOrigin { x: copy.from.0 as usize, y: copy.from.1 as usize, z: 0 },
                    MTLSize { width: copy.size.0 as usize, height: copy.size.1 as usize, depth: 1 },
                    &plan.atlas,
                    0,
                    0,
                    MTLOrigin { x: copy.to.0 as usize, y: copy.to.1 as usize, z: 0 },
                );
            }
        }
        blit.endEncoding();

        for (level, slots) in (1..LEVELS).zip(&plan.downsample) {
            let (Some(target), Some(source)) = (plan.levels.get(level), plan.levels.get(level - 1))
            else {
                break;
            };
            let descriptor = MTLRenderPassDescriptor::new();
            // SAFETY: Index zero is within the render pass's color attachment array.
            let attachment = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
            attachment.setTexture(Some(target));
            // Only the slots are written and only the slots are read.
            attachment.setLoadAction(MTLLoadAction::DontCare);
            attachment.setStoreAction(MTLStoreAction::Store);
            let encoder = command_buffer
                .renderCommandEncoderWithDescriptor(&descriptor)
                .context("creating a lens blur encoder")?;
            let target_size = size(
                DevicePixels(target.width() as i32),
                DevicePixels(target.height() as i32),
            );
            encoder.setRenderPipelineState(&plan.pipelines.downsample);
            // SAFETY: six unit vertices and `count` downsample slots at an
            // aligned binding match the shader's slots; the size bytes are
            // copied; `source` is another level than the attachment.
            unsafe {
                encoder.setVertexBuffer_offset_atIndex(
                    Some(&self.unit_vertices),
                    0,
                    LensDownsampleIndex::Vertices as usize,
                );
                encoder.setVertexBuffer_offset_atIndex(
                    Some(&slots.buffer),
                    slots.offset,
                    LensDownsampleIndex::Slots as usize,
                );
                encoder.setVertexBytes_length_atIndex(
                    ptr::NonNull::from(&target_size).cast(),
                    mem::size_of_val(&target_size),
                    LensDownsampleIndex::TargetSize as usize,
                );
                encoder.setFragmentBuffer_offset_atIndex(
                    Some(&slots.buffer),
                    slots.offset,
                    LensDownsampleIndex::Slots as usize,
                );
                encoder
                    .setFragmentTexture_atIndex(Some(source), LensDownsampleIndex::Source as usize);
                encoder.drawPrimitives_vertexStart_vertexCount_instanceCount(
                    MTLPrimitiveType::Triangle,
                    0,
                    6,
                    plan.count,
                );
            }
            encoder.endEncoding();
        }
        Ok(())
    }

    fn draw_glass(
        &self,
        plan: &LensPlan,
        viewport_size: Size<DevicePixels>,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) {
        command_encoder.setRenderPipelineState(&plan.pipelines.lens);
        // SAFETY: six unit vertices, `count` lenses and `count` slots at
        // aligned bindings match the shader's slots; the size bytes are
        // copied; the atlas is retained by the plan and the command buffer.
        unsafe {
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&self.unit_vertices),
                0,
                LensInputIndex::Vertices as usize,
            );
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&plan.lenses.buffer),
                plan.lenses.offset,
                LensInputIndex::Lenses as usize,
            );
            command_encoder.setVertexBytes_length_atIndex(
                ptr::NonNull::from(&viewport_size).cast(),
                mem::size_of_val(&viewport_size),
                LensInputIndex::ViewportSize as usize,
            );
            command_encoder.setFragmentBuffer_offset_atIndex(
                Some(&plan.lenses.buffer),
                plan.lenses.offset,
                LensInputIndex::Lenses as usize,
            );
            command_encoder.setFragmentBuffer_offset_atIndex(
                Some(&plan.slots.buffer),
                plan.slots.offset,
                LensInputIndex::Slots as usize,
            );
            command_encoder.setFragmentBytes_length_atIndex(
                ptr::NonNull::from(&plan.atlas_size).cast(),
                mem::size_of_val(&plan.atlas_size),
                LensInputIndex::AtlasSize as usize,
            );
            command_encoder
                .setFragmentTexture_atIndex(Some(&plan.atlas), LensInputIndex::Atlas as usize);
            command_encoder.drawPrimitives_vertexStart_vertexCount_instanceCount(
                MTLPrimitiveType::Triangle,
                0,
                6,
                plan.count,
            );
        }
    }
}

/// QUILL: whether the layer hands out drawables the lens pass can read.
/// Readable drawables cost the compositor its direct path, so they are
/// asked for when glass is drawn and given back after a stretch of frames
/// without any (or when the window goes idle).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum DrawableReads {
    #[default]
    FramebufferOnly,
    ForGlass {
        frames_without: u32,
    },
}

/// Frames without glass before the layer goes back to framebuffer-only.
const FRAMES_BEFORE_FRAMEBUFFER_ONLY: u32 = 120;

impl DrawableReads {
    /// The next state after a frame that drew `lenses` lenses.
    pub(super) fn after_frame(self, lenses: usize) -> Self {
        match (self, lenses) {
            (_, 1..) => Self::ForGlass { frames_without: 0 },
            (Self::FramebufferOnly, 0) => Self::FramebufferOnly,
            (Self::ForGlass { frames_without }, 0) => {
                let frames_without = frames_without.saturating_add(1);
                if frames_without >= FRAMES_BEFORE_FRAMEBUFFER_ONLY {
                    Self::FramebufferOnly
                } else {
                    Self::ForGlass { frames_without }
                }
            }
        }
    }

    pub(super) fn readable(self) -> bool {
        matches!(self, Self::ForGlass { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Bounds, ContentMask, ScaledPixels, point};

    fn lens(x: f32, y: f32, w: f32, h: f32) -> BackdropLens {
        let bounds = Bounds {
            origin: point(ScaledPixels(x), ScaledPixels(y)),
            size: size(ScaledPixels(w), ScaledPixels(h)),
        };
        BackdropLens {
            bounds,
            body: bounds,
            content_mask: ContentMask { bounds },
            ..Default::default()
        }
    }

    #[test]
    fn footprints_are_clipped_to_the_target_and_aligned_in_the_atlas() {
        let packing = pack(
            &[
                lens(-10.5, 20.0, 100.0, 50.0),
                lens(900.0, 700.0, 200.0, 200.0),
            ],
            (1000, 800),
        )
        .unwrap();
        assert_eq!(
            packing.copies[0],
            Copy {
                from: (0, 20),
                to: (0, 0),
                size: (90, 50),
            }
        );
        assert_eq!(packing.copies[1].from, (900, 700));
        assert_eq!(packing.copies[1].size, (100, 100));
        assert_eq!(packing.copies[1].to, (96 + GUTTER, 0));
        assert_eq!(packing.width, 96 + GUTTER + 112 + GUTTER);
        assert_eq!(packing.height, 112);
    }

    #[test]
    fn a_full_row_starts_a_shelf() {
        let lenses = [lens(0.0, 0.0, 600.0, 40.0), lens(0.0, 100.0, 600.0, 40.0)];
        let packing = pack(&lenses, (1000, 800)).unwrap();
        assert_eq!(packing.copies[1].to, (0, 48 + GUTTER));
        assert_eq!(packing.height, 48 + GUTTER + 48);
    }

    #[test]
    fn readable_drawables_last_a_stretch_after_the_glass() {
        let mut reads = DrawableReads::default().after_frame(2);
        assert!(reads.readable());
        for _ in 0..FRAMES_BEFORE_FRAMEBUFFER_ONLY - 1 {
            reads = reads.after_frame(0);
        }
        assert!(reads.readable());
        assert_eq!(reads.after_frame(0), DrawableReads::FramebufferOnly);
    }
}
