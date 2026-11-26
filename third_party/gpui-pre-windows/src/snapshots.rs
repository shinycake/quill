// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.
//! QUILL: snapshot textures on DirectX 11 (docs/decisions/snapshot-transitions.md).
//!
//! The window hands the renderer, with each frame, the scenes to draw into
//! snapshot textures (`Scene::snapshot_scenes`) and the textures whose
//! handles dropped (`Scene::snapshot_releases`). Each live slot owns one
//! `Texture2D` in the SDR swap chain's format with a render-target and a
//! shader-resource view, kept in gpui's `SnapshotPool` with the other
//! backends' rules (sizes, spares, losses). Each scene is
//! drawn by the frame's own recording (`DirectXRenderer::record`) with the
//! texture bound as the target, its own viewport and SDR output; the next
//! frame's `pre_draw` binds the frame's target, viewport and globals again.
//! The pool lives in `DirectXRenderPipelines`, so a lost device drops it.

use std::ops::Range;

use anyhow::{Context, Result};
use gpui::{Hsla, Scene, SnapshotLosses, SnapshotScene};
use windows::Win32::Graphics::{Direct3D11::*, Dxgi::Common::*};

use super::{DirectXRenderer, DrawTarget, FrameKind, RENDER_TARGET_FORMAT};

struct SnapshotTexture {
    target: Option<ID3D11RenderTargetView>,
    view: [Option<ID3D11ShaderResourceView>; 1],
}

/// The snapshot textures of one renderer (one window), dropped with the
/// device.
pub(super) struct Snapshots {
    pool: gpui::SnapshotPool<SnapshotTexture>,
    /// The depth view of the last snapshot pass and its size, kept while
    /// the next one is the same size.
    depth: Option<(u32, u32, Option<ID3D11DepthStencilView>)>,
}

impl Snapshots {
    /// No textures yet; none larger than the device takes.
    pub(super) fn new(device: &ID3D11Device) -> Self {
        Self {
            pool: gpui::SnapshotPool::new(super::backdrop_glass::max_atlas_side(device)),
            depth: None,
        }
    }

    fn release(&mut self, scene: &Scene) {
        self.pool.release(&scene.snapshot_releases);
        if self.pool.is_empty() {
            self.depth = None;
        }
    }

    fn view(&self, slot: gpui::SnapshotSlot) -> Option<&[Option<ID3D11ShaderResourceView>; 1]> {
        self.pool.get(slot).map(|texture| &texture.view)
    }
}

/// What a snapshot's texture is cleared to.
fn clear_color(backing: Option<Hsla>) -> [f32; 4] {
    match backing {
        Some(color) => {
            let rgba = color.to_rgb();
            [rgba.r, rgba.g, rgba.b, 1.0]
        }
        None => [0.0; 4],
    }
}

fn new_texture(device: &ID3D11Device, width: u32, height: u32) -> Result<SnapshotTexture> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: RENDER_TARGET_FORMAT,
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
    // SAFETY: a valid descriptor; the call writes the out parameter.
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture))? };
    let texture = texture.context("creating a snapshot texture")?;
    let mut target = None;
    let mut view = None;
    // SAFETY: views of the whole texture just made, by its own format.
    unsafe {
        device.CreateRenderTargetView(&texture, None, Some(&mut target))?;
        device.CreateShaderResourceView(&texture, None, Some(&mut view))?;
    }
    Ok(SnapshotTexture {
        target,
        view: [view],
    })
}

fn new_depth(device: &ID3D11Device, width: u32, height: u32) -> Result<Option<ID3D11DepthStencilView>> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_D32_FLOAT,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_DEPTH_STENCIL.0 as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut texture = None;
    // SAFETY: a valid descriptor; the call writes the out parameter.
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture))? };
    let texture = texture.context("creating a snapshot depth texture")?;
    let mut view = None;
    // SAFETY: a view of the whole depth texture just made.
    unsafe { device.CreateDepthStencilView(&texture, None, Some(&mut view))? };
    Ok(view)
}

impl DirectXRenderer {
    /// The largest side of a snapshot texture, `None` without a device.
    pub(crate) fn snapshot_max_side(&self) -> Option<u32> {
        self.devices
            .is_some()
            .then(|| self.pipelines.snapshots.pool.max_side())
    }

    /// What this renderer lost since the last call.
    pub(crate) fn take_snapshot_losses(&mut self) -> SnapshotLosses {
        let mut losses = std::mem::take(&mut self.snapshot_losses);
        losses.merge(self.pipelines.snapshots.pool.take_losses());
        losses
    }

    /// Draws one snapshot scene now, outside a frame
    /// (`Window::snapshot_last_frame`). Direct3D keeps the atlas textures it
    /// reads alive until the GPU is done with them.
    pub(crate) fn draw_snapshot(&mut self, snapshot: &SnapshotScene) -> bool {
        self.try_draw_snapshot(snapshot)
    }

    /// Releases the textures of dropped snapshots and draws the frame's
    /// snapshot scenes. While a lost device is being replaced none can be
    /// drawn, and every texture counts as lost.
    pub(super) fn draw_snapshot_scenes(&mut self, scene: &Scene) {
        self.pipelines.snapshots.release(scene);
        if self.skip_draws {
            if !scene.snapshot_scenes.is_empty() {
                self.snapshot_losses = SnapshotLosses::All;
            }
            return;
        }
        for snapshot in &scene.snapshot_scenes {
            self.try_draw_snapshot(snapshot);
        }
    }

    /// Draws one snapshot scene; a failure loses its texture.
    fn try_draw_snapshot(&mut self, snapshot: &SnapshotScene) -> bool {
        match self.draw_snapshot_scene(snapshot) {
            Ok(()) => true,
            Err(error) => {
                log::error!("snapshot not drawn: {error:#}");
                self.pipelines.snapshots.pool.lose(snapshot.slot);
                false
            }
        }
    }

    /// Draws one snapshot scene into its texture.
    fn draw_snapshot_scene(&mut self, snapshot: &SnapshotScene) -> Result<()> {
        // A recovery in progress leaves the atlas without its textures.
        anyhow::ensure!(!self.skip_draws, "recovering from a lost device");
        let device = self
            .devices
            .as_ref()
            .context("devices missing")?
            .device
            .clone();
        let snapshots = &mut self.pipelines.snapshots;
        let target = snapshots
            .pool
            .target(snapshot.slot, snapshot.size, |_| true, |width, height| {
                new_texture(&device, width, height)
            })?
            .target
            .clone()
            .context("snapshot texture missing")?;
        let (width, height) = snapshots
            .pool
            .size_of(snapshot.slot)
            .context("snapshot texture missing")?;
        if !snapshots.depth.as_ref().is_some_and(|(w, h, _)| *w == width && *h == height) {
            snapshots.depth = Some((width, height, new_depth(&device, width, height)?));
        }
        let target = DrawTarget {
            target,
            depth: snapshots.depth.as_ref().and_then(|(_, _, depth)| depth.clone()),
            viewport: D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: width as f32,
                Height: height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            },
            kind: FrameKind::Snapshot,
        };
        self.record(&snapshot.scene, &clear_color(snapshot.backing), &target)
    }

    /// Draws a batch of snapshot quads, each with its own texture; their
    /// sprites follow the scene's own in the polychrome buffer. A quad whose
    /// texture is gone draws nothing.
    pub(super) fn draw_snapshots(&self, scene: &Scene, quads: Range<usize>) -> Result<()> {
        let Some(batch) = scene.snapshots.get(quads.clone()) else {
            return Ok(());
        };
        let devices = self.devices.as_ref().context("devices missing")?;
        let batch_params = self
            .globals
            .batch_params_buffer
            .as_ref()
            .context("batch params buffer missing")?;
        let first = scene.polychrome_sprites.len();
        for (index, quad) in quads.zip(batch) {
            let Some(view) = self.pipelines.snapshots.view(quad.slot) else {
                continue;
            };
            let Ok(instance) = u32::try_from(first + index) else {
                continue;
            };
            self.pipelines.poly_sprites.draw_range_with_texture(
                &devices.device_context,
                view,
                batch_params,
                std::slice::from_ref(&self.globals.sampler),
                instance,
                1,
            )?;
        }
        Ok(())
    }
}
