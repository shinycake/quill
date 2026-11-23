// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.
//! NV12 video pictures for the DirectX renderer: each `PaintSurface` whose
//! frame is `SurfaceFormat::Nv12` is uploaded into two textures (luma as
//! `R8_UNORM`, the interleaved chroma as `R8G8_UNORM` at half size) and
//! drawn with the `yuv_surface` shaders, which convert to RGB on the GPU
//! with the frame's matrix and range (`YuvColor::rgb_rows`, carried in the
//! per-surface instance data). A playing clip re-uploads only when its
//! picture changes, into the same textures while its size stays the same.
//!
//! Nothing here fails a frame. Each frame learns what happened to it
//! (`SurfaceFrame::mark_drawn` / `mark_refused`): a frame this renderer
//! can't draw (textures that can't be made, a frame too large for a
//! texture) is refused and its producer shows it another way. After a
//! texture failure this renderer refuses NV12 for [`RETRY_FRAMES`] frames,
//! then tries again. The answer is per renderer, so per window.
//!
//! Textures have one mip level and are sampled bilinearly: a picture drawn
//! much smaller than its texture aliases. The producer decodes into the
//! box it is shown in, so the texture stays close to the drawn size.

use std::slice;
use std::time::Instant;

use anyhow::Result;
use gpui::{Bounds, PaintSurface, ScaledPixels, SurfaceFormat, SurfaceFrame};
use windows::Win32::Graphics::{Direct3D11::*, Dxgi::Common::*};

use super::shader_resources::ShaderModule;
use super::{DirectXRendererDevices, PipelineState, create_blend_state};
use crate::surface_support;

/// Direct3D 11's largest 2D texture side.
const MAX_TEXTURE_SIDE: u32 = D3D11_REQ_TEXTURE2D_U_OR_V_DIMENSION;

/// Texture pairs kept at most (one per clip on screen; a chat can show a
/// few playing tiles at once).
const MAX_SLOTS: usize = 16;

/// A slot not drawn for this many frames is freed (its clip went away).
const IDLE_FRAMES: u64 = 2;

/// After a texture failure, NV12 frames are refused for this many frames
/// (about two seconds at 60 Hz) before textures are tried again.
const RETRY_FRAMES: u64 = 120;

/// One `YuvSurface` in `shaders.hlsl`.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub(super) struct YuvSurfaceInstance {
    bounds: Bounds<ScaledPixels>,
    content_mask: Bounds<ScaledPixels>,
    /// `YuvColor::rgb_rows`: R, G and B as `dot(row, (y, cb, cr, 1))`.
    rows: [[f32; 4]; 3],
}

const _: () = assert!(std::mem::size_of::<YuvSurfaceInstance>() == 80);

/// The two textures of one clip's picture.
struct Slot {
    width: u32,
    height: u32,
    luma: ID3D11Texture2D,
    luma_view: Option<ID3D11ShaderResourceView>,
    chroma: ID3D11Texture2D,
    chroma_view: Option<ID3D11ShaderResourceView>,
    /// The frame whose pixels the textures hold.
    frame_id: u64,
    /// The renderer frame it was last drawn in.
    drawn: u64,
}

pub(super) struct YuvSurfaces {
    pipeline: PipelineState<YuvSurfaceInstance>,
    sampler: Option<ID3D11SamplerState>,
    slots: Vec<Slot>,
    /// Counts renderer frames ([`Self::end_frame`]).
    frame: u64,
    /// The NV12 frames this renderer frame draws ([`Self::begin_frame`]):
    /// their slots are not handed to other frames.
    in_scene: Vec<u64>,
    /// NV12 frames are refused until this frame (after a texture failure).
    retry_at: u64,
}

impl YuvSurfaces {
    pub(super) fn new(device: &ID3D11Device) -> Result<Self> {
        let pipeline = PipelineState::new(
            device,
            "yuv_surface_pipeline",
            ShaderModule::YuvSurface,
            1,
            create_blend_state(device)?,
        )?;
        // Bilinear and clamped, so the edges don't pick up the opposite side.
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
            pipeline,
            sampler,
            slots: Vec::new(),
            frame: 0,
            in_scene: Vec::new(),
            retry_at: 0,
        })
    }

    /// A renderer frame starts with these surfaces: note the NV12 frames in
    /// it, so a slot holding one of them is not reused for another.
    pub(super) fn begin_frame(&mut self, surfaces: &[PaintSurface]) {
        self.in_scene.clear();
        self.in_scene.extend(
            surfaces
                .iter()
                .filter(|surface| surface.frame.format() == SurfaceFormat::Nv12)
                .map(|surface| surface.frame.id()),
        );
    }

    /// Draw one NV12 surface (uploading its picture if it is new). The frame
    /// is marked drawn, or refused when it can't be.
    pub(super) fn draw(
        &mut self,
        devices: &DirectXRendererDevices,
        surface: &PaintSurface,
    ) -> Result<()> {
        let Some(index) = self.slot_for(devices, &surface.frame) else {
            surface.frame.mark_refused();
            return Ok(());
        };
        let Some(slot) = self.slots.get(index) else {
            surface.frame.mark_refused();
            return Ok(());
        };
        let (luma, chroma) = (slot.luma_view.clone(), slot.chroma_view.clone());
        self.pipeline.update_buffer(
            &devices.device,
            &devices.device_context,
            &[YuvSurfaceInstance {
                bounds: surface.bounds,
                content_mask: surface.content_mask.bounds,
                rows: surface.frame.yuv().rgb_rows(),
            }],
        )?;
        // t0 is the luma (bound by `draw_with_texture`), t1 the instance
        // buffer, t2 the chroma.
        // SAFETY: binding a live view to a pixel shader slot.
        unsafe {
            devices
                .device_context
                .PSSetShaderResources(2, Some(slice::from_ref(&chroma)));
        }
        let drawn = self.pipeline.draw_with_texture(
            &devices.device_context,
            slice::from_ref(&luma),
            slice::from_ref(&self.sampler),
            1,
        );
        // Unbind the chroma so no later pipeline samples it by accident.
        // SAFETY: clearing a pixel shader slot.
        unsafe {
            devices
                .device_context
                .PSSetShaderResources(2, Some(slice::from_ref(&None)));
        }
        drawn?;
        surface.frame.mark_drawn();
        Ok(())
    }

    /// The renderer finished a frame: free the textures of clips that were
    /// not drawn lately.
    pub(super) fn end_frame(&mut self) {
        let frame = self.frame;
        self.slots
            .retain(|slot| frame.saturating_sub(slot.drawn) < IDLE_FRAMES);
        self.frame = frame.wrapping_add(1);
    }

    /// Free every texture.
    pub(super) fn release(&mut self) {
        self.slots.clear();
    }

    /// The slot holding `frame`'s pixels: the one that already does, else
    /// one of the same size whose frame is not in this scene (uploading
    /// into it), else a new one. `None` (logged) while retrying after a
    /// texture failure, when textures can't be made or the upload fails,
    /// and for a frame that isn't NV12 or is too large for a texture.
    fn slot_for(
        &mut self,
        devices: &DirectXRendererDevices,
        frame: &SurfaceFrame,
    ) -> Option<usize> {
        let frame_now = self.frame;
        if let Some(index) = self
            .slots
            .iter()
            .position(|slot| slot.frame_id == frame.id())
        {
            if let Some(slot) = self.slots.get_mut(index) {
                slot.drawn = frame_now;
            }
            return Some(index);
        }
        if frame_now < self.retry_at {
            return None;
        }
        let (width, height) = (frame.width(), frame.height());
        if width > MAX_TEXTURE_SIDE || height > MAX_TEXTURE_SIDE {
            log::warn!("NV12 frame of {width}x{height} is larger than a texture can be");
            return None;
        }
        let in_scene = &self.in_scene;
        let reusable = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| {
                slot.width == width
                    && slot.height == height
                    && slot.drawn != frame_now
                    && !in_scene.contains(&slot.frame_id)
            })
            .min_by_key(|(_, slot)| slot.drawn)
            .map(|(index, _)| index);
        let index = match reusable {
            Some(index) => index,
            None => {
                let slot = match new_slot(&devices.device, width, height) {
                    Ok(slot) => slot,
                    Err(error) => {
                        log::warn!(
                            "NV12 textures of {width}x{height} failed, retrying in \
                             {RETRY_FRAMES} frames: {error:#}"
                        );
                        self.retry_at = frame_now.saturating_add(RETRY_FRAMES);
                        return None;
                    }
                };
                if self.slots.len() >= MAX_SLOTS {
                    // The least recently drawn goes, one outside the scene
                    // if there is one.
                    let evict = |pick_in_scene: bool| {
                        self.slots
                            .iter()
                            .enumerate()
                            .filter(|(_, slot)| pick_in_scene || !in_scene.contains(&slot.frame_id))
                            .min_by_key(|(_, slot)| slot.drawn)
                            .map(|(index, _)| index)
                    };
                    if let Some(oldest) = evict(false).or_else(|| evict(true)) {
                        self.slots.swap_remove(oldest);
                    }
                }
                self.slots.push(slot);
                self.slots.len().checked_sub(1)?
            }
        };
        let slot = self.slots.get_mut(index)?;
        if !upload(&devices.device_context, slot, frame) {
            log::warn!("NV12 frame {width}x{height} doesn't match its planes");
            return None;
        }
        slot.frame_id = frame.id();
        slot.drawn = frame_now;
        Some(index)
    }
}

/// A texture of `format` at `width` x `height` and its shader view.
fn texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
    format: DXGI_FORMAT,
) -> Result<(ID3D11Texture2D, Option<ID3D11ShaderResourceView>)> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut texture = None;
    // SAFETY: a valid descriptor; the call writes the out parameter.
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture))? };
    let texture = texture.ok_or_else(|| anyhow::anyhow!("no texture"))?;
    let mut view = None;
    // SAFETY: as above, for a view of the texture just made.
    unsafe { device.CreateShaderResourceView(&texture, None, Some(&mut view))? };
    Ok((texture, view))
}

fn new_slot(device: &ID3D11Device, width: u32, height: u32) -> Result<Slot> {
    let (luma, luma_view) = texture(device, width, height, DXGI_FORMAT_R8_UNORM)?;
    let (chroma, chroma_view) = texture(device, width / 2, height / 2, DXGI_FORMAT_R8G8_UNORM)?;
    Ok(Slot {
        width,
        height,
        luma,
        luma_view,
        chroma,
        chroma_view,
        frame_id: 0,
        drawn: 0,
    })
}

/// Copy `frame`'s planes into the slot's textures (of the frame's size).
/// `false` when the frame's planes don't match its size (never expected:
/// `SurfaceFrame::new` checked the length).
fn upload(context: &ID3D11DeviceContext, slot: &Slot, frame: &SurfaceFrame) -> bool {
    let Some((luma, chroma)) = frame.nv12_planes() else {
        return false;
    };
    let width = frame.width();
    if slot.width != width || slot.height != frame.height() {
        return false;
    }
    let started = surface_support::trace_uploads().then(Instant::now);
    // SAFETY: `nv12_planes` returned exactly `width * height` luma bytes
    // (`height` rows of `width` R8 texels, row pitch `width`) and
    // `width * height / 2` chroma bytes (`height / 2` rows of `width / 2`
    // R8G8 texels, row pitch `width`), which is what each copy into the
    // slot's textures of that size reads.
    unsafe {
        context.UpdateSubresource(&slot.luma, 0, None, luma.as_ptr().cast(), width, 0);
        context.UpdateSubresource(&slot.chroma, 0, None, chroma.as_ptr().cast(), width, 0);
    }
    if let Some(started) = started {
        surface_support::trace_upload("nv12", luma.len() + chroma.len(), started);
    }
    true
}
