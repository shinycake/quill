// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.
//! HDR stills for the DirectX renderer: `PaintSurface` frames (linear
//! half-float pixels) drawn through the `hdr_surface` shaders, and the
//! swap chain's switch to scRGB while a window asks for HDR output
//! ([`crate::hdr_output`]).

use std::slice;

use anyhow::{Context, Result};
use gpui::{Bounds, PaintSurface, ScaledPixels, SurfaceFormat, SurfaceFrame};
use gpui_util::ResultExt;
use windows::Win32::Graphics::{
    Direct3D11::*,
    Dxgi::{Common::*, *},
};
use windows::core::Interface;

use super::shader_resources::ShaderModule;
use super::{
    BUFFER_COUNT, DirectXRenderer, DirectXRendererDevices, PipelineState, RENDER_TARGET_FORMAT,
    create_blend_state,
};
use crate::hdr_output;

/// Uploaded stills kept on the GPU: the one on screen and the one before
/// it, so paging back and forth doesn't re-upload.
const CACHED_TEXTURES: usize = 2;

/// Direct3D 11's largest 2D texture side.
const MAX_TEXTURE_SIDE: u32 = D3D11_REQ_TEXTURE2D_U_OR_V_DIMENSION;

/// One `HdrSurface` in `shaders.hlsl`.
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub(super) struct HdrSurfaceInstance {
    bounds: Bounds<ScaledPixels>,
    content_mask: Bounds<ScaledPixels>,
    headroom: f32,
    _pad: [u32; 3],
}

const _: () = assert!(std::mem::size_of::<HdrSurfaceInstance>() == 48);

pub(super) struct HdrSurfaces {
    pipeline: PipelineState<HdrSurfaceInstance>,
    sampler: Option<ID3D11SamplerState>,
    /// Least recently drawn first.
    textures: Vec<(u64, Option<ID3D11ShaderResourceView>)>,
}

impl HdrSurfaces {
    pub(super) fn new(device: &ID3D11Device) -> Result<Self> {
        let pipeline = PipelineState::new(
            device,
            "hdr_surface_pipeline",
            ShaderModule::HdrSurface,
            1,
            create_blend_state(device)?,
        )?;
        // Clamped, so the edges don't pick up the opposite side, and
        // trilinear over the mip chain, so a large still scaled down stays
        // smooth.
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
            textures: Vec::with_capacity(CACHED_TEXTURES),
        })
    }

    pub(super) fn draw(
        &mut self,
        devices: &DirectXRendererDevices,
        surfaces: &[PaintSurface],
    ) -> Result<()> {
        for surface in surfaces {
            let Some(view) = self.texture(devices, &surface.frame)? else {
                continue;
            };
            self.pipeline.update_buffer(
                &devices.device,
                &devices.device_context,
                &[HdrSurfaceInstance {
                    bounds: surface.bounds,
                    content_mask: surface.content_mask.bounds,
                    headroom: surface.frame.headroom(),
                    _pad: [0; 3],
                }],
            )?;
            self.pipeline.draw_with_texture(
                &devices.device_context,
                slice::from_ref(&Some(view)),
                slice::from_ref(&self.sampler),
                1,
            )?;
        }
        Ok(())
    }

    /// Free every uploaded still.
    pub(super) fn release(&mut self) {
        self.textures.clear();
    }

    /// The frame's texture, uploading it on first use.
    fn texture(
        &mut self,
        devices: &DirectXRendererDevices,
        frame: &SurfaceFrame,
    ) -> Result<Option<ID3D11ShaderResourceView>> {
        if let Some(position) = self.textures.iter().position(|(id, _)| *id == frame.id()) {
            let entry = self.textures.remove(position);
            let view = entry.1.clone();
            self.textures.push(entry);
            return Ok(view);
        }
        let view = upload(devices, frame)?;
        if self.textures.len() >= CACHED_TEXTURES && !self.textures.is_empty() {
            self.textures.remove(0);
        }
        self.textures.push((frame.id(), view.clone()));
        Ok(view)
    }
}

/// A shader-readable texture of the frame's pixels with a full mip chain.
fn upload(
    devices: &DirectXRendererDevices,
    frame: &SurfaceFrame,
) -> Result<Option<ID3D11ShaderResourceView>> {
    // Four half floats a pixel.
    let (format, bytes_per_pixel) = match frame.format() {
        SurfaceFormat::Rgba16Float => (DXGI_FORMAT_R16G16B16A16_FLOAT, 8),
        // Drawn by `yuv_surfaces`, never handed here.
        SurfaceFormat::Nv12 => return Ok(None),
    };
    let (width, height) = (frame.width(), frame.height());
    if width > MAX_TEXTURE_SIDE || height > MAX_TEXTURE_SIDE {
        log::warn!("HDR still of {width}x{height} is larger than a texture can be");
        return Ok(None);
    }
    let row_pitch = width
        .checked_mul(bytes_per_pixel)
        .context("HDR still row too long")?;
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        // Zero asks for the whole chain.
        MipLevels: 0,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        // `GenerateMips` renders into the smaller levels.
        BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
        CPUAccessFlags: 0,
        MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
    };
    let mut texture = None;
    // SAFETY: a valid descriptor; the call writes the out parameter.
    unsafe { devices.device.CreateTexture2D(&desc, None, Some(&mut texture))? };
    let texture = texture.context("creating the HDR still texture")?;
    let mut view = None;
    // SAFETY: as above, for a view of the texture just made.
    unsafe {
        devices
            .device
            .CreateShaderResourceView(&texture, None, Some(&mut view))?
    };
    // SAFETY: `SurfaceFrame::new` checked that `bytes` holds exactly
    // `height` tightly packed rows of `row_pitch` bytes, which is what the
    // copy into mip level 0 reads.
    unsafe {
        devices.device_context.UpdateSubresource(
            &texture,
            0,
            None,
            frame.bytes().as_ptr().cast(),
            row_pitch,
            0,
        );
        if let Some(view) = &view {
            devices.device_context.GenerateMips(view);
        }
    }
    Ok(view)
}

/// What the swap chain holds: 8-bit sRGB, or scRGB half floats while an HDR
/// still is on screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct OutputMode {
    pub(super) hdr: bool,
    /// SDR white in scRGB units (1.0 in SDR).
    pub(super) scale: f32,
}

impl OutputMode {
    pub(super) const SDR: Self = Self {
        hdr: false,
        scale: 1.0,
    };

    pub(super) fn format(self) -> DXGI_FORMAT {
        if self.hdr {
            DXGI_FORMAT_R16G16B16A16_FLOAT
        } else {
            RENDER_TARGET_FORMAT
        }
    }

    fn color_space(self) -> DXGI_COLOR_SPACE_TYPE {
        if self.hdr {
            DXGI_COLOR_SPACE_RGB_FULL_G10_NONE_P709
        } else {
            DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709
        }
    }

    /// The opaque window background: white, at SDR white.
    pub(super) fn opaque_clear(self) -> [f32; 4] {
        [self.scale, self.scale, self.scale, 1.0]
    }
}

impl DirectXRenderer {
    /// Switch the swap chain to scRGB while the window asks for HDR output
    /// and its display is in HDR mode, and back otherwise. A failed switch
    /// is not retried until the window releases HDR output.
    pub(super) fn sync_output_mode(&mut self) {
        // A switch that failed after resizing the buffers can leave no
        // render target; rebuild it at the current format.
        if self
            .resources
            .as_ref()
            .is_some_and(|resources| resources.render_target_view.is_none())
        {
            self.switch_output(self.output).log_err();
        }
        let wanted = hdr_output::wanted(self.hwnd);
        if !wanted && self.hdr_failed {
            self.hdr_failed = false;
            hdr_output::note_failed(self.hwnd, false);
        }
        let target = if wanted && !self.hdr_failed {
            let info = hdr_output::display_info(self.hwnd.0 as isize);
            if info.hdr_enabled {
                OutputMode {
                    hdr: true,
                    scale: info.output_scale(),
                }
            } else {
                OutputMode::SDR
            }
        } else {
            OutputMode::SDR
        };
        if target.hdr == self.output.hdr {
            self.output.scale = target.scale;
            return;
        }
        if let Err(error) = self.switch_output(target) {
            log::warn!("switching HDR output failed: {error:#}");
            self.hdr_failed = true;
            hdr_output::note_failed(self.hwnd, true);
            if self.output.hdr {
                self.switch_output(OutputMode::SDR).log_err();
            }
        }
    }

    fn switch_output(&mut self, mode: OutputMode) -> Result<()> {
        let devices = self.devices.as_ref().context("devices missing")?;
        let resources = self.resources.as_mut().context("resources missing")?;
        // SAFETY: unbinding the render targets before the buffers resize.
        unsafe { devices.device_context.OMSetRenderTargets(None, None) };
        resources.render_target.take();
        resources.render_target_view.take();
        // SAFETY: no views of the swap chain's buffers are alive.
        let resized = unsafe {
            resources.swap_chain.ResizeBuffers(
                BUFFER_COUNT as u32,
                self.width,
                self.height,
                mode.format(),
                DXGI_SWAP_CHAIN_FLAG(0),
            )
        }
        .context("resizing the swap chain buffers");
        let colored = resized.and_then(|()| {
            self.output = mode;
            let swap_chain: IDXGISwapChain3 = resources.swap_chain.cast()?;
            // SAFETY: plain queries and a setter on a live swap chain.
            unsafe {
                let support = swap_chain.CheckColorSpaceSupport(mode.color_space())?;
                anyhow::ensure!(
                    support & DXGI_SWAP_CHAIN_COLOR_SPACE_SUPPORT_FLAG_PRESENT.0 as u32 != 0,
                    "the swap chain can't present {:?}",
                    mode.color_space()
                );
                swap_chain.SetColorSpace1(mode.color_space())?;
            }
            Ok(())
        });
        // Whatever happened above, the buffers need views again.
        resources.recreate_resources(devices, self.width, self.height)?;
        // SAFETY: binding the views just made.
        unsafe {
            devices
                .device_context
                .OMSetRenderTargets(Some(slice::from_ref(&resources.render_target_view)), None);
        }
        colored
    }
}
