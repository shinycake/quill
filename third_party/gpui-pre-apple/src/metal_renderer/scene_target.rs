// Added by the Quill project (2026). See third_party/gpui-pre-apple/QUILL-CHANGES.md.
//! QUILL: drawing a whole scene into a texture of its own, on its own
//! command buffer: the re-rendered backdrops of sharp magnifiers and
//! snapshot textures draw through it (docs/decisions/snapshot-transitions.md).

use anyhow::{Context as _, Result};
use gpui::{DevicePixels, Scene, size};
use objc2_core_foundation::CFRetained;
use objc2_core_video::CVMetalTexture;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{
    MTLClearColor, MTLCommandBuffer, MTLDevice, MTLPixelFormat, MTLStorageMode, MTLTexture,
    MTLTextureDescriptor, MTLTextureUsage,
};

use super::{InstanceBufferWriter, MetalRenderer};

/// A texture `width` x `height` texels in `format` that a scene can be
/// drawn into and a shader can read.
pub(super) fn new_scene_target(
    device: &ProtocolObject<dyn MTLDevice>,
    format: MTLPixelFormat,
    width: u32,
    height: u32,
) -> Result<Retained<ProtocolObject<dyn MTLTexture>>> {
    let descriptor = MTLTextureDescriptor::new();
    // SAFETY: positive sizes; callers keep them within the device limit.
    unsafe {
        descriptor.setWidth(width.max(1) as usize);
        descriptor.setHeight(height.max(1) as usize);
    }
    descriptor.setPixelFormat(format);
    descriptor.setStorageMode(MTLStorageMode::Private);
    descriptor.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
    device
        .newTextureWithDescriptor(&descriptor)
        .context("allocating a scene target")
}

impl MetalRenderer {
    /// Encodes `scene` into the whole of `texture`, cleared to `clear`, on a
    /// command buffer of its own that the caller commits ahead of the frame
    /// that reads it. The video textures it reads join `surfaces`, which
    /// the caller keeps until that command buffer completes.
    pub(super) fn draw_scene_into(
        &mut self,
        scene: &Scene,
        writer: &mut InstanceBufferWriter,
        texture: &ProtocolObject<dyn MTLTexture>,
        clear: MTLClearColor,
        surfaces: &mut Vec<CFRetained<CVMetalTexture>>,
    ) -> Result<Retained<ProtocolObject<dyn MTLCommandBuffer>>> {
        let bindings = super::write_instances(scene, writer, None)?;
        let viewport = size(
            DevicePixels(i32::try_from(texture.width()).unwrap_or(i32::MAX)),
            DevicePixels(i32::try_from(texture.height()).unwrap_or(i32::MAX)),
        );
        self.draw_primitives_to_texture(
            scene,
            &bindings,
            writer,
            texture,
            viewport,
            surfaces,
            None,
            clear,
        )
    }
}
