// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.
//! QUILL: backdrop glass on DirectX 11 (`PrimitiveBatch::Lenses`), as
//! gpui-pre-apple's `metal_renderer/lens.rs` and gpui-pre-wgpu's
//! `wgpu_renderer/lens.rs` draw it.
//!
//! A lens batch unbinds the back buffer, copies each lens's footprint out of
//! it into a slot of a pooled atlas (`CopySubresourceRegion`), fills the
//! atlas's mip levels with dual-Kawase downsamples, binds the back buffer
//! again and draws the lenses from the atlas (`lens_fragment` in
//! shaders_lens.hlsl). A back buffer that can't be copied (more than one
//! sample), shaders that did not build or footprints too large for an atlas
//! draw the batch as its fallback quads.
//!
//! The pipelines are built on the first frame with glass; frames without
//! glass do no extra work, and the first one frees the atlas. All of it
//! lives in `DirectXRenderPipelines`, so a device loss drops it with the
//! device.

use std::slice;

use anyhow::{Context, Result};
use gpui::{BackdropLens, LENS_ATLAS_LEVELS, LensCopy, LensPacking};
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
    assert!(std::mem::size_of::<BackdropLens>() == 240);
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
    pad_x: f32,
    pad_y: f32,
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
        Self {
            dst_x: at(copy.to.0, level),
            dst_y: at(copy.to.1, level),
            dst_w: at(copy.size.0, level).ceil(),
            dst_h: at(copy.size.1, level).ceil(),
            src_x0: at(copy.to.0, level - 1),
            src_y0: at(copy.to.1, level - 1),
            src_x1: at(copy.to.0, level - 1) + at(copy.size.0, level - 1).ceil(),
            src_y1: at(copy.to.1, level - 1) + at(copy.size.1, level - 1).ceil(),
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
#[derive(Default)]
pub(super) struct LensState {
    pipelines: Pipelines,
    atlas: Option<LensAtlas>,
}

impl LensState {
    /// Frees the atlas after a frame without glass.
    pub(super) fn release_idle_atlas(&mut self, lenses: usize) {
        if lenses == 0 {
            self.atlas = None;
        }
    }
}

/// An atlas that holds `width` x `height` in `format`: the pooled one when it
/// does, else a new one.
fn atlas<'a>(
    atlas: &'a mut Option<LensAtlas>,
    device: &ID3D11Device,
    format: DXGI_FORMAT,
    width: u32,
    height: u32,
    max_side: u32,
) -> Result<&'a LensAtlas> {
    if let Some(kept) = atlas.take_if(|kept| kept.fits(format, width, height)) {
        return Ok(atlas.insert(kept));
    }
    *atlas = None;
    Ok(atlas.insert(LensAtlas::new(device, format, width, height, max_side)?))
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
    /// Draws a batch of lenses over what the back buffer holds, or says it
    /// must draw flat. Leaves the back buffer bound with the frame's
    /// viewport, whatever happens.
    pub(super) fn draw_lenses(&mut self, lenses: &[BackdropLens]) -> Result<Glass> {
        if lenses.is_empty() {
            return Ok(Glass::Drawn);
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
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: a plain query of the live back buffer.
        unsafe { target.GetDesc(&mut desc) };
        // A copy with a source box needs a single-sample source; the
        // flip-model back buffer always is one, but refuse rather than copy
        // if that ever changes.
        if desc.SampleDesc.Count != 1 {
            return Ok(Glass::Flat);
        }
        let device = &devices.device;
        let context = &devices.device_context;
        let state = &mut self.pipelines.lenses;
        let Some(pipelines) = state.pipelines.get(device) else {
            return Ok(Glass::Flat);
        };
        let max_side = max_atlas_side(device);
        let Some(packing) = LensPacking::pack(lenses, (desc.Width, desc.Height), max_side) else {
            return Ok(Glass::Flat);
        };
        let atlas = atlas(
            &mut state.atlas,
            device,
            desc.Format,
            packing.width,
            packing.height,
            max_side,
        )?;
        let count = u32::try_from(lenses.len()).context("too many lenses in one batch")?;

        pipelines.lens.update_buffer(device, context, lenses)?;
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
        pipelines.slots.write(device, context, &slots)?;
        // Level k's slots start at (k - 1) * count.
        let level_slots: Vec<LensDownsampleSlot> = (1..LEVELS)
            .flat_map(|level| {
                packing
                    .copies
                    .iter()
                    .map(move |copy| LensDownsampleSlot::new(copy, level))
            })
            .collect();
        pipelines
            .downsample
            .update_buffer(device, context, &level_slots)?;

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
            &packing.copies,
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

/// Copies the footprints into the atlas, fills its levels and draws the
/// lenses over the back buffer. The caller restores the bindings after, as
/// an error can leave an atlas level bound.
fn draw_glass(
    context: &ID3D11DeviceContext,
    batch_params: &ID3D11Buffer,
    pipelines: &LensPipelines,
    atlas: &LensAtlas,
    frame: &Frame<'_>,
    copies: &[LensCopy],
    count: u32,
) -> Result<()> {
    // SAFETY: unbinding the back buffer before it is read; every source box
    // lies inside it and every destination inside the atlas's level 0
    // (`LensPacking::pack` clipped the footprints to the target and sized
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
            level.saturating_mul(count),
            count,
        )?;
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
