// Added by the Quill project (2026). See third_party/gpui-pre-apple/QUILL-CHANGES.md.
//! QUILL: snapshot textures on Metal (docs/decisions/snapshot-transitions.md).
//!
//! The window hands the renderer, with each frame, the scenes to draw into
//! snapshot textures (`Scene::snapshot_scenes`) and the textures whose
//! handles dropped (`Scene::snapshot_releases`). Each live slot owns one
//! texture in the drawable's format, sides rounded up to 256 as the lens
//! canvases are; a released texture is kept for the next capture while
//! another snapshot lives, and freed with the last one. The frame draws a
//! `SnapshotQuad` through the polychrome sprite pipeline, the snapshot's
//! texture bound in place of an atlas.

use std::collections::HashMap;
use std::ops::Range;

use anyhow::Result;
use gpui::{DevicePixels, Hsla, Scene, SnapshotScene, SnapshotSlot, size};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLClearColor, MTLCommandBuffer, MTLDevice, MTLPixelFormat, MTLPrimitiveType,
    MTLRenderCommandEncoder, MTLTexture,
};
use std::{mem, ptr};

use super::{InstanceBindings, InstanceBufferWriter, MetalRenderer, SpriteInputIndex};

type Texture = Retained<ProtocolObject<dyn MTLTexture>>;

/// A side no texture exceeds.
const MAX_SIDE: u32 = 16384;

/// The snapshot textures of one renderer (one window).
#[derive(Default)]
pub(super) struct SnapshotPool {
    textures: HashMap<SnapshotSlot, Texture>,
    /// Released textures kept for the next capture.
    spare: Vec<Texture>,
    /// How many times textures were lost (`PlatformWindow::snapshot_losses`).
    losses: u64,
}

fn rounded(side: i32) -> Option<u32> {
    let side = u32::try_from(side).ok().filter(|side| *side > 0)?;
    Some(side.next_multiple_of(256).min(MAX_SIDE)).filter(|rounded| *rounded >= side)
}

impl SnapshotPool {
    pub(super) fn texture(&self, slot: SnapshotSlot) -> Option<&Texture> {
        self.textures.get(&slot)
    }

    pub(super) fn losses(&self) -> u64 {
        self.losses
    }

    fn release(&mut self, slots: &[SnapshotSlot]) {
        for slot in slots {
            if let Some(texture) = self.textures.remove(slot) {
                self.spare.push(texture);
            }
        }
        if self.textures.is_empty() {
            self.spare.clear();
        }
    }

    /// The texture `slot` draws into: its own when it is large enough and in
    /// `format`, else a spare that is, else a new one.
    fn target(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        format: MTLPixelFormat,
        slot: SnapshotSlot,
        texels: gpui::Size<DevicePixels>,
    ) -> Result<Texture> {
        let (Some(width), Some(height)) = (rounded(texels.width.0), rounded(texels.height.0)) else {
            anyhow::bail!("a snapshot of {texels:?} texels");
        };
        let fits = |texture: &Texture| {
            texture.width() == width as usize
                && texture.height() == height as usize
                && texture.pixelFormat() == format
        };
        if let Some(texture) = self.textures.get(&slot).filter(|texture| fits(texture)) {
            return Ok(texture.clone());
        }
        if let Some(old) = self.textures.remove(&slot) {
            self.spare.push(old);
        }
        let texture = match self.spare.iter().position(fits) {
            Some(index) => self.spare.swap_remove(index),
            None => super::scene_target::new_scene_target(device, format, width, height)?,
        };
        self.textures.insert(slot, texture.clone());
        Ok(texture)
    }

    /// A texture could not be drawn: it is dropped, and the window learns
    /// its snapshots are lost.
    fn lose(&mut self, slot: SnapshotSlot) {
        self.textures.remove(&slot);
        self.losses = self.losses.wrapping_add(1);
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
    /// Releases the textures of dropped snapshots and encodes the frame's
    /// snapshot scenes, each into its texture on a command buffer of its
    /// own, to be committed ahead of the frame's. A scene that can't be
    /// drawn loses its texture.
    pub(super) fn draw_snapshot_scenes(
        &mut self,
        scene: &Scene,
        writer: &mut InstanceBufferWriter,
    ) -> Vec<Retained<ProtocolObject<dyn MTLCommandBuffer>>> {
        self.snapshots.release(&scene.snapshot_releases);
        let mut renders = Vec::new();
        for snapshot in &scene.snapshot_scenes {
            match self.encode_snapshot(snapshot, writer) {
                Ok(command_buffer) => renders.push(command_buffer),
                Err(error) => {
                    log::error!("snapshot not drawn: {error:#}");
                    self.snapshots.lose(snapshot.slot);
                }
            }
        }
        renders
    }

    fn encode_snapshot(
        &mut self,
        snapshot: &SnapshotScene,
        writer: &mut InstanceBufferWriter,
    ) -> Result<Retained<ProtocolObject<dyn MTLCommandBuffer>>> {
        let format = self.drawable_format;
        let texture = self
            .snapshots
            .target(&self.device, format, snapshot.slot, snapshot.size)?;
        self.draw_scene_into(
            &snapshot.scene,
            writer,
            &texture,
            clear_color(snapshot.backing),
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
            let command_buffer = self.encode_snapshot(snapshot, &mut writer)?;
            let buffer = writer.finish();
            command_buffer.commit();
            command_buffer.waitUntilCompleted();
            self.instance_buffer_pool.lock().release(buffer);
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

    /// QUILL: how many times this renderer lost snapshot textures.
    pub fn snapshot_losses(&self) -> u64 {
        self.snapshots.losses()
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
            let Some(texture) = self.snapshots.texture(quad.slot) else {
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
