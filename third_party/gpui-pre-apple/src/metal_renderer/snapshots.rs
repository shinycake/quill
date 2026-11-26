// Added by the Quill project (2026). See third_party/gpui-pre-apple/QUILL-CHANGES.md.
//! QUILL: snapshot textures on Metal (docs/decisions/snapshot-transitions.md).
//!
//! The window hands the renderer, with each frame, the scenes to draw into
//! snapshot textures (`Scene::snapshot_scenes`) and the textures whose
//! handles dropped (`Scene::snapshot_releases`). Each live slot owns one
//! texture in the drawable's format, kept in gpui's `SnapshotPool` with the
//! other backends' rules (sizes, spares, losses). The frame draws a
//! `SnapshotQuad` through the polychrome sprite pipeline, the snapshot's
//! texture bound in place of an atlas.

use std::ops::Range;

use anyhow::Result;
use gpui::{DevicePixels, Hsla, Scene, SnapshotLosses, SnapshotScene, size};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLClearColor, MTLCommandBuffer, MTLPrimitiveType, MTLRenderCommandEncoder, MTLTexture,
};
use std::{mem, ptr};

use super::{
    InstanceBindings, InstanceBuffer, InstanceBufferPool, InstanceBufferWriter, MetalRenderer,
    SpriteInputIndex,
};
use block2::RcBlock;
use objc2_core_foundation::CFRetained;
use objc2_core_video::CVMetalTexture;
use parking_lot::Mutex;
use std::{cell::Cell, sync::Arc};

type Texture = Retained<ProtocolObject<dyn MTLTexture>>;

/// The snapshot textures of one renderer (one window).
pub(super) type SnapshotPool = gpui::SnapshotPool<Texture>;

/// What the snapshot command buffers of one frame read, kept until the
/// last of them completes: the instance buffer goes back to its pool then.
struct HeldUntilDrawn {
    pool: Arc<Mutex<InstanceBufferPool>>,
    buffer: Option<InstanceBuffer>,
    _surfaces: Vec<CFRetained<CVMetalTexture>>,
}

impl Drop for HeldUntilDrawn {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            self.pool.lock().release(buffer);
        }
    }
}

/// What a snapshot's texture is cleared to.
fn clear_color(backing: Option<Hsla>) -> MTLClearColor {
    match backing {
        Some(color) => {
            let rgba = color.to_rgb();
            MTLClearColor {
                red: f64::from(rgba.r),
                green: f64::from(rgba.g),
                blue: f64::from(rgba.b),
                alpha: 1.0,
            }
        }
        None => MTLClearColor {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 0.0,
        },
    }
}

impl MetalRenderer {
    /// Releases the textures of dropped snapshots and draws the frame's
    /// snapshot scenes, each into its texture on a command buffer of its
    /// own, committed now, ahead of the frame's. Their instance buffer and
    /// the video textures they read are kept until the last of them
    /// completes. A scene that can't be drawn loses its texture.
    pub(super) fn draw_snapshot_scenes(&mut self, scene: &Scene) {
        self.snapshots.release(&scene.snapshot_releases);
        if scene.snapshot_scenes.is_empty() {
            return;
        }
        let mut writer = match InstanceBufferWriter::new(
            &self.device,
            &self.instance_buffer_pool,
            self.is_unified_memory,
        ) {
            Ok(writer) => writer,
            Err(error) => {
                log::error!("snapshots not drawn: {error:#}");
                for snapshot in &scene.snapshot_scenes {
                    self.snapshots.lose(snapshot.slot);
                }
                return;
            }
        };
        let mut surfaces = Vec::new();
        let mut renders = Vec::new();
        for snapshot in &scene.snapshot_scenes {
            match self.encode_snapshot(snapshot, &mut writer, &mut surfaces) {
                Ok(command_buffer) => renders.push(command_buffer),
                Err(error) => {
                    log::error!("snapshot not drawn: {error:#}");
                    self.snapshots.lose(snapshot.slot);
                }
            }
        }
        let held = Arc::new(HeldUntilDrawn {
            pool: self.instance_buffer_pool.clone(),
            buffer: Some(writer.finish()),
            _surfaces: surfaces,
        });
        for command_buffer in &renders {
            let held = Cell::new(Some(held.clone()));
            let block = RcBlock::new(
                move |_: ptr::NonNull<ProtocolObject<dyn MTLCommandBuffer>>| drop(held.take()),
            );
            // SAFETY: Metal copies the live block and calls it once, after
            // this command buffer completes; it only drops its share.
            unsafe { command_buffer.addCompletedHandler(RcBlock::as_ptr(&block)) };
            command_buffer.commit();
        }
    }

    fn encode_snapshot(
        &mut self,
        snapshot: &SnapshotScene,
        writer: &mut InstanceBufferWriter,
        surfaces: &mut Vec<CFRetained<CVMetalTexture>>,
    ) -> Result<Retained<ProtocolObject<dyn MTLCommandBuffer>>> {
        let format = self.drawable_format;
        let device = self.device.clone();
        let texture = self
            .snapshots
            .target(
                snapshot.slot,
                snapshot.size,
                |texture| texture.pixelFormat() == format,
                |width, height| super::scene_target::new_scene_target(&device, format, width, height),
            )?
            .clone();
        self.draw_scene_into(
            &snapshot.scene,
            writer,
            &texture,
            clear_color(snapshot.backing),
            surfaces,
        )
    }

    /// QUILL: draws one snapshot scene into its texture now, outside a
    /// frame (`Window::snapshot_last_frame`), and waits for the GPU, since
    /// the atlas tiles it reads may be freed as soon as this returns.
    pub fn draw_snapshot(&mut self, snapshot: &SnapshotScene) -> bool {
        let drawn = objc2::rc::autoreleasepool(|_| -> Result<()> {
            let mut writer = InstanceBufferWriter::new(
                &self.device,
                &self.instance_buffer_pool,
                self.is_unified_memory,
            )?;
            let mut surfaces = Vec::new();
            let command_buffer = self.encode_snapshot(snapshot, &mut writer, &mut surfaces)?;
            let buffer = writer.finish();
            command_buffer.commit();
            command_buffer.waitUntilCompleted();
            self.instance_buffer_pool.lock().release(buffer);
            drop(surfaces);
            Ok(())
        });
        match drawn {
            Ok(()) => true,
            Err(error) => {
                log::error!("snapshot not drawn: {error:#}");
                self.snapshots.lose(snapshot.slot);
                false
            }
        }
    }

    /// QUILL: the largest side of a snapshot texture.
    pub fn snapshot_max_side(&self) -> u32 {
        self.snapshots.max_side()
    }

    /// QUILL: what this renderer lost since the last call.
    pub fn take_snapshot_losses(&mut self) -> SnapshotLosses {
        self.snapshots.take_losses()
    }

    /// Draws a batch of snapshot quads, each with its own texture. A quad
    /// whose texture is gone draws nothing.
    pub(super) fn draw_snapshots(
        &self,
        scene: &Scene,
        quads: Range<usize>,
        instance_bindings: &InstanceBindings,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) {
        let Some(batch) = scene.snapshots.get(quads.clone()) else {
            return;
        };
        command_encoder.setRenderPipelineState(&self.polychrome_sprites_pipeline_state);
        for (index, quad) in quads.zip(batch) {
            let Some(texture) = self.snapshots.get(quad.slot) else {
                continue;
            };
            let texture_size = size(
                DevicePixels(i32::try_from(texture.width()).unwrap_or(i32::MAX)),
                DevicePixels(i32::try_from(texture.height()).unwrap_or(i32::MAX)),
            );
            // SAFETY: the snapshot sprites were written for every quad of the
            // scene in order, matching the shader's `PolychromeSprite` layout;
            // `index` is in range. Metal retains the buffer and texture.
            unsafe {
                command_encoder.setVertexBuffer_offset_atIndex(
                    Some(&instance_bindings.snapshot_sprites.buffer),
                    instance_bindings.snapshot_sprites.offset,
                    SpriteInputIndex::Sprites as usize,
                );
                command_encoder.setVertexBytes_length_atIndex(
                    ptr::NonNull::from(&texture_size).cast(),
                    mem::size_of_val(&texture_size),
                    SpriteInputIndex::AtlasTextureSize as usize,
                );
                command_encoder.setFragmentBuffer_offset_atIndex(
                    Some(&instance_bindings.snapshot_sprites.buffer),
                    instance_bindings.snapshot_sprites.offset,
                    SpriteInputIndex::Sprites as usize,
                );
                command_encoder
                    .setFragmentTexture_atIndex(Some(texture), SpriteInputIndex::AtlasTexture as usize);
                command_encoder.drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                    MTLPrimitiveType::TriangleStrip,
                    0,
                    4,
                    1,
                    index,
                );
            }
        }
    }
}
