// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.
//! QUILL: backdrop glass on DirectX 11 (`PrimitiveBatch::Lenses`), as
//! gpui-pre-apple's `metal_renderer/lens.rs` and gpui-pre-wgpu's
//! `wgpu_renderer/lens.rs` draw it.
//!
//! The frame plans every lens's slot in one pooled atlas before its first
//! batch ([`DirectXRenderer::plan_glass`], gpui-pre's `LensReuse`). A lens
//! batch unbinds the back buffer, copies the footprints of its lenses whose
//! backdrop changed out of it into their slots (`CopySubresourceRegion`),
//! fills those slots' mip levels with dual-Kawase downsamples, binds the
//! back buffer again and draws the lenses from the atlas (`lens_fragment`
//! in shaders_lens.hlsl). A batch whose every slot still holds its backdrop
//! from the last frame only draws. A back buffer that can't be copied (more than one
//! sample), shaders that did not build or footprints too large for an atlas
//! draw the batch as its fallback quads.
//!
//! The pipelines are built on the first frame with glass; frames without
//! glass do no extra work, and the first one frees the atlas. All of it
//! lives in `DirectXRenderPipelines`, so a device loss drops it with the
//! device.

use std::slice;

use anyhow::{Context, Result};
use gpui::{
    BackdropLens, LENS_ATLAS_LEVELS, LensCopy, LensFrame, LensFrameContext, LensReuse,
    LensSlotPlan, LensWork, Scene,
};
use windows::Win32::Graphics::{Direct3D::*, Direct3D11::*, Dxgi::Common::*};

use super::shader_resources::ShaderModule;
use super::{
    DirectXRenderer, MAX_INSTANCE_BUFFER_SIZE, PipelineState, create_blend_state, create_buffer,
    create_buffer_view, update_buffer,
};

/// Mip levels of the atlas (the packing gpui-pre does assumes this many;
/// `LENS_LEVELS` in shaders_lens.hlsl).
const LEVELS: u32 = LENS_ATLAS_LEVELS;
/// The atlas's sides are multiples of this, so every level halves exactly.
const ATLAS_STEP: u32 = 256;
/// The largest texture side of feature level 10 devices (11 and up take
/// `D3D11_REQ_TEXTURE2D_U_OR_V_DIMENSION`).
const MAX_SIDE_LEVEL_10: u32 = 8192;

// `BackdropLens` in shaders_lens.hlsl: a structured buffer packs its fields
// at 4 bytes, as `#[repr(C)]` lays out these all-4-byte fields.
const _: () = {
    assert!(std::mem::size_of::<BackdropLens>() == 328);
    assert!(std::mem::size_of::<LensSlot>() == 32);
    assert!(std::mem::size_of::<LensDownsampleSlot>() == 32);
};

/// Where one lens's backdrop lies in the target and in the atlas, device
/// pixels (`LensSlot` in shaders_lens.hlsl).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
struct LensSlot {
    copy_x: f32,
    copy_y: f32,
    copy_w: f32,
    copy_h: f32,
    slot_x: f32,
    slot_y: f32,
    density: f32,
    pad: f32,
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

/// One slot of one downsample pass: the texels it writes in the level being
/// filled, and the texels of the level above it may read
/// (`LensDownsampleSlot` in shaders_lens.hlsl).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
struct LensDownsampleSlot {
    dst_x: f32,
    dst_y: f32,
    dst_w: f32,
    dst_h: f32,
    src_x0: f32,
    src_y0: f32,
    src_x1: f32,
    src_y1: f32,
}

impl LensDownsampleSlot {
    /// `copy`'s slot in atlas level `level` (1 or more), read from the
    /// level above.
    fn new(copy: &LensCopy, level: u32) -> Self {
        let at = |v: u32, level: u32| v as f32 / (1u32 << level) as f32;
        let (w, h) = copy.texels();
        Self {
            dst_x: at(copy.to.0, level),
            dst_y: at(copy.to.1, level),
            dst_w: at(w, level).ceil(),
            dst_h: at(h, level).ceil(),
            src_x0: at(copy.to.0, level - 1),
            src_y0: at(copy.to.1, level - 1),
            src_x1: at(copy.to.0, level - 1) + at(w, level - 1).ceil(),
            src_y1: at(copy.to.1, level - 1) + at(h, level - 1).ceil(),
        }
    }
}

/// How a lens batch was drawn.
pub(super) enum Glass {
    /// As glass.
    Drawn,
    /// Not at all: the caller draws its fallback quads.
    Flat,
}

/// The lenses' atlas slots, a structured buffer at t2 (the lenses
/// themselves are the lens pipeline's own buffer at t1).
struct SlotBuffer {
    buffer: ID3D11Buffer,
    view: Option<ID3D11ShaderResourceView>,
    capacity: usize,
}

impl SlotBuffer {
    fn new(device: &ID3D11Device, capacity: usize) -> Result<Self> {
        let buffer = create_buffer(device, std::mem::size_of::<LensSlot>(), capacity)?;
        let view = create_buffer_view(device, &buffer)?;
        Ok(Self {
            buffer,
            view,
            capacity,
        })
    }

    fn write(
        &mut self,
        device: &ID3D11Device,
        context: &ID3D11DeviceContext,
        slots: &[LensSlot],
    ) -> Result<()> {
        if self.capacity < slots.len() {
            anyhow::ensure!(
                std::mem::size_of_val(slots) <= MAX_INSTANCE_BUFFER_SIZE,
                "{} lens slots are more than a buffer holds",
                slots.len()
            );
            *self = Self::new(device, slots.len().next_power_of_two())?;
        }
        update_buffer(context, &self.buffer, slots)
    }
}

/// The glass's shaders and what binds to them.
struct LensPipelines {
    lens: PipelineState<BackdropLens>,
    downsample: PipelineState<LensDownsampleSlot>,
    slots: SlotBuffer,
    /// Clamped and trilinear: between levels for the lens, within the one
    /// bound level for the downsamples.
    sampler: Option<ID3D11SamplerState>,
}

impl LensPipelines {
    fn new(device: &ID3D11Device) -> Result<Self> {
        let lens = PipelineState::new(
            device,
            "lens_pipeline",
            ShaderModule::Lens,
            8,
            create_blend_state(device)?,
        )?;
        let downsample = PipelineState::new(
            device,
            "lens_downsample_pipeline",
            ShaderModule::LensDownsample,
            32,
            create_replace_blend_state(device)?,
        )?;
        let desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            MipLODBias: 0.0,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D11_COMPARISON_ALWAYS,
            BorderColor: [0.0; 4],
            MinLOD: 0.0,
            MaxLOD: D3D11_FLOAT32_MAX,
        };
        let mut sampler = None;
        // SAFETY: a valid descriptor; the call writes the out parameter.
        unsafe { device.CreateSamplerState(&desc, Some(&mut sampler))? };
        Ok(Self {
            lens,
            downsample,
            slots: SlotBuffer::new(device, 8)?,
            sampler,
        })
    }
}

/// Writes the source over the target: the downsamples replace a slot.
fn create_replace_blend_state(device: &ID3D11Device) -> Result<ID3D11BlendState> {
    let mut desc = D3D11_BLEND_DESC::default();
    desc.RenderTarget[0].BlendEnable = false.into();
    desc.RenderTarget[0].SrcBlend = D3D11_BLEND_ONE;
    desc.RenderTarget[0].DestBlend = D3D11_BLEND_ZERO;
    desc.RenderTarget[0].BlendOp = D3D11_BLEND_OP_ADD;
    desc.RenderTarget[0].SrcBlendAlpha = D3D11_BLEND_ONE;
    desc.RenderTarget[0].DestBlendAlpha = D3D11_BLEND_ZERO;
    desc.RenderTarget[0].BlendOpAlpha = D3D11_BLEND_OP_ADD;
    desc.RenderTarget[0].RenderTargetWriteMask = D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8;
    let mut state = None;
    // SAFETY: a valid descriptor; the call writes the out parameter.
    unsafe { device.CreateBlendState(&desc, Some(&mut state))? };
    state.context("creating the lens downsample blend state")
}

/// Whether the glass's shaders exist. They are built the first time a frame
/// has glass, and never again after they fail (glass then draws flat).
#[derive(Default)]
enum Pipelines {
    #[default]
    Unbuilt,
    Built(Box<LensPipelines>),
    Failed,
}

impl Pipelines {
    fn get(&mut self, device: &ID3D11Device) -> Option<&mut LensPipelines> {
        if matches!(self, Self::Unbuilt) {
            *self = match LensPipelines::new(device) {
                Ok(pipelines) => Self::Built(Box::new(pipelines)),
                Err(error) => {
                    log::error!("glass draws flat: its shaders did not build: {error:#}");
                    Self::Failed
                }
            };
        }
        match self {
            Self::Built(pipelines) => Some(pipelines),
            Self::Unbuilt | Self::Failed => None,
        }
    }
}

/// One mip level of the atlas: rendered by one downsample, read by the next.
struct AtlasLevel {
    target: Option<ID3D11RenderTargetView>,
    view: Option<ID3D11ShaderResourceView>,
    viewport: D3D11_VIEWPORT,
}

/// The pooled atlas: a mip-chained texture in the back buffer's format, a
/// view of the whole chain (the lens samples it) and a target and a view of
/// each level (a downsample renders one level while it samples the one
/// above).
struct LensAtlas {
    texture: ID3D11Texture2D,
    format: DXGI_FORMAT,
    width: u32,
    height: u32,
    chain: Option<ID3D11ShaderResourceView>,
    levels: Vec<AtlasLevel>,
}

impl LensAtlas {
    fn new(
        device: &ID3D11Device,
        format: DXGI_FORMAT,
        width: u32,
        height: u32,
        max_side: u32,
    ) -> Result<Self> {
        // Rounded up, so a lens that grows a little keeps its atlas.
        let side = |v: u32| v.next_multiple_of(ATLAS_STEP).max(ATLAS_STEP).min(max_side);
        let (width, height) = (side(width), side(height));
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: LEVELS,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut texture = None;
        // SAFETY: a valid descriptor (sides of at least 256 allow five
        // levels); the call writes the out parameter.
        unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture))? };
        let texture = texture.context("creating the lens atlas")?;
        let mut chain = None;
        // SAFETY: a view of the whole texture just made, by its own format.
        unsafe { device.CreateShaderResourceView(&texture, None, Some(&mut chain))? };
        let levels = (0..LEVELS)
            .map(|level| {
                let target_desc = D3D11_RENDER_TARGET_VIEW_DESC {
                    Format: format,
                    ViewDimension: D3D11_RTV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D11_RENDER_TARGET_VIEW_DESC_0 {
                        Texture2D: D3D11_TEX2D_RTV { MipSlice: level },
                    },
                };
                let view_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
                    Format: format,
                    ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                        Texture2D: D3D11_TEX2D_SRV {
                            MostDetailedMip: level,
                            MipLevels: 1,
                        },
                    },
                };
                let (mut target, mut view) = (None, None);
                // SAFETY: views of one existing level of the texture, in
                // its format, for the bindings it was made with.
                unsafe {
                    device.CreateRenderTargetView(
                        &texture,
                        Some(&target_desc),
                        Some(&mut target),
                    )?;
                    device.CreateShaderResourceView(&texture, Some(&view_desc), Some(&mut view))?;
                }
                Ok(AtlasLevel {
                    target,
                    view,
                    viewport: D3D11_VIEWPORT {
                        TopLeftX: 0.0,
                        TopLeftY: 0.0,
                        Width: (width >> level) as f32,
                        Height: (height >> level) as f32,
                        MinDepth: 0.0,
                        MaxDepth: 1.0,
                    },
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            texture,
            format,
            width,
            height,
            chain,
            levels,
        })
    }

    fn fits(&self, format: DXGI_FORMAT, width: u32, height: u32) -> bool {
        self.format == format && self.width >= width && self.height >= height
    }
}

/// What drawing glass keeps between frames, per window. The atlas goes
/// after the first frame without glass; all of it goes with the device.
/// `reuse` remembers what the atlas's slots hold, and `atlas_generation`
/// moves whenever the atlas is made or dropped, so nothing is reused from
/// an atlas that is not the one drawn.
#[derive(Default)]
pub(super) struct LensState {
    pipelines: Pipelines,
    atlas: Option<LensAtlas>,
    atlas_generation: u64,
    reuse: LensReuse,
}

impl LensState {
    /// Frees the atlas after a frame without glass.
    pub(super) fn release_idle_atlas(&mut self, lenses: usize) {
        if lenses == 0 {
            if self.atlas.take().is_some() {
                self.atlas_generation = self.atlas_generation.wrapping_add(1);
            }
            self.reuse.forget();
        }
    }

    /// Nothing in the atlas is known to be current (set before a frame
    /// draws, so a frame that fails part way leaves it so).
    pub(super) fn forget(&mut self) {
        self.reuse.forget();
    }

    /// The frame drew every lens as planned.
    pub(super) fn keep(&mut self, glass: &GlassFrame) {
        self.reuse.keep(&glass.plan, glass.context);
    }

    /// An atlas that holds `width` x `height` in `format`: the pooled one
    /// when it does (`false`), else a new one (`true`).
    fn atlas(
        &mut self,
        device: &ID3D11Device,
        format: DXGI_FORMAT,
        width: u32,
        height: u32,
        max_side: u32,
    ) -> Result<bool> {
        if let Some(kept) = self.atlas.take_if(|kept| kept.fits(format, width, height)) {
            self.atlas = Some(kept);
            return Ok(false);
        }
        self.atlas = None;
        self.atlas_generation = self.atlas_generation.wrapping_add(1);
        self.atlas = Some(LensAtlas::new(device, format, width, height, max_side)?);
        Ok(true)
    }
}

/// The frame's glass, planned before its first batch: every lens's slot in
/// the atlas, and whether last frame's copy still holds it.
pub(super) struct GlassFrame {
    plan: LensFrame,
    context: LensFrameContext,
}

/// How a frame's lens batches were drawn: all from the frame's plan, or
/// some flat (whose slots then hold nothing to reuse).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GlassDrawn {
    AsPlanned,
    Flat,
}

/// The largest atlas side the device takes, a whole number of steps.
fn max_atlas_side(device: &ID3D11Device) -> u32 {
    // SAFETY: a plain query of a live device.
    let level = unsafe { device.GetFeatureLevel() };
    if level.0 >= D3D_FEATURE_LEVEL_11_0.0 {
        D3D11_REQ_TEXTURE2D_U_OR_V_DIMENSION
    } else {
        MAX_SIDE_LEVEL_10
    }
}

impl DirectXRenderer {
    /// Plans the frame's glass: `None` when it draws flat (no lenses, a
    /// back buffer that can't be copied, no shaders, lenses too large for
    /// an atlas, or no atlas). Re-rendered backdrops are Metal's; here
    /// those lenses copy what the frame drew.
    pub(super) fn plan_glass(&mut self, scene: &Scene) -> Option<GlassFrame> {
        if scene.lenses.is_empty() {
            return None;
        }
        let device = &self.devices.as_ref()?.device;
        let target = self.resources.as_ref()?.render_target.as_ref()?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: a plain query of the live back buffer.
        unsafe { target.GetDesc(&mut desc) };
        // A copy with a source box needs a single-sample source; the
        // flip-model back buffer always is one, but refuse rather than copy
        // if that ever changes.
        if desc.SampleDesc.Count != 1 {
            return None;
        }
        let state = &mut self.pipelines.lenses;
        state.pipelines.get(device)?;
        let max_side = max_atlas_side(device);
        let mut context = LensFrameContext {
            target: (desc.Width, desc.Height),
            atlas: state.atlas_generation,
            sprites: self.atlas.removals(),
        };
        let mut plan = state.reuse.plan(scene, context, max_side, false)?;
        let made = state
            .atlas(device, desc.Format, plan.width, plan.height, max_side)
            .inspect_err(|error| log::error!("glass draws flat: {error:#}"))
            .ok()?;
        if made {
            plan.copy_everything();
        }
        context.atlas = state.atlas_generation;
        Some(GlassFrame { plan, context })
    }

    /// Draws lenses `range` of the scene over what the back buffer holds,
    /// as `glass` planned them, or says they must draw flat. Leaves the
    /// back buffer bound with the frame's viewport, whatever happens.
    pub(super) fn draw_lenses(
        &mut self,
        glass: Option<&GlassFrame>,
        lenses: &[BackdropLens],
        range: std::ops::Range<usize>,
    ) -> Result<Glass> {
        if lenses.is_empty() {
            return Ok(Glass::Drawn);
        }
        let Some(glass) = glass else {
            return Ok(Glass::Flat);
        };
        let planned = glass.plan.batch(range);
        if planned.len() != lenses.len() {
            return Ok(Glass::Flat);
        }
        let devices = self.devices.as_ref().context("devices missing")?;
        let resources = self.resources.as_ref().context("resources missing")?;
        let batch_params = self
            .globals
            .batch_params_buffer
            .as_ref()
            .context("batch params buffer missing")?;
        let Some(target) = resources.render_target.as_ref() else {
            return Ok(Glass::Flat);
        };
        let device = &devices.device;
        let context = &devices.device_context;
        let state = &mut self.pipelines.lenses;
        let (Some(pipelines), Some(atlas)) = (state.pipelines.get(device), state.atlas.as_ref())
        else {
            return Ok(Glass::Flat);
        };
        let count = u32::try_from(lenses.len()).context("too many lenses in one batch")?;

        pipelines.lens.update_buffer(device, context, lenses)?;
        let slots: Vec<LensSlot> = planned.iter().map(LensSlot::of).collect();
        pipelines.slots.write(device, context, &slots)?;
        let copies: Vec<LensCopy> = planned
            .iter()
            .filter(|slot| slot.work != LensWork::Reuse)
            .map(|slot| slot.copy)
            .collect();
        if !copies.is_empty() {
            // Level k's slots start at (k - 1) * copies.
            let level_slots: Vec<LensDownsampleSlot> = (1..LEVELS)
                .flat_map(|level| {
                    copies
                        .iter()
                        .map(move |copy| LensDownsampleSlot::new(copy, level))
                })
                .collect();
            pipelines
                .downsample
                .update_buffer(device, context, &level_slots)?;
        }

        let frame = Frame {
            texture: target,
            target: &resources.render_target_view,
            viewport: &resources.viewport,
        };
        let drawn = draw_glass(
            context,
            batch_params,
            pipelines,
            atlas,
            &frame,
            &copies,
            count,
        );
        // SAFETY: rebinds the frame's own live target and viewport, and
        // unbinds the atlas, so the next batch (or the next frame's copy
        // into the atlas) finds neither bound.
        unsafe {
            context.VSSetShaderResources(3, Some(&[None]));
            context.PSSetShaderResources(2, Some(&[None, None]));
            context.OMSetRenderTargets(Some(slice::from_ref(&resources.render_target_view)), None);
            context.RSSetViewports(Some(slice::from_ref(&resources.viewport)));
        }
        drawn.map(|()| Glass::Drawn)
    }
}

/// The back buffer a lens batch reads and draws into.
struct Frame<'a> {
    texture: &'a ID3D11Texture2D,
    target: &'a Option<ID3D11RenderTargetView>,
    viewport: &'a D3D11_VIEWPORT,
}

/// Copies the fresh footprints into the atlas, fills their levels and draws
/// the lenses over the back buffer. The caller restores the bindings after,
/// as an error can leave an atlas level bound.
fn draw_glass(
    context: &ID3D11DeviceContext,
    batch_params: &ID3D11Buffer,
    pipelines: &LensPipelines,
    atlas: &LensAtlas,
    frame: &Frame<'_>,
    copies: &[LensCopy],
    count: u32,
) -> Result<()> {
    if !copies.is_empty() {
        fill_slots(context, batch_params, pipelines, atlas, frame, copies)?;
    }
    // SAFETY: binding the back buffer replaces the last level as the
    // target before the whole chain is bound to be read.
    unsafe {
        context.OMSetRenderTargets(Some(slice::from_ref(frame.target)), None);
        context.RSSetViewports(Some(slice::from_ref(frame.viewport)));
        context.VSSetShaderResources(3, Some(&[None]));
        context.PSSetShaderResources(2, Some(slice::from_ref(&pipelines.slots.view)));
        context.PSSetShaderResources(3, Some(slice::from_ref(&atlas.chain)));
    }
    pipelines.lens.draw_range(context, batch_params, 0, count)
}

/// Copies `copies` out of the back buffer into their slots and fills their
/// mip levels.
fn fill_slots(
    context: &ID3D11DeviceContext,
    batch_params: &ID3D11Buffer,
    pipelines: &LensPipelines,
    atlas: &LensAtlas,
    frame: &Frame<'_>,
    copies: &[LensCopy],
) -> Result<()> {
    let fresh = u32::try_from(copies.len()).context("too many lenses in one batch")?;
    // SAFETY: unbinding the back buffer before it is read; every source box
    // lies inside it and every destination inside the atlas's level 0
    // (the frame's plan clipped the footprints to the target and sized
    // the atlas to hold the slots), and both share one format.
    unsafe {
        context.OMSetRenderTargets(None, None);
        for copy in copies
            .iter()
            .filter(|copy| copy.size.0 > 0 && copy.size.1 > 0)
        {
            let source = D3D11_BOX {
                left: copy.from.0,
                top: copy.from.1,
                front: 0,
                right: copy.from.0.saturating_add(copy.size.0),
                bottom: copy.from.1.saturating_add(copy.size.1),
                back: 1,
            };
            context.CopySubresourceRegion(
                &atlas.texture,
                0,
                copy.to.0,
                copy.to.1,
                0,
                frame.texture,
                0,
                Some(&source),
            );
        }
        context.PSSetSamplers(1, Some(slice::from_ref(&pipelines.sampler)));
    }
    for ((level, source), written) in (0u32..).zip(&atlas.levels).zip(atlas.levels.iter().skip(1)) {
        // SAFETY: the level written is bound before the one read, so the
        // two bindings never name one level at once.
        unsafe {
            context.OMSetRenderTargets(Some(slice::from_ref(&written.target)), None);
            context.RSSetViewports(Some(slice::from_ref(&written.viewport)));
            context.VSSetShaderResources(3, Some(slice::from_ref(&source.view)));
            context.PSSetShaderResources(3, Some(slice::from_ref(&source.view)));
        }
        pipelines.downsample.draw_range(
            context,
            batch_params,
            level.saturating_mul(fresh),
            fresh,
        )?;
    }
    Ok(())
}
