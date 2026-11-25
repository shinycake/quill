// Added by the Quill project (2026). See third_party/gpui-pre-wgpu/QUILL-CHANGES.md.
//! QUILL: snapshot textures on wgpu (docs/decisions/snapshot-transitions.md).
//!
//! The window hands the renderer, with each frame, the scenes to draw into
//! snapshot textures (`Scene::snapshot_scenes`) and the textures whose
//! handles dropped (`Scene::snapshot_releases`). Each live slot owns one
//! texture in the target's format (the pipelines are built for it), sides
//! rounded up to 256, and a bind group that samples it; a released texture
//! is kept for the next capture while another snapshot lives. Each scene is
//! drawn by the frame's own recording (`record_frame`, `FrameKind::Snapshot`)
//! on a submission of its own ahead of the frame's. The pool lives in
//! `WgpuResources`, so a device recovery drops it with the device; the
//! renderer counts that as a loss.

use std::collections::HashMap;
use std::ops::Range;

use anyhow::Result;
use gpui::{DevicePixels, Hsla, Scene, Size, SnapshotScene, SnapshotSlot};

use super::{DEPTH_FORMAT, DepthTexture, FrameKind, InstanceBinding, WgpuRendererCore};

/// A side no texture exceeds.
const MAX_SIDE: u32 = 16384;

pub(super) struct SnapshotTexture {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}

/// The snapshot textures of one renderer (one window).
#[derive(Default)]
pub(super) struct SnapshotPool {
    textures: HashMap<SnapshotSlot, SnapshotTexture>,
    /// Released textures kept for the next capture.
    spare: Vec<SnapshotTexture>,
    /// The depth attachment of the last snapshot pass, kept while the next
    /// one is the same size.
    depth: Option<DepthTexture>,
}

fn rounded(side: i32, limit: u32) -> Option<u32> {
    let side = u32::try_from(side).ok().filter(|side| *side > 0)?;
    Some(side.next_multiple_of(256).min(MAX_SIDE).min(limit)).filter(|rounded| *rounded >= side)
}

impl SnapshotPool {
    fn release(&mut self, slots: &[SnapshotSlot]) {
        for slot in slots {
            if let Some(texture) = self.textures.remove(slot) {
                self.spare.push(texture);
            }
        }
        if self.textures.is_empty() {
            self.spare.clear();
            self.depth = None;
        }
    }

    pub(super) fn bind_group(&self, slot: SnapshotSlot) -> Option<&wgpu::BindGroup> {
        self.textures.get(&slot).map(|texture| &texture.bind_group)
    }
}

/// What a snapshot's texture is cleared to.
fn clear_color(backing: Option<Hsla>) -> wgpu::Color {
    match backing {
        Some(color) => {
            let rgba = color.to_rgb();
            wgpu::Color {
                r: f64::from(rgba.r),
                g: f64::from(rgba.g),
                b: f64::from(rgba.b),
                a: 1.0,
            }
        }
        None => wgpu::Color::TRANSPARENT,
    }
}

impl WgpuRendererCore {
    /// Whether this renderer can draw snapshots: not on the path without
    /// storage buffers.
    pub(super) fn snapshots_supported(&self) -> bool {
        !self.uses_webgl_instance_data
    }

    /// Releases the textures of dropped snapshots and draws the frame's
    /// snapshot scenes, each on a submission of its own. A scene that can't
    /// be drawn loses its texture; returns whether any was lost.
    pub(super) fn draw_snapshot_scenes(&mut self, scene: &Scene) -> bool {
        self.resources.snapshots.release(&scene.snapshot_releases);
        let mut lost = false;
        for snapshot in &scene.snapshot_scenes {
            if let Err(error) = self.draw_snapshot(snapshot) {
                log::error!("snapshot not drawn: {error:#}");
                self.resources.snapshots.textures.remove(&snapshot.slot);
                lost = true;
            }
        }
        lost
    }

    /// Draws one snapshot scene into its texture and submits it.
    pub(super) fn draw_snapshot(&mut self, snapshot: &SnapshotScene) -> Result<wgpu::SubmissionIndex> {
        anyhow::ensure!(self.snapshots_supported(), "no snapshots on this path");
        let texture = self.snapshot_target(snapshot.slot, snapshot.size)?;
        let size = Size {
            width: DevicePixels(i32::try_from(texture.width()).unwrap_or(i32::MAX)),
            height: DevicePixels(i32::try_from(texture.height()).unwrap_or(i32::MAX)),
        };
        self.ensure_snapshot_depth(size);
        self.write_gamma_params();
        self.record_frame(
            &snapshot.scene,
            &texture,
            size,
            clear_color(snapshot.backing),
            FrameKind::Snapshot,
        )
    }

    /// The texture `slot` draws into: its own when it is large enough, else
    /// a spare that is, else a new one.
    fn snapshot_target(&mut self, slot: SnapshotSlot, texels: Size<DevicePixels>) -> Result<wgpu::Texture> {
        let limit = self.max_texture_size;
        let (Some(width), Some(height)) = (rounded(texels.width.0, limit), rounded(texels.height.0, limit)) else {
            anyhow::bail!("a snapshot of {texels:?} texels");
        };
        let fits = |held: &SnapshotTexture| held.texture.width() == width && held.texture.height() == height;
        let pool = &mut self.resources.snapshots;
        if let Some(held) = pool.textures.get(&slot).filter(|held| fits(held)) {
            return Ok(held.texture.clone());
        }
        if let Some(old) = pool.textures.remove(&slot) {
            pool.spare.push(old);
        }
        let spare = pool
            .spare
            .iter()
            .position(fits)
            .map(|index| pool.spare.swap_remove(index));
        let held = match spare {
            Some(held) => held,
            None => self.new_snapshot_texture(width, height),
        };
        let texture = held.texture.clone();
        self.resources.snapshots.textures.insert(slot, held);
        Ok(texture)
    }

    fn new_snapshot_texture(&self, width: u32, height: u32) -> SnapshotTexture {
        let texture = self.resources.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("snapshot_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.target_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.create_texture_bind_group("snapshot_texture_bind_group", &view);
        SnapshotTexture { texture, bind_group }
    }

    fn ensure_snapshot_depth(&mut self, size: Size<DevicePixels>) {
        let (width, height) = (size.width.0 as u32, size.height.0 as u32);
        if self
            .resources
            .snapshots
            .depth
            .as_ref()
            .is_some_and(|depth| depth.texture.width() == width && depth.texture.height() == height)
        {
            return;
        }
        let texture = self.resources.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("snapshot_depth_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.resources.snapshots.depth = Some(DepthTexture { texture, view });
    }

    /// The depth attachment of a snapshot pass.
    pub(super) fn snapshot_depth_view(&self) -> Option<wgpu::TextureView> {
        self.resources.snapshots.depth.as_ref().map(|depth| depth.view.clone())
    }

    /// Draws a batch of snapshot quads, each with its own texture. A quad
    /// whose texture is gone draws nothing.
    pub(super) fn draw_snapshots(
        &self,
        sprites: &InstanceBinding,
        scene: &Scene,
        quads: Range<usize>,
        pass: &mut wgpu::RenderPass<'_>,
    ) {
        let Some(batch) = scene.snapshots.get(quads.clone()) else {
            return;
        };
        let resources = self.resources();
        pass.set_pipeline(&resources.pipelines.poly_sprites);
        pass.set_bind_group(1, &sprites.bind_group, &[]);
        for (index, quad) in quads.zip(batch) {
            let Some(bind_group) = resources.snapshots.bind_group(quad.slot) else {
                continue;
            };
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
            pass.set_bind_group(2, bind_group, &[]);
            let instance = sprites.first_instance + index;
            pass.draw(0..4, instance..instance + 1);
        }
    }
}
