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
    new_command_encoder_for_texture, set_frame_globals,
};
use anyhow::{Context as _, Result};
use gpui::{
    DevicePixels, LENS_ATLAS_LEVELS, LensFrame, LensFrameContext, LensReuse, LensSlotPlan,
    LensWork, Scene, Size, size,
};
use objc2::{rc::Retained, runtime::ProtocolObject};
use objc2_core_foundation::CFRetained;
use objc2_core_video::CVMetalTexture;
use objc2_foundation::{NSRange, NSString};
use objc2_metal::{
    MTLBlitCommandEncoder, MTLCommandBuffer, MTLCommandEncoder, MTLDevice, MTLLibrary,
    MTLLoadAction, MTLOrigin, MTLPixelFormat, MTLPrimitiveType, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLSize,
    MTLStorageMode, MTLStoreAction, MTLTexture, MTLTextureDescriptor, MTLTextureType,
    MTLTextureUsage,
};
use std::{mem, ops::Range, ptr};

/// Mip levels of the atlas (the packing gpui-pre does assumes this many).
const LEVELS: usize = LENS_ATLAS_LEVELS as usize;
/// The largest atlas side every Mac GPU supports.
const MAX_SIDE: u32 = 16384;

/// Where one lens's backdrop lies in the target and in the atlas.
/// `lens_fragment` reads it next to its `BackdropLens`. The copy is in
/// device pixels, the slot in atlas texels, `density` texels per pixel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct LensSlot {
    pub copy_x: f32,
    pub copy_y: f32,
    pub copy_w: f32,
    pub copy_h: f32,
    pub slot_x: f32,
    pub slot_y: f32,
    pub density: f32,
    pub pad: f32,
}

impl LensSlot {
    fn of(plan: &LensSlotPlan) -> Self {
        let copy = plan.copy;
        Self {
            copy_x: copy.from.0 as f32,
            copy_y: copy.from.1 as f32,
            copy_w: copy.size.0 as f32,
            copy_h: copy.size.1 as f32,
            slot_x: copy.to.0 as f32,
            slot_y: copy.to.1 as f32,
            density: copy.density.max(1) as f32,
            pad: 0.0,
        }
    }
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

/// The lens pass's slots. Buffers start after `GlobalInputIndex`'s, which
/// the frame's other batches read from the same encoder after the lenses.
#[repr(C)]
pub enum LensInputIndex {
    Lenses = 2,
    Slots = 3,
    AtlasSize = 4,
    Atlas = 5,
}

#[repr(C)]
pub enum LensDownsampleIndex {
    Slots = 1,
    TargetSize = 2,
    Source = 3,
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
                true,
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

/// A texture a re-rendered backdrop is drawn into, kept between frames.
struct SharpCanvas {
    texture: Retained<ProtocolObject<dyn MTLTexture>>,
}

/// What drawing glass keeps between frames: the pipelines, built the first
/// time a lens is drawn in a format (`None`: they did not build, so that
/// format draws fallback quads without trying again), the atlas, what last
/// frame copied into it, and the re-rendered backdrops' canvases.
#[derive(Default)]
pub(super) struct LensState {
    pipelines: Vec<(MTLPixelFormat, Option<LensPipelines>)>,
    atlas: Option<LensAtlas>,
    /// Bumped whenever the atlas is made or dropped.
    atlas_generation: u64,
    reuse: LensReuse,
    canvases: Vec<SharpCanvas>,
}

impl LensState {
    /// Frees the atlas (the window went idle, or a frame drew no glass).
    pub(super) fn release(&mut self) {
        if self.atlas.take().is_some() {
            self.atlas_generation = self.atlas_generation.wrapping_add(1);
        }
        self.canvases.clear();
        self.reuse.forget();
    }

    /// The frame did not draw as planned: nothing in the atlas is trusted.
    pub(super) fn forget(&mut self) {
        self.reuse.forget();
    }

    /// The frame drew `glass` as planned: its slots can be reused.
    pub(super) fn keep(&mut self, glass: &GlassFrame) {
        self.reuse.keep(&glass.plan, glass.context);
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

    /// The atlas for `width` x `height`, and whether it was made now.
    fn atlas(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        format: MTLPixelFormat,
        width: u32,
        height: u32,
    ) -> Result<(&LensAtlas, bool)> {
        if let Some(atlas) = self
            .atlas
            .take_if(|atlas| atlas.fits(format, width, height))
        {
            return Ok((self.atlas.insert(atlas), false));
        }
        // The old atlas goes first, so the two never live at once.
        self.atlas = None;
        self.atlas_generation = self.atlas_generation.wrapping_add(1);
        let atlas = LensAtlas::new(device, format, width, height)?;
        Ok((self.atlas.insert(atlas), true))
    }

    /// The `index`th canvas, `width` x `height` texels in `format`.
    fn canvas(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        index: usize,
        format: MTLPixelFormat,
        width: u32,
        height: u32,
    ) -> Result<Retained<ProtocolObject<dyn MTLTexture>>> {
        let fits = |canvas: &SharpCanvas| {
            canvas.texture.pixelFormat() == format
                && canvas.texture.width() == width as usize
                && canvas.texture.height() == height as usize
        };
        if let Some(canvas) = self.canvases.get(index).filter(|c| fits(c)) {
            return Ok(canvas.texture.clone());
        }
        // The plan keeps every slot inside MAX_SIDE.
        let texture = super::scene_target::new_scene_target(device, format, width, height)
            .context("allocating a sharp backdrop canvas")?;
        let canvas = SharpCanvas {
            texture: texture.clone(),
        };
        match self.canvases.get_mut(index) {
            Some(slot) => *slot = canvas,
            None => self.canvases.push(canvas),
        }
        Ok(texture)
    }
}

/// One frame's glass, planned before anything is encoded: the pipelines,
/// the atlas, every lens's slot, and the canvases re-rendered backdrops
/// were drawn into.
pub(super) struct GlassFrame {
    pipelines: LensPipelines,
    atlas: Retained<ProtocolObject<dyn MTLTexture>>,
    levels: Vec<Retained<ProtocolObject<dyn MTLTexture>>>,
    atlas_size: Size<DevicePixels>,
    pub(super) plan: LensFrame,
    context: LensFrameContext,
    /// Every lens of the scene, and its slot, in scene order.
    lenses: InstanceBinding,
    slots: InstanceBinding,
    /// Lens index and the canvas its backdrop was re-rendered into.
    canvases: Vec<(usize, Retained<ProtocolObject<dyn MTLTexture>>)>,
    /// The command buffers drawing the canvases, committed (ahead of the
    /// frame's) once the frame's instance data is published.
    renders: Vec<Retained<ProtocolObject<dyn MTLCommandBuffer>>>,
}

impl GlassFrame {
    /// The atlas, for labels that pick their ink from it.
    pub(super) fn atlas(&self) -> &Retained<ProtocolObject<dyn MTLTexture>> {
        &self.atlas
    }

    /// Commits the re-rendered backdrops; call after the instance writer
    /// is finished and before the frame's command buffer is committed.
    pub(super) fn commit_renders(&self) {
        for command_buffer in &self.renders {
            command_buffer.commit();
        }
    }
}

impl MetalRenderer {
    /// Plans the frame's glass: `None` when it draws flat (no lenses, a
    /// framebuffer-only target, no pipelines, footprints too large). Encodes
    /// the re-rendered backdrops into their canvases, in command buffers
    /// [`GlassFrame::commit_renders`] commits ahead of the frame's.
    pub(super) fn plan_glass(
        &mut self,
        scene: &Scene,
        texture: &ProtocolObject<dyn MTLTexture>,
        writer: &mut InstanceBufferWriter,
        surfaces: &mut Vec<CFRetained<CVMetalTexture>>,
    ) -> Result<Option<GlassFrame>> {
        if scene.lenses.is_empty() || texture.isFramebufferOnly() {
            return Ok(None);
        }
        let format = texture.pixelFormat();
        let Some(pipelines) = self.lens.pipelines(&self.device, &self.library, format) else {
            return Ok(None);
        };
        let target = (texture.width() as u32, texture.height() as u32);
        let mut context = LensFrameContext {
            target,
            atlas: self.lens.atlas_generation,
            sprites: self.sprite_atlas.removals(),
        };
        let Some(mut plan) = self.lens.reuse.plan(scene, context, MAX_SIDE, true) else {
            return Ok(None);
        };
        let (atlas, made) = self
            .lens
            .atlas(&self.device, format, plan.width, plan.height)?;
        let (atlas_texture, levels, atlas_size) =
            (atlas.texture.clone(), atlas.levels.clone(), atlas.size());
        if made {
            plan.copy_everything();
        }
        context.atlas = self.lens.atlas_generation;

        let mut canvases = Vec::new();
        let mut renders = Vec::new();
        for (index, slot) in plan.slots.iter().enumerate() {
            if slot.work != LensWork::Render {
                continue;
            }
            let Some(sharp) = scene.sharp_backdrops.iter().find(|s| s.lens == index) else {
                continue;
            };
            let (width, height) = slot.copy.texels();
            let canvas = self
                .lens
                .canvas(&self.device, canvases.len(), format, width, height)?;
            let clear = self.frame_clear();
            let command_buffer =
                self.draw_scene_into(&sharp.scene, writer, &canvas, clear, surfaces)?;
            renders.push(command_buffer);
            canvases.push((index, canvas));
        }

        let slots: Vec<LensSlot> = plan.slots.iter().map(LensSlot::of).collect();
        Ok(Some(GlassFrame {
            pipelines,
            atlas: atlas_texture,
            levels,
            atlas_size,
            lenses: writer.write(&scene.lenses)?,
            slots: writer.write(&slots)?,
            plan,
            context,
            canvases,
            renders,
        }))
    }

    /// Draws one lens batch, glass when the frame planned it, else its
    /// fallback quads. May end `command_encoder` and put a new one over the
    /// same target in its place.
    pub(super) fn draw_lenses(
        &mut self,
        glass: Option<&GlassFrame>,
        range: Range<usize>,
        instance_bindings: &InstanceBindings,
        writer: &mut InstanceBufferWriter,
        texture: &ProtocolObject<dyn MTLTexture>,
        depth_texture: &ProtocolObject<dyn MTLTexture>,
        viewport_size: Size<DevicePixels>,
        command_buffer: &ProtocolObject<dyn MTLCommandBuffer>,
        command_encoder: &mut Retained<ProtocolObject<dyn MTLRenderCommandEncoder>>,
    ) -> Result<()> {
        let Some(glass) = glass else {
            self.draw_quads(range, &instance_bindings.lens_fallbacks, command_encoder);
            return Ok(());
        };
        let fresh: Vec<(usize, LensSlotPlan)> = glass
            .plan
            .batch(range.clone())
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.work != LensWork::Reuse)
            .map(|(i, slot)| (range.start + i, *slot))
            .collect();
        if !fresh.is_empty() {
            command_encoder.endEncoding();
            let filled = self.fill_slots(glass, &fresh, texture, writer, command_buffer);
            *command_encoder = new_command_encoder_for_texture(
                command_buffer,
                texture,
                depth_texture,
                viewport_size,
                None,
            )?;
            // A frame with lenses draws without depth (a zero scale), so the
            // reopened pass starts as the frame's first one did.
            set_frame_globals(command_encoder, viewport_size, 0.0);
            command_encoder.setDepthStencilState(Some(&self.depth_ignore_state));
            if let Err(error) = filled {
                log::error!("glass draws flat: {error:#}");
                self.lens.forget();
                self.draw_quads(range, &instance_bindings.lens_fallbacks, command_encoder);
                return Ok(());
            }
        }
        self.draw_glass(glass, range, command_encoder);
        Ok(())
    }

    /// Copies the batch's fresh slots into the atlas (from the target, or
    /// from their canvas) and fills their mip levels.
    fn fill_slots(
        &self,
        glass: &GlassFrame,
        fresh: &[(usize, LensSlotPlan)],
        texture: &ProtocolObject<dyn MTLTexture>,
        writer: &mut InstanceBufferWriter,
        command_buffer: &ProtocolObject<dyn MTLCommandBuffer>,
    ) -> Result<()> {
        let blit = command_buffer
            .blitCommandEncoder()
            .context("creating the lens copy encoder")?;
        for (index, slot) in fresh {
            let copy = slot.copy;
            let (width, height) = copy.texels();
            if width == 0 || height == 0 {
                continue;
            }
            let (source, origin): (&ProtocolObject<dyn MTLTexture>, (u32, u32)) = match slot.work {
                LensWork::Render => match glass.canvases.iter().find(|(lens, _)| lens == index) {
                    Some((_, canvas)) => (canvas, (0, 0)),
                    None => continue,
                },
                _ => (texture, copy.from),
            };
            // SAFETY: both textures are live and retained by the command
            // buffer; the source region lies inside the target (the plan
            // clipped it) or is the whole canvas (made at the slot's
            // size), the destination lies inside the atlas (the plan sized
            // it), and all of them share the target's pixel format.
            unsafe {
                blit.copyFromTexture_sourceSlice_sourceLevel_sourceOrigin_sourceSize_toTexture_destinationSlice_destinationLevel_destinationOrigin(
                    source,
                    0,
                    0,
                    MTLOrigin { x: origin.0 as usize, y: origin.1 as usize, z: 0 },
                    MTLSize { width: width as usize, height: height as usize, depth: 1 },
                    &glass.atlas,
                    0,
                    0,
                    MTLOrigin { x: copy.to.0 as usize, y: copy.to.1 as usize, z: 0 },
                );
            }
        }
        blit.endEncoding();

        for level in 1..LEVELS {
            let (Some(target), Some(source)) =
                (glass.levels.get(level), glass.levels.get(level - 1))
            else {
                break;
            };
            let at = |v: u32, level: usize| v as f32 / (1u32 << level) as f32;
            let level_slots: Vec<LensDownsampleSlot> = fresh
                .iter()
                .map(|(_, slot)| {
                    let copy = slot.copy;
                    let (w, h) = copy.texels();
                    LensDownsampleSlot {
                        dst_x: at(copy.to.0, level),
                        dst_y: at(copy.to.1, level),
                        dst_w: at(w, level).ceil(),
                        dst_h: at(h, level).ceil(),
                        src_x0: at(copy.to.0, level - 1),
                        src_y0: at(copy.to.1, level - 1),
                        src_x1: at(copy.to.0, level - 1) + at(w, level - 1).ceil(),
                        src_y1: at(copy.to.1, level - 1) + at(h, level - 1).ceil(),
                    }
                })
                .collect();
            let slots = writer.write(&level_slots)?;
            let descriptor = MTLRenderPassDescriptor::new();
            // SAFETY: Index zero is within the render pass's color attachment array.
            let attachment = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
            attachment.setTexture(Some(target));
            // Load: slots reused from last frame live in this level too.
            attachment.setLoadAction(MTLLoadAction::Load);
            attachment.setStoreAction(MTLStoreAction::Store);
            let encoder = command_buffer
                .renderCommandEncoderWithDescriptor(&descriptor)
                .context("creating a lens blur encoder")?;
            let target_size = size(
                DevicePixels(target.width() as i32),
                DevicePixels(target.height() as i32),
            );
            encoder.setRenderPipelineState(&glass.pipelines.downsample);
            // SAFETY: `fresh.len()` downsample slots at an aligned binding
            // match the shader's slots; the size bytes are copied; `source`
            // is another level than the attachment.
            unsafe {
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
                    MTLPrimitiveType::TriangleStrip,
                    0,
                    4,
                    fresh.len(),
                );
            }
            encoder.endEncoding();
        }
        Ok(())
    }

    fn draw_glass(
        &self,
        glass: &GlassFrame,
        range: Range<usize>,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) {
        if range.is_empty() {
            return;
        }
        command_encoder.setRenderPipelineState(&glass.pipelines.lens);
        // SAFETY: the frame's lenses and slots (one per scene lens) at
        // aligned bindings match the shader's slots; the batch's range lies
        // inside them; the size bytes are copied; the atlas is retained by
        // the frame and the command buffer. The viewport size is the frame's
        // global (`set_frame_globals`).
        unsafe {
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&glass.lenses.buffer),
                glass.lenses.offset,
                LensInputIndex::Lenses as usize,
            );
            command_encoder.setFragmentBuffer_offset_atIndex(
                Some(&glass.lenses.buffer),
                glass.lenses.offset,
                LensInputIndex::Lenses as usize,
            );
            command_encoder.setFragmentBuffer_offset_atIndex(
                Some(&glass.slots.buffer),
                glass.slots.offset,
                LensInputIndex::Slots as usize,
            );
            command_encoder.setFragmentBytes_length_atIndex(
                ptr::NonNull::from(&glass.atlas_size).cast(),
                mem::size_of_val(&glass.atlas_size),
                LensInputIndex::AtlasSize as usize,
            );
            command_encoder
                .setFragmentTexture_atIndex(Some(&glass.atlas), LensInputIndex::Atlas as usize);
            command_encoder.drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                MTLPrimitiveType::TriangleStrip,
                0,
                4,
                range.len(),
                range.start,
            );
        }
    }
}

/// QUILL: whether the layer hands out drawables the lens pass can read.
/// Readable drawables cost the compositor its direct path, so they are
/// asked for when glass is drawn and given back after a stretch of frames
/// without any. Idling keeps them: an idle window draws nothing, and its
/// next frame usually has its glass again.
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
