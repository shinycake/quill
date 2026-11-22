// Added by the Quill project (2026). See third_party/gpui-pre-wgpu/QUILL-CHANGES.md.
//! QUILL: backdrop glass on wgpu (`PrimitiveBatch::Lenses`), as
//! gpui-pre-apple's `metal_renderer/lens.rs` draws it on Metal.
//!
//! A lens batch ends the frame's render pass, copies each lens's footprint
//! out of the target into a slot of a pooled atlas, fills the atlas's mip
//! levels with dual-Kawase downsamples, resumes the pass over what was
//! drawn and draws the lenses from the atlas (`fs_lens` in
//! shaders_lens.wgsl). A target that can't be read (no `COPY_SRC`), the
//! texture instance transport (WebGL), pipelines that did not build or
//! footprints too large for an atlas draw the batch as its fallback quads.
//!
//! A surface that can't be read (OpenGL's never can) is drawn through a
//! frame copy while glass is on screen: the frame renders into a readable
//! texture of the surface's size, which a last pass copies onto the
//! surface ([`WgpuRendererCore::glass_frame`]).

use super::{InstanceData, WgpuRendererCore};
use anyhow::{Result, anyhow};
use bytemuck::{Pod, Zeroable};
use gpui::{BackdropLens, LENS_ATLAS_LEVELS, LensCopy, LensPacking};
use std::num::NonZeroU64;

/// The lens shader over the shared definitions it uses (and the storage
/// transport those refer to).
const LENS_SHADERS: &str = concat!(
    include_str!("../shaders.wgsl"),
    include_str!("../shaders_storage.wgsl"),
    include_str!("../shaders_lens.wgsl"),
);

/// The downsample and frame copy passes.
const LENS_POST_SHADERS: &str = include_str!("../shaders_lens_post.wgsl");

/// Mip levels of the atlas (the packing gpui-pre does assumes this many).
const LEVELS: u32 = LENS_ATLAS_LEVELS;
/// The atlas's sides are multiples of this, so every level halves exactly.
const ATLAS_STEP: u32 = 256;
/// The largest atlas side asked for, on devices that allow more.
const MAX_SIDE: u32 = 16384;

// `BackdropLens` in shaders_lens.wgsl lays the struct out by WGSL's rules,
// where a `Bounds` (two vec2) aligns to 8 bytes. These are the offsets that
// alignment could move, and the stride of the storage array.
const _: () = {
    assert!(std::mem::size_of::<BackdropLens>() == 240);
    assert!(std::mem::offset_of!(BackdropLens, bounds) == 8);
    assert!(std::mem::offset_of!(BackdropLens, content_mask) == 24);
    assert!(std::mem::offset_of!(BackdropLens, body) == 40);
    assert!(std::mem::offset_of!(BackdropLens, color) == 96);
    assert!(std::mem::offset_of!(BackdropLens, light_mix) == 236);
};

/// Where one lens's backdrop lies in the target and in the atlas, device
/// pixels (`LensSlot` in shaders_lens.wgsl).
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
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

/// One slot of one downsample pass: the texels it writes in the level
/// being filled, and the texels of the level above it may read
/// (`LensDownsampleSlot` in shaders_lens_post.wgsl).
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
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

/// The pooled atlas: a mip-chained texture in the target's format, a view
/// of the whole chain (the lens samples it) and one of each level (a
/// downsample renders one level while it samples the one above).
struct LensAtlas {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    levels: Vec<wgpu::TextureView>,
}

impl LensAtlas {
    fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        max_side: u32,
    ) -> Self {
        // Rounded up, so a lens that grows a little keeps its atlas.
        let side = |v: u32| v.next_multiple_of(ATLAS_STEP).clamp(ATLAS_STEP, max_side);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("lens_atlas"),
            size: wgpu::Extent3d {
                width: side(width),
                height: side(height),
                depth_or_array_layers: 1,
            },
            mip_level_count: LEVELS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let levels = (0..LEVELS)
            .map(|level| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    label: Some("lens_atlas_level"),
                    base_mip_level: level,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        Self {
            texture,
            view,
            levels,
        }
    }

    fn fits(&self, format: wgpu::TextureFormat, width: u32, height: u32) -> bool {
        self.texture.format() == format
            && self.texture.width() >= width
            && self.texture.height() >= height
    }
}

/// The glass's pipelines and what binds to them, for one target format and
/// blend.
#[derive(Clone)]
struct LensPipelines {
    lens: wgpu::RenderPipeline,
    lens_layout: wgpu::BindGroupLayout,
    downsample: wgpu::RenderPipeline,
    downsample_layout: wgpu::BindGroupLayout,
    frame_copy: wgpu::RenderPipeline,
    frame_copy_layout: wgpu::BindGroupLayout,
    /// Linear between texels and levels, for the lens.
    atlas_sampler: wgpu::Sampler,
    /// Linear within one level, for the downsamples.
    level_sampler: wgpu::Sampler,
}

fn storage_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn texture_entry(binding: u32, visibility: wgpu::ShaderStages) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn sampler_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

impl LensPipelines {
    /// Builds them inside error scopes, so a shader the backend rejects is
    /// an error here instead of one on every later frame.
    fn build(
        device: &wgpu::Device,
        globals_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        premultiplied_alpha: bool,
    ) -> Result<Self> {
        let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipelines = Self::create(device, globals_layout, format, premultiplied_alpha);
        let validation = gpui::block_on(validation.pop());
        let internal = gpui::block_on(internal.pop());
        match validation.or(internal) {
            Some(error) => Err(anyhow!("{error}")),
            None => Ok(pipelines),
        }
    }

    fn create(
        device: &wgpu::Device,
        globals_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        premultiplied_alpha: bool,
    ) -> Self {
        let lens_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lens_shaders"),
            source: wgpu::ShaderSource::Wgsl(LENS_SHADERS.into()),
        });
        let post_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lens_post_shaders"),
            source: wgpu::ShaderSource::Wgsl(LENS_POST_SHADERS.into()),
        });
        let vertex_fragment = wgpu::ShaderStages::VERTEX_FRAGMENT;
        let lens_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lens_layout"),
            entries: &[
                storage_entry(4, vertex_fragment),
                storage_entry(5, wgpu::ShaderStages::FRAGMENT),
                texture_entry(6, wgpu::ShaderStages::FRAGMENT),
                sampler_entry(7),
            ],
        });
        let downsample_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lens_downsample_layout"),
            entries: &[
                storage_entry(0, vertex_fragment),
                texture_entry(1, vertex_fragment),
                sampler_entry(2),
            ],
        });
        let frame_copy_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lens_frame_copy_layout"),
            entries: &[texture_entry(3, wgpu::ShaderStages::FRAGMENT)],
        });
        let blend = if premultiplied_alpha {
            wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING
        } else {
            wgpu::BlendState::ALPHA_BLENDING
        };
        let pipeline = |name: &str,
                        layouts: &[Option<&wgpu::BindGroupLayout>],
                        module: &wgpu::ShaderModule,
                        entries: (&str, &str),
                        topology: wgpu::PrimitiveTopology,
                        blend: Option<wgpu::BlendState>| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some(name),
                bind_group_layouts: layouts,
                immediate_size: 0,
            });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(name),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some(entries.0),
                    buffers: &[],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(entries.1),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let lens = pipeline(
            "lenses",
            &[Some(globals_layout), Some(&lens_layout)],
            &lens_module,
            ("vs_lens", "fs_lens"),
            wgpu::PrimitiveTopology::TriangleStrip,
            Some(blend),
        );
        let downsample = pipeline(
            "lens_downsample",
            &[Some(&downsample_layout)],
            &post_module,
            ("vs_lens_downsample", "fs_lens_downsample"),
            wgpu::PrimitiveTopology::TriangleStrip,
            None,
        );
        let frame_copy = pipeline(
            "lens_frame_copy",
            &[Some(&frame_copy_layout)],
            &post_module,
            ("vs_lens_frame_copy", "fs_lens_frame_copy"),
            wgpu::PrimitiveTopology::TriangleList,
            None,
        );
        let sampler = |label: &str, mipmap_filter: wgpu::MipmapFilterMode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some(label),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter,
                ..Default::default()
            })
        };
        Self {
            lens,
            lens_layout,
            downsample,
            downsample_layout,
            frame_copy,
            frame_copy_layout,
            atlas_sampler: sampler("lens_atlas_sampler", wgpu::MipmapFilterMode::Linear),
            level_sampler: sampler("lens_level_sampler", wgpu::MipmapFilterMode::Nearest),
        }
    }
}

/// Whether the glass's pipelines exist. They are built the first time a
/// frame has glass, again when the blend changes (the window's
/// transparency), and never again after they fail (glass then draws flat).
#[derive(Default)]
enum Pipelines {
    #[default]
    Unbuilt,
    Built {
        premultiplied_alpha: bool,
        pipelines: Box<LensPipelines>,
    },
    Failed,
}

/// A readable texture the size of the surface, drawn into instead of a
/// surface that can't be read while glass is on screen.
struct FrameCopy {
    texture: wgpu::Texture,
}

/// What drawing glass keeps between frames. All of it is dropped with the
/// renderer's other resources on a device loss; the atlas goes after the
/// first frame without glass, the frame copy once the glass leaves the
/// screen, and both on a resize.
#[derive(Default)]
pub(super) struct LensState {
    pipelines: Pipelines,
    atlas: Option<LensAtlas>,
    frame_copy: Option<FrameCopy>,
}

impl LensState {
    /// Frees the atlas and the frame copy (the window was resized).
    pub(super) fn release(&mut self) {
        self.atlas = None;
        self.frame_copy = None;
    }

    fn pipelines(
        &mut self,
        device: &wgpu::Device,
        globals_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        premultiplied_alpha: bool,
    ) -> Option<LensPipelines> {
        match &self.pipelines {
            Pipelines::Built {
                premultiplied_alpha: built_for,
                pipelines,
            } if *built_for == premultiplied_alpha => return Some((**pipelines).clone()),
            Pipelines::Failed => return None,
            Pipelines::Unbuilt | Pipelines::Built { .. } => {}
        }
        match LensPipelines::build(device, globals_layout, format, premultiplied_alpha) {
            Ok(pipelines) => {
                self.pipelines = Pipelines::Built {
                    premultiplied_alpha,
                    pipelines: Box::new(pipelines.clone()),
                };
                Some(pipelines)
            }
            Err(error) => {
                log::error!("glass draws flat: its pipelines did not build: {error:#}");
                self.pipelines = Pipelines::Failed;
                None
            }
        }
    }

    fn atlas(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        max_side: u32,
    ) -> &LensAtlas {
        if let Some(atlas) = self
            .atlas
            .take_if(|atlas| atlas.fits(format, width, height))
        {
            return self.atlas.insert(atlas);
        }
        // Dropped, not destroyed: an earlier batch of this frame may still
        // name the old one in the unsubmitted encoder.
        self.atlas = None;
        self.atlas
            .insert(LensAtlas::new(device, format, width, height, max_side))
    }
}

/// A frame's allocation in the shared storage buffer.
struct StorageSlice {
    buffer: wgpu::Buffer,
    offset: u64,
    size: u64,
}

impl StorageSlice {
    fn binding(&self, binding: u32) -> wgpu::BindGroupEntry<'_> {
        wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: &self.buffer,
                offset: self.offset,
                size: NonZeroU64::new(self.size),
            }),
        }
    }
}

/// One downsample pass: the level it renders and what it reads.
struct DownsamplePass {
    target: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

/// Everything one lens batch draws with, gathered before the pass ends.
pub(super) struct LensPlan {
    lens_pipeline: wgpu::RenderPipeline,
    downsample_pipeline: wgpu::RenderPipeline,
    atlas: wgpu::Texture,
    copies: Vec<LensCopy>,
    glass: wgpu::BindGroup,
    downsamples: Vec<DownsamplePass>,
    count: u32,
}

impl LensPlan {
    /// Copies the footprints into the atlas and fills its mip levels. The
    /// frame's pass must have ended.
    pub(super) fn copy_backdrop(&self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::Texture) {
        for copy in self
            .copies
            .iter()
            .filter(|copy| copy.size.0 > 0 && copy.size.1 > 0)
        {
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: target,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: copy.from.0,
                        y: copy.from.1,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self.atlas,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: copy.to.0,
                        y: copy.to.1,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: copy.size.0,
                    height: copy.size.1,
                    depth_or_array_layers: 1,
                },
            );
        }
        for downsample in &self.downsamples {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lens_downsample_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &downsample.target,
                    resolve_target: None,
                    // Only the slots are written and only the slots are
                    // read; loading keeps the pass from clearing the level.
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(&self.downsample_pipeline);
            pass.set_bind_group(0, &downsample.bind_group, &[]);
            pass.draw(0..4, 0..self.count);
        }
    }
}

impl WgpuRendererCore {
    /// The largest atlas side this device takes, a whole number of steps.
    fn lens_max_side(&self) -> u32 {
        self.max_texture_size.min(MAX_SIDE) / ATLAS_STEP * ATLAS_STEP
    }

    fn lens_pipelines(&mut self, premultiplied_alpha: bool) -> Option<LensPipelines> {
        if cfg!(target_family = "wasm") || self.uses_webgl_instance_data {
            return None;
        }
        let format = self.target_format;
        let resources = &mut self.resources;
        resources.lens.pipelines(
            &resources.device,
            &resources.bind_group_layouts.globals,
            format,
            premultiplied_alpha,
        )
    }

    /// Writes `instances` to the frame's storage buffer; `None` on the
    /// texture transport, which the glass does not use.
    fn write_lens_storage<T>(
        &mut self,
        instance_offset: &mut u64,
        instances: &[T],
    ) -> Result<Option<StorageSlice>> {
        let (offset, size, _) = self.write_instance_data(instance_offset, instances)?;
        Ok(match &self.resources.instance_data {
            InstanceData::Storage(buffer) => Some(StorageSlice {
                buffer: buffer.clone(),
                offset,
                size,
            }),
            InstanceData::Texture { .. } => None,
        })
    }

    /// Frees the atlas after a frame without glass.
    pub(super) fn release_idle_lens_atlas(&mut self, lenses: usize) {
        if lenses == 0 {
            self.resources.lens.atlas = None;
        }
    }

    /// The batch's plan, or `None` when it must draw flat: the target can't
    /// be read, or the pipelines, the atlas or the storage can't be had.
    pub(super) fn plan_lenses(
        &mut self,
        lenses: &[BackdropLens],
        target: &wgpu::Texture,
        premultiplied_alpha: bool,
        instance_offset: &mut u64,
    ) -> Result<Option<LensPlan>> {
        if lenses.is_empty() || !target.usage().contains(wgpu::TextureUsages::COPY_SRC) {
            return Ok(None);
        }
        let Some(pipelines) = self.lens_pipelines(premultiplied_alpha) else {
            return Ok(None);
        };
        let max_side = self.lens_max_side();
        let Some(packing) = LensPacking::pack(lenses, (target.width(), target.height()), max_side)
        else {
            return Ok(None);
        };
        let Some(lens_data) = self.write_lens_storage(instance_offset, lenses)? else {
            return Ok(None);
        };
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
        let Some(slot_data) = self.write_lens_storage(instance_offset, &slots)? else {
            return Ok(None);
        };
        let mut level_data = Vec::with_capacity(LEVELS as usize);
        for level in 1..LEVELS {
            let at = |v: u32, level: u32| v as f32 / (1u32 << level) as f32;
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
            let Some(data) = self.write_lens_storage(instance_offset, &level_slots)? else {
                return Ok(None);
            };
            level_data.push(data);
        }

        let format = target.format();
        let resources = &mut self.resources;
        let atlas = resources.lens.atlas(
            &resources.device,
            format,
            packing.width,
            packing.height,
            max_side,
        );
        let device = &resources.device;
        let glass = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lens_bind_group"),
            layout: &pipelines.lens_layout,
            entries: &[
                lens_data.binding(4),
                slot_data.binding(5),
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&atlas.view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::Sampler(&pipelines.atlas_sampler),
                },
            ],
        });
        let downsamples = atlas
            .levels
            .iter()
            .zip(atlas.levels.iter().skip(1))
            .zip(&level_data)
            .map(|((source, target), slots)| DownsamplePass {
                target: target.clone(),
                bind_group: device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("lens_downsample_bind_group"),
                    layout: &pipelines.downsample_layout,
                    entries: &[
                        slots.binding(0),
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(source),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&pipelines.level_sampler),
                        },
                    ],
                }),
            })
            .collect();
        Ok(Some(LensPlan {
            lens_pipeline: pipelines.lens,
            downsample_pipeline: pipelines.downsample,
            atlas: atlas.texture.clone(),
            copies: packing.copies,
            glass,
            downsamples,
            count: u32::try_from(lenses.len()).unwrap_or(u32::MAX),
        }))
    }

    /// Draws the batch's glass into the resumed frame pass.
    pub(super) fn draw_glass(&self, plan: &LensPlan, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&plan.lens_pipeline);
        pass.set_bind_group(0, &self.resources.globals_bind_group, &[]);
        pass.set_bind_group(1, &plan.glass, &[]);
        pass.draw(0..4, 0..plan.count);
    }

    /// The texture to draw a frame of `lenses` lenses into instead of
    /// `surface`, when the surface can't be read: a readable copy of its
    /// size that [`Self::copy_frame`] then puts on the surface. `None` when
    /// the frame goes straight to the surface (no glass, a readable
    /// surface, or glass that draws flat anyway); the copy is freed then.
    pub(super) fn glass_frame(
        &mut self,
        lenses: usize,
        surface: &wgpu::Texture,
        premultiplied_alpha: bool,
    ) -> Option<wgpu::Texture> {
        if lenses == 0 || surface.usage().contains(wgpu::TextureUsages::COPY_SRC) {
            self.resources.lens.frame_copy = None;
            return None;
        }
        self.lens_pipelines(premultiplied_alpha)?;
        let size = surface.size();
        let format = surface.format();
        let resources = &mut self.resources;
        if let Some(copy) = resources
            .lens
            .frame_copy
            .take_if(|copy| copy.texture.size() == size && copy.texture.format() == format)
        {
            return Some(resources.lens.frame_copy.insert(copy).texture.clone());
        }
        resources.lens.frame_copy = None;
        let texture = resources.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("lens_frame_copy"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        Some(
            resources
                .lens
                .frame_copy
                .insert(FrameCopy { texture })
                .texture
                .clone(),
        )
    }

    /// Puts a frame drawn into [`Self::glass_frame`]'s texture on the
    /// surface.
    pub(super) fn copy_frame(
        &mut self,
        frame: &wgpu::Texture,
        surface: &wgpu::Texture,
        premultiplied_alpha: bool,
    ) -> Result<wgpu::SubmissionIndex> {
        let pipelines = self
            .lens_pipelines(premultiplied_alpha)
            .ok_or_else(|| anyhow!("the frame copy pipeline is gone"))?;
        let device = &self.resources.device;
        let frame_view = frame.create_view(&wgpu::TextureViewDescriptor::default());
        let surface_view = surface.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lens_frame_copy_bind_group"),
            layout: &pipelines.frame_copy_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&frame_view),
            }],
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("lens_frame_copy_encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("lens_frame_copy_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            });
            pass.set_pipeline(&pipelines.frame_copy);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        Ok(self
            .resources
            .queue
            .submit(std::iter::once(encoder.finish())))
    }
}
