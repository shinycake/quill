// Added by the Quill project (2026). See third_party/gpui-pre-wgpu/QUILL-CHANGES.md.
//! NV12 video pictures for the wgpu renderer, as gpui-pre-windows'
//! `yuv_surfaces` does for DirectX: each `PaintSurface` whose frame is
//! `SurfaceFormat::Nv12` is uploaded into two textures (luma as `R8Unorm`,
//! the interleaved chroma as `Rg8Unorm` at half size) and drawn with the
//! `vs_surface` / `fs_surface` shaders, which convert to RGB on the GPU with
//! the frame's matrix and range (`YuvColor::rgb_rows`, in the per-draw
//! uniform). A playing clip re-uploads only when its picture changes, into
//! the same textures while its size stays the same.
//!
//! Each frame learns what happened to it (`SurfaceFrame::mark_drawn` /
//! `mark_refused`): a frame this renderer can't draw (another format, too
//! large for a texture, more surfaces than [`MAX_DRAWS`]) is refused and its
//! producer shows it another way.

use std::num::NonZeroU64;
use std::ops::Range;

use bytemuck::{Pod, Zeroable};
use gpui::{Bounds, PaintSurface, ScaledPixels, SurfaceFormat, SurfaceFrame};

/// Texture pairs kept at most (one per clip on screen).
const MAX_SLOTS: usize = 16;

/// A slot not drawn for this many frames is freed (its clip went away).
const IDLE_FRAMES: u64 = 2;

/// Surfaces drawn in one renderer frame at most: the uniform buffer holds
/// this many draws' parameters.
const MAX_DRAWS: usize = 64;

/// `SurfaceParams` in `shaders.wgsl`.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct SurfaceParams {
    bounds: [f32; 4],
    content_mask: [f32; 4],
    /// `YuvColor::rgb_rows`: R, G and B as `dot(row, (y, cb, cr, 1))`.
    rows: [[f32; 4]; 3],
}

pub(crate) const SURFACE_PARAMS_SIZE: u64 = std::mem::size_of::<SurfaceParams>() as u64;

const _: () = assert!(SURFACE_PARAMS_SIZE == 80);

fn pod_bounds(bounds: Bounds<ScaledPixels>) -> [f32; 4] {
    [
        bounds.origin.x.0,
        bounds.origin.y.0,
        bounds.size.width.0,
        bounds.size.height.0,
    ]
}

/// The two textures of one clip's picture.
struct Slot {
    width: u32,
    height: u32,
    luma: wgpu::Texture,
    chroma: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    /// The frame whose pixels the textures hold.
    frame_id: u64,
    /// The renderer frame it was last drawn in.
    drawn: u64,
}

pub(crate) struct YuvSurfaces {
    sampler: wgpu::Sampler,
    params: wgpu::Buffer,
    /// Bytes between two draws' parameters (the uniform offset alignment).
    params_stride: u64,
    slots: Vec<Slot>,
    /// Counts renderer frames ([`Self::end_frame`]).
    frame: u64,
    /// Per surface of the scene being drawn: its slot and uniform offset,
    /// `None` when it was refused ([`Self::prepare`]).
    draws: Vec<Option<(usize, u32)>>,
}

impl YuvSurfaces {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        // Bilinear and clamped, so the edges don't pick up the opposite side.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("yuv_surface_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        let params_stride = SURFACE_PARAMS_SIZE.next_multiple_of(alignment.max(1));
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("yuv_surface_params"),
            size: params_stride * MAX_DRAWS as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            sampler,
            params,
            params_stride,
            slots: Vec::new(),
            frame: 0,
            draws: Vec::new(),
        }
    }

    /// Before the render pass: upload the pictures of the scene's surfaces
    /// that are new and write each draw's parameters. Every frame is marked
    /// drawn or refused here.
    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        surfaces: &[PaintSurface],
    ) {
        self.draws.clear();
        if surfaces.is_empty() {
            return;
        }
        let in_scene: Vec<u64> = surfaces
            .iter()
            .filter(|surface| surface.frame.format() == SurfaceFormat::Nv12)
            .map(|surface| surface.frame.id())
            .collect();
        let mut params = Vec::new();
        for (index, surface) in surfaces.iter().enumerate() {
            let frame = &surface.frame;
            let slot = (index < MAX_DRAWS && frame.format() == SurfaceFormat::Nv12)
                .then(|| self.slot_for(device, queue, layout, frame, &in_scene))
                .flatten();
            let Some(slot) = slot else {
                frame.mark_refused();
                self.draws.push(None);
                continue;
            };
            let offset = index as u64 * self.params_stride;
            params.resize(offset as usize, 0);
            params.extend_from_slice(bytemuck::bytes_of(&SurfaceParams {
                bounds: pod_bounds(surface.bounds),
                content_mask: pod_bounds(surface.content_mask.bounds),
                rows: frame.yuv().rgb_rows(),
            }));
            self.draws.push(Some((slot, offset as u32)));
            frame.mark_drawn();
        }
        if !params.is_empty() {
            queue.write_buffer(&self.params, 0, &params);
        }
    }

    /// Draw the scene's surfaces in `range` (as [`Self::prepare`] left them).
    pub(crate) fn draw(
        &self,
        range: Range<usize>,
        pipeline: &wgpu::RenderPipeline,
        globals: &wgpu::BindGroup,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        for index in range {
            let Some(Some((slot, offset))) = self.draws.get(index) else {
                continue;
            };
            let Some(slot) = self.slots.get(*slot) else {
                continue;
            };
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, globals, &[]);
            pass.set_bind_group(1, &slot.bind_group, &[*offset]);
            pass.draw(0..4, 0..1);
        }
    }

    /// The renderer finished a frame: free the textures of clips that were
    /// not drawn lately, and all of them once no surface is on screen (the
    /// viewer closed).
    pub(crate) fn end_frame(&mut self, any_surfaces: bool) {
        let frame = self.frame;
        if any_surfaces {
            self.slots
                .retain(|slot| frame.saturating_sub(slot.drawn) < IDLE_FRAMES);
        } else {
            self.slots.clear();
        }
        self.frame = frame.wrapping_add(1);
    }

    /// Texture pairs held, for tests.
    #[cfg(test)]
    pub(crate) fn slot_count(&self) -> usize {
        self.slots.len()
    }

    /// The slot holding `frame`'s pixels: the one that already does, else
    /// one of the same size whose frame is not in this scene (uploading
    /// into it), else a new one. `None` (logged) for a frame too large for
    /// a texture or whose planes don't match its size.
    fn slot_for(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        frame: &SurfaceFrame,
        in_scene: &[u64],
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
        let (width, height) = (frame.width(), frame.height());
        let max_side = device.limits().max_texture_dimension_2d;
        if width > max_side || height > max_side {
            log::warn!("NV12 frame of {width}x{height} is larger than a texture can be");
            return None;
        }
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
                let slot = self.new_slot(device, layout, width, height);
                self.slots.push(slot);
                self.slots.len().checked_sub(1)?
            }
        };
        let slot = self.slots.get_mut(index)?;
        if !upload(queue, slot, frame) {
            log::warn!("NV12 frame {width}x{height} doesn't match its planes");
            return None;
        }
        slot.frame_id = frame.id();
        slot.drawn = frame_now;
        Some(index)
    }

    fn new_slot(
        &self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        width: u32,
        height: u32,
    ) -> Slot {
        let texture = |label, width, height, format| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            })
        };
        let luma = texture("yuv_luma", width, height, wgpu::TextureFormat::R8Unorm);
        let chroma = texture(
            "yuv_chroma",
            width / 2,
            height / 2,
            wgpu::TextureFormat::Rg8Unorm,
        );
        let luma_view = luma.create_view(&wgpu::TextureViewDescriptor::default());
        let chroma_view = chroma.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("yuv_surface_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.params,
                        offset: 0,
                        size: NonZeroU64::new(SURFACE_PARAMS_SIZE),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&luma_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&chroma_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        Slot {
            width,
            height,
            luma,
            chroma,
            bind_group,
            frame_id: 0,
            drawn: 0,
        }
    }
}

/// Copy `frame`'s planes into the slot's textures (of the frame's size).
/// `false` when the frame's planes don't match its size (never expected:
/// `SurfaceFrame::new` checked the length).
fn upload(queue: &wgpu::Queue, slot: &Slot, frame: &SurfaceFrame) -> bool {
    let Some((luma, chroma)) = frame.nv12_planes() else {
        return false;
    };
    let (width, height) = (frame.width(), frame.height());
    if slot.width != width || slot.height != height {
        return false;
    }
    let plane = |texture: &wgpu::Texture, bytes: &[u8], texels: (u32, u32)| {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            // Both planes are `width` bytes a row: luma one byte a texel,
            // chroma two at half the width.
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: texels.0,
                height: texels.1,
                depth_or_array_layers: 1,
            },
        );
    };
    plane(&slot.luma, luma, (width, height));
    plane(&slot.chroma, chroma, (width / 2, height / 2));
    true
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use crate::WgpuHeadlessRenderer;
    use gpui::{
        Bounds, ContentMask, DevicePixels, PaintSurface, PlatformHeadlessRenderer, Point, Scene,
        Size, SurfaceDrawState, SurfaceFormat, SurfaceFrame, YuvColor, YuvMatrix,
    };
    use std::sync::Arc;

    const LIMITED_709: YuvColor = YuvColor {
        matrix: YuvMatrix::Bt709,
        full_range: false,
    };

    /// An NV12 frame of `width` x `height` whose left half has luma `left`
    /// and right half `right`, all with chroma (`cb`, `cr`).
    fn frame(width: u32, height: u32, (left, right): (u8, u8), (cb, cr): (u8, u8)) -> Arc<SurfaceFrame> {
        let mut bytes = Vec::new();
        for _ in 0..height {
            for x in 0..width {
                bytes.push(if x < width / 2 { left } else { right });
            }
        }
        for _ in 0..height / 2 {
            for _ in 0..width / 2 {
                bytes.extend([cb, cr]);
            }
        }
        let frame = SurfaceFrame::new(width, height, SurfaceFormat::Nv12, Arc::from(bytes))
            .expect("valid NV12 frame");
        Arc::new(frame.with_yuv(LIMITED_709))
    }

    fn scene_with(frames: &[(Arc<SurfaceFrame>, f32)]) -> Scene {
        let mut scene = Scene::default();
        for (frame, x) in frames {
            let bounds = Bounds {
                origin: Point {
                    x: (*x).into(),
                    y: 0.0.into(),
                },
                size: Size {
                    width: 32.0.into(),
                    height: 16.0.into(),
                },
            };
            scene.insert_primitive(PaintSurface {
                order: 0,
                bounds,
                content_mask: ContentMask { bounds },
                frame: frame.clone(),
            });
        }
        scene.finish();
        scene
    }

    fn size(width: i32, height: i32) -> Size<DevicePixels> {
        Size {
            width: DevicePixels(width),
            height: DevicePixels(height),
        }
    }

    fn assert_near(image: &image::RgbaImage, x: u32, y: u32, expected: [u8; 3]) {
        let actual = image.get_pixel(x, y).0;
        assert!(
            actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 4),
            "pixel ({x}, {y}) was {actual:?}, expected {expected:?}"
        );
    }

    #[test]
    fn an_nv12_picture_shows_in_its_own_colors() -> anyhow::Result<()> {
        let mut renderer = WgpuHeadlessRenderer::new()?;
        // Limited range: 235 is white, 16 black.
        let grey = frame(16, 8, (235, 16), (128, 128));
        // Pure red and pure blue in BT.709 code values.
        let red = frame(16, 8, (63, 63), (102, 240));
        let blue = frame(16, 8, (32, 32), (240, 118));
        let scene = scene_with(&[(grey.clone(), 0.0), (red.clone(), 32.0), (blue.clone(), 64.0)]);
        let image = renderer.render_scene_to_image(&scene, size(96, 16))?;
        assert_near(&image, 4, 8, [255, 255, 255]);
        assert_near(&image, 28, 8, [0, 0, 0]);
        assert_near(&image, 48, 8, [255, 0, 0]);
        assert_near(&image, 80, 8, [0, 0, 255]);
        for frame in [grey, red, blue] {
            assert_eq!(frame.draw_state(), SurfaceDrawState::Drawn);
        }
        Ok(())
    }

    #[test]
    fn a_playing_clip_keeps_its_textures_until_it_leaves() -> anyhow::Result<()> {
        let mut renderer = WgpuHeadlessRenderer::new()?;
        let first = frame(16, 8, (235, 235), (128, 128));
        renderer.render_scene(&scene_with(&[(first, 0.0)]), size(32, 16))?;
        assert_eq!(renderer.yuv_slot_count(), 1);
        // The next picture of the same size goes into the same textures.
        let second = frame(16, 8, (16, 16), (128, 128));
        let image = renderer.render_scene_to_image(&scene_with(&[(second, 0.0)]), size(32, 16))?;
        assert_eq!(renderer.yuv_slot_count(), 1);
        assert_near(&image, 16, 8, [0, 0, 0]);
        // The viewer closed: nothing is kept.
        renderer.render_scene(&scene_with(&[]), size(32, 16))?;
        assert_eq!(renderer.yuv_slot_count(), 0);
        Ok(())
    }

    #[test]
    fn a_frame_it_cannot_draw_is_refused() -> anyhow::Result<()> {
        let mut renderer = WgpuHeadlessRenderer::new()?;
        let hdr = Arc::new(SurfaceFrame::new(
            2,
            2,
            SurfaceFormat::Rgba16Float,
            Arc::from(vec![0u8; 32]),
        )?);
        renderer.render_scene(&scene_with(&[(hdr.clone(), 0.0)]), size(32, 16))?;
        assert_eq!(hdr.draw_state(), SurfaceDrawState::Refused);
        assert_eq!(renderer.yuv_slot_count(), 0);
        Ok(())
    }
}
