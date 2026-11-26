// Modified by the Quill project (2026) from gpui-pre-apple 0.3.9 (Apache-2.0):
// window capture renders the scene offscreen and reads it back; backdrop glass
// (`metal_renderer/lens.rs`); snapshot textures (`metal_renderer/snapshots.rs`).
// See third_party/gpui-pre-apple/QUILL-CHANGES.md.
use crate::metal_atlas::MetalAtlas;
use drawables::{DrawableSource, Swap};

mod drawables;
mod lens;
mod scene_target;
mod snapshots;
pub use lens::{LensDownsampleIndex, LensDownsampleSlot, LensInputIndex, LensSlot};
use anyhow::{Context as _, Result};
use block2::RcBlock;
use gpui::{
    AtlasTextureId, BackdropLens, Bounds, ContentMask, DevicePixels, PaintSurface, PaintedPath,
    PrimitiveBatch, ScaledPixels, Scene, Size, SnapshotQuad, quad_depth, quad_depth_scale, size,
};
use image::RgbaImage;
use objc2::{Message, rc::Retained, runtime::ProtocolObject};
use objc2_core_foundation::{CFRetained, CGSize};
use objc2_core_graphics::{CGColorSpace, kCGColorSpaceDisplayP3};
use objc2_core_video::{
    CVMetalTexture, CVMetalTextureCache, CVMetalTextureGetTexture, CVPixelBufferGetHeight,
    CVPixelBufferGetHeightOfPlane, CVPixelBufferGetPixelFormatType, CVPixelBufferGetWidth,
    CVPixelBufferGetWidthOfPlane, kCVPixelFormatType_64RGBAHalf,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, kCVReturnSuccess,
};
use objc2_foundation::{NSRange, NSString};
#[cfg(target_os = "macos")]
use objc2_metal::MTLCopyAllDevices;
use objc2_metal::{
    MTLBlendFactor, MTLBlendOperation, MTLBuffer, MTLClearColor, MTLCommandBuffer,
    MTLCommandEncoder, MTLCommandQueue, MTLCompareFunction, MTLCreateSystemDefaultDevice,
    MTLDepthStencilDescriptor, MTLDepthStencilState, MTLDevice, MTLDrawable, MTLGPUFamily,
    MTLLibrary, MTLLoadAction, MTLPixelFormat, MTLPrimitiveType, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLRenderPipelineDescriptor, MTLRenderPipelineState,
    MTLResourceOptions, MTLStorageMode, MTLStoreAction, MTLTexture, MTLTextureDescriptor,
    MTLTextureUsage, MTLViewport,
};
use objc2_metal::{MTLBlitCommandEncoder, MTLOrigin, MTLRegion, MTLSize};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};
use parking_lot::Mutex;

use std::{
    cell::Cell,
    mem,
    mem::MaybeUninit,
    ops::Range,
    ptr, slice,
    sync::Arc,
    time::{Duration, Instant},
};

#[cfg(quill_metallib)]
const SHADERS_METALLIB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shaders.metallib"));
#[cfg(feature = "runtime_shaders")]
const SHADERS_SOURCE_FILE: &str = include_str!(concat!(env!("OUT_DIR"), "/stitched_shaders.metal"));
/// Metal requires the offset a buffer is bound at to be 256-byte aligned.
const INSTANCE_BUFFER_ALIGNMENT: usize = 256;
const MAX_INSTANCE_BUFFER_SIZE: usize = 256 * 1024 * 1024;
/// QUILL: a drawable not handed out for this long, while another one was
/// handed out within `DRAWING_WITHIN`, is a spare (`trim_spare_drawables`).
const SPARE_DRAWABLE_AFTER: Duration = Duration::from_secs(3);
const DRAWING_WITHIN: Duration = Duration::from_secs(1);

/// QUILL: an `IOSurfaceRef` as `-[MTLTexture iosurface]` returns it. A bare
/// `*mut c_void` fails objc2's check of the return type in debug builds.
#[repr(transparent)]
struct SurfaceRef(*mut std::ffi::c_void);

// SAFETY: a pointer to the opaque `struct __IOSurface`, as declared.
unsafe impl objc2::encode::Encode for SurfaceRef {
    const ENCODING: objc2::encode::Encoding =
        objc2::encode::Encoding::Pointer(&objc2::encode::Encoding::Struct("__IOSurface", &[]));
}

pub type Context = Arc<Mutex<InstanceBufferPool>>;
pub type Renderer = MetalRenderer;

pub fn new_renderer(
    context: self::Context,
    transparent: bool,
    use_display_p3: bool,
) -> Result<Renderer> {
    MetalRenderer::new(context, transparent, use_display_p3)
}

pub struct InstanceBufferPool {
    buffer_size: usize,
    buffers: Vec<InstanceBuffer>,
}

impl Default for InstanceBufferPool {
    fn default() -> Self {
        Self {
            buffer_size: 2 * 1024 * 1024,
            buffers: Vec::new(),
        }
    }
}

struct InstanceBuffer {
    metal_buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
    size: usize,
}

// SAFETY: Each buffer has one CPU writer and returns to the mutex-protected pool only after GPU completion.
unsafe impl Send for InstanceBuffer {}

impl InstanceBufferPool {
    pub(crate) fn reset(&mut self, buffer_size: usize) {
        self.buffer_size = buffer_size;
        self.buffers.clear();
    }

    fn acquire(
        &mut self,
        device: &ProtocolObject<dyn MTLDevice>,
        unified_memory: bool,
    ) -> Result<InstanceBuffer> {
        if let Some(buffer) = self.buffers.pop() {
            return Ok(buffer);
        }
        let options = if unified_memory {
            MTLResourceOptions::StorageModeShared
                // Buffers are write only which can benefit from the combined cache
                // https://developer.apple.com/documentation/metal/mtlresourceoptions/cpucachemodewritecombined
                | MTLResourceOptions::CPUCacheModeWriteCombined
        } else {
            MTLResourceOptions::StorageModeManaged
        };
        let buffer = device
            .newBufferWithLength_options(self.buffer_size, options)
            .with_context(|| format!("allocating {}-byte instance buffer", self.buffer_size))?;
        Ok(InstanceBuffer {
            metal_buffer: buffer,
            size: self.buffer_size,
        })
    }

    fn release(&mut self, buffer: InstanceBuffer) {
        if buffer.size == self.buffer_size {
            self.buffers.push(buffer)
        }
    }
}

enum DisplayLinkMode {
    Detached,
    Attached {
        drawable: Option<Retained<ProtocolObject<dyn CAMetalDrawable>>>,
    },
}

pub struct MetalRenderer {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    layer: Option<Retained<CAMetalLayer>>,
    is_apple_gpu: bool,
    is_unified_memory: bool,
    presents_with_transaction: bool,
    display_link_mode: DisplayLinkMode,
    /// For headless rendering, tracks whether output should be opaque
    opaque: bool,
    /// Whether the last scene drawn to the layer let native views show through it.
    shows_native_views: bool,
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    path_tiles_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    shadows_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    quads_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    opaque_quads_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    underlines_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    monochrome_sprites_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    polychrome_sprites_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    surfaces_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    native_views_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    depth_write_state: Retained<ProtocolObject<dyn MTLDepthStencilState>>,
    depth_test_state: Retained<ProtocolObject<dyn MTLDepthStencilState>>,
    /// QUILL: frames with glass draw without the opaque pre-pass, so depth
    /// neither culls nor occludes (`draw_primitives_to_texture`).
    depth_ignore_state: Retained<ProtocolObject<dyn MTLDepthStencilState>>,
    depth_texture: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
    // QUILL: HDR still images (`64RGBAHalf` surfaces), and the drawable
    // format the pipelines above were built for. A float (EDR) layer swaps
    // in a set built for `RGBA16Float`; the other set is kept for the way
    // back. A format whose set failed to build is not tried again, because
    // the attempt would repeat on every frame.
    hdr_surfaces_pipeline_state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    drawable_format: MTLPixelFormat,
    other_pipelines: Option<DrawablePipelines>,
    failed_format: Option<MTLPixelFormat>,
    library: Retained<ProtocolObject<dyn MTLLibrary>>,
    instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
    sprite_atlas: Arc<MetalAtlas>,
    core_video_texture_cache: CFRetained<CVMetalTextureCache>,
    /// Offscreen render target reused across `render_scene` calls when
    /// rendering headlessly without reading pixels back.
    #[cfg(any(test, feature = "bench-support", feature = "test-support"))]
    headless_render_target: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
    /// QUILL: when each drawable of the layer's pool, known by its
    /// IOSurface, was last handed out (`trim_spare_drawables`).
    drawables_handed_out: Vec<(usize, Instant)>,
    /// QUILL: backdrop glass's pipelines and atlas (`lens.rs`).
    lens: lens::LensState,
    /// QUILL: whether the layer hands out drawables glass can read.
    drawable_reads: lens::DrawableReads,
    /// QUILL: snapshot textures (`snapshots.rs`).
    snapshots: snapshots::SnapshotPool,
    /// QUILL: whether the last frame reached the screen as drawn
    /// (`drawables.rs`).
    presented: drawables::Presented,
}

impl MetalRenderer {
    /// Creates a new MetalRenderer with a CAMetalLayer for window-based rendering.
    pub fn new(
        instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
        transparent: bool,
        use_display_p3: bool,
    ) -> Result<Self> {
        let device = Self::create_device()?;
        let layer = CAMetalLayer::new();
        Self::configure_layer(&layer, &device, transparent, use_display_p3);
        Self::new_internal(device, Some(layer), !transparent, instance_buffer_pool)
    }

    /// Creates a renderer for a CAMetalLayer owned by a platform view, such as
    /// the backing layer UIKit creates for a view whose `layerClass` is
    /// `CAMetalLayer`. The renderer retains the layer.
    pub fn from_layer(
        instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
        layer: &CAMetalLayer,
        transparent: bool,
    ) -> Result<Self> {
        let device = Self::create_device()?;
        let layer = layer.retain();
        Self::configure_layer(&layer, &device, transparent, false);
        Self::new_internal(device, Some(layer), !transparent, instance_buffer_pool)
    }

    fn configure_layer(
        layer: &CAMetalLayer,
        device: &ProtocolObject<dyn MTLDevice>,
        transparent: bool,
        use_display_p3: bool,
    ) {
        layer.setDevice(Some(device));
        layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        if use_display_p3 {
            // SAFETY: The constant is extern, from Core Graphics
            let display_p3_name = unsafe { kCGColorSpaceDisplayP3 };
            if let Some(display_p3_color_space) = CGColorSpace::with_name(Some(display_p3_name)) {
                layer.setColorspace(Some(&display_p3_color_space));
            } else {
                log::error!("unable to create the Display P3 color space");
            }
        }
        // Support direct-to-display rendering if the window is not transparent
        // https://developer.apple.com/documentation/metal/managing-your-game-window-for-metal-in-macos
        layer.setOpaque(!transparent);
        layer.setMaximumDrawableCount(3);
        // Allow texture reading for visual tests and UI automation screenshots
        // (captures without ScreenCaptureKit). Only debug builds pay the
        // presentation cost, even when `test-support` is compiled in.
        #[cfg(all(feature = "test-support", debug_assertions))]
        layer.setFramebufferOnly(false);
        layer.setAllowsNextDrawableTimeout(false);
        layer.setNeedsDisplayOnBoundsChange(true);
        // UIKit sizes a view's backing layer itself; only AppKit-hosted
        // layers need to track their superlayer's bounds.
        #[cfg(target_os = "macos")]
        layer.setAutoresizingMask(
            objc2_quartz_core::CAAutoresizingMask::LayerWidthSizable
                | objc2_quartz_core::CAAutoresizingMask::LayerHeightSizable,
        );
    }

    /// Creates a new headless MetalRenderer for offscreen rendering without a window.
    ///
    /// This renderer can render scenes to images without requiring a CAMetalLayer,
    /// window, or AppKit. Use `render_scene_to_image()` to render scenes.
    #[cfg(any(test, feature = "bench-support", feature = "test-support"))]
    pub fn new_headless(instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>) -> Result<Self> {
        let device = Self::create_device()?;
        Self::new_internal(device, None, true, instance_buffer_pool)
    }

    #[cfg(target_os = "macos")]
    fn create_device() -> Result<Retained<ProtocolObject<dyn MTLDevice>>> {
        // Prefer low‐power integrated GPUs on Intel Mac. On Apple
        // Silicon, there is only ever one GPU, so this is equivalent to
        // `metal::Device::system_default()`.
        if let Some(device) = MTLCopyAllDevices()
            .iter()
            .min_by_key(|device| (device.isRemovable(), !device.isLowPower()))
        {
            Ok(device)
        } else {
            // For some reason `all()` can return an empty list, see https://github.com/zed-industries/zed/issues/37689
            // In that case, we fall back to the system default device.
            log::error!(
                "Unable to enumerate Metal devices; attempting to use system default device"
            );
            Self::system_default_device()
        }
    }

    #[cfg(target_os = "ios")]
    fn create_device() -> Result<Retained<ProtocolObject<dyn MTLDevice>>> {
        Self::system_default_device()
    }

    fn system_default_device() -> Result<Retained<ProtocolObject<dyn MTLDevice>>> {
        MTLCreateSystemDefaultDevice().context("accessing a compatible Metal device")
    }

    fn new_internal(
        device: Retained<ProtocolObject<dyn MTLDevice>>,
        layer: Option<Retained<CAMetalLayer>>,
        opaque: bool,
        instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
    ) -> Result<Self> {
        Self::with_shaders(device, layer, opaque, instance_buffer_pool, built_in_library)
    }

    /// QUILL: a window that can't get a renderer comes back as an `Err` the
    /// caller handles, in place of 0.3.10's `expect`s, which aborted the app.
    /// `MTLCompilerService` can fail under load, so one failure gets a retry.
    fn with_shaders(
        device: Retained<ProtocolObject<dyn MTLDevice>>,
        layer: Option<Retained<CAMetalLayer>>,
        opaque: bool,
        instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
        library: impl Fn(
            &ProtocolObject<dyn MTLDevice>,
        ) -> Result<Retained<ProtocolObject<dyn MTLLibrary>>>,
    ) -> Result<Self> {
        let attempt = || {
            Self::build(
                device.clone(),
                layer.clone(),
                opaque,
                instance_buffer_pool.clone(),
                library(&device)?,
            )
        };
        attempt().or_else(|error| {
            log::warn!("Metal setup failed, trying once more: {error:#}");
            attempt()
        })
    }

    fn build(
        device: Retained<ProtocolObject<dyn MTLDevice>>,
        layer: Option<Retained<CAMetalLayer>>,
        opaque: bool,
        instance_buffer_pool: Arc<Mutex<InstanceBufferPool>>,
        library: Retained<ProtocolObject<dyn MTLLibrary>>,
    ) -> Result<Self> {
        // Shared memory can be used only if CPU and GPU share the same memory space.
        // https://developer.apple.com/documentation/metal/setting-resource-storage-modes
        // iOS does not support managed resources. Its simulator may report a
        // non-unified host GPU even though resources must still use shared storage.
        let is_unified_memory = cfg!(target_os = "ios") || device.hasUnifiedMemory();
        // Only Apple GPUs can create shared-storage textures on macOS.
        // https://developer.apple.com/documentation/metal/mtlgpufamily
        let is_apple_gpu = device.supportsFamily(MTLGPUFamily::Apple1);

        let path_tiles_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "path_tiles",
            "path_tile_vertex",
            "path_tile_fragment",
            MTLPixelFormat::BGRA8Unorm,
            true,
        )?;
        let shadows_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "shadows",
            "shadow_vertex",
            "shadow_fragment",
            MTLPixelFormat::BGRA8Unorm,
            true,
        )?;
        let quads_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "quads",
            "quad_vertex",
            "quad_fragment",
            MTLPixelFormat::BGRA8Unorm,
            true,
        )?;
        let opaque_quads_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "opaque_quads",
            "opaque_quad_vertex",
            "opaque_quad_fragment",
            MTLPixelFormat::BGRA8Unorm,
            false,
        )?;
        let underlines_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "underlines",
            "underline_vertex",
            "underline_fragment",
            MTLPixelFormat::BGRA8Unorm,
            true,
        )?;
        let monochrome_sprites_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "monochrome_sprites",
            "monochrome_sprite_vertex",
            "monochrome_sprite_fragment",
            MTLPixelFormat::BGRA8Unorm,
            true,
        )?;
        let polychrome_sprites_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "polychrome_sprites",
            "polychrome_sprite_vertex",
            "polychrome_sprite_fragment",
            MTLPixelFormat::BGRA8Unorm,
            true,
        )?;
        let surfaces_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "surfaces",
            "surface_vertex",
            "surface_fragment",
            MTLPixelFormat::BGRA8Unorm,
            true,
        )?;
        // Blending is off so that the transparent output replaces what was drawn beneath it.
        let native_views_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "native_views",
            "native_view_vertex",
            "native_view_fragment",
            MTLPixelFormat::BGRA8Unorm,
            false,
        )?;

        let depth_write_state = build_depth_stencil_state(&device, true)?;
        let depth_test_state = build_depth_stencil_state(&device, false)?;
        let depth_ignore_state = build_depth_ignore_state(&device)?;

        let hdr_surfaces_pipeline_state = build_pipeline_state(
            &device,
            &library,
            "hdr_surfaces",
            "surface_vertex",
            "hdr_surface_fragment",
            MTLPixelFormat::BGRA8Unorm,
            true,
        )?;

        let command_queue = device.newCommandQueue().context("creating command queue")?;
        let supports_shared_storage = cfg!(target_os = "ios") || is_apple_gpu;
        #[expect(
            clippy::arc_with_non_send_sync,
            reason = "PlatformWindow::sprite_atlas requires Arc, not Send or Sync"
        )]
        let sprite_atlas = Arc::new(MetalAtlas::new(device.clone(), supports_shared_storage));
        let mut cache = ptr::null_mut();
        // SAFETY: The output pointer is writable and both optional attribute dictionaries are absent.
        let status = unsafe {
            CVMetalTextureCache::create(None, None, &device, None, ptr::NonNull::from(&mut cache))
        };
        anyhow::ensure!(
            status == kCVReturnSuccess,
            "creating CoreVideo Metal texture cache: status {status}"
        );
        let cache = ptr::NonNull::new(cache).context("retrieving CoreVideo Metal texture cache")?;
        // SAFETY: The successful create call transfers ownership of this non-null cache.
        let core_video_texture_cache = unsafe { CFRetained::from_raw(cache) };

        Ok(Self {
            device,
            layer,
            presents_with_transaction: false,
            display_link_mode: DisplayLinkMode::Detached,
            is_apple_gpu,
            is_unified_memory,
            opaque,
            shows_native_views: false,
            command_queue,
            path_tiles_pipeline_state,
            shadows_pipeline_state,
            quads_pipeline_state,
            opaque_quads_pipeline_state,
            underlines_pipeline_state,
            monochrome_sprites_pipeline_state,
            polychrome_sprites_pipeline_state,
            surfaces_pipeline_state,
            native_views_pipeline_state,
            depth_write_state,
            depth_test_state,
            depth_ignore_state,
            depth_texture: None,
            hdr_surfaces_pipeline_state,
            drawable_format: MTLPixelFormat::BGRA8Unorm,
            other_pipelines: None,
            failed_format: None,
            library,
            instance_buffer_pool,
            sprite_atlas,
            core_video_texture_cache,
            #[cfg(any(test, feature = "bench-support", feature = "test-support"))]
            headless_render_target: None,
            drawables_handed_out: Vec::new(),
            lens: lens::LensState::default(),
            drawable_reads: lens::DrawableReads::default(),
            snapshots: snapshots::SnapshotPool::default(),
            presented: drawables::Presented::AsDrawn,
        })
    }

    pub fn layer(&self) -> Option<&CAMetalLayer> {
        self.layer.as_deref()
    }

    pub fn layer_ptr(&self) -> *mut CAMetalLayer {
        self.layer
            .as_ref()
            .map(|layer| Retained::as_ptr(layer).cast_mut())
            .unwrap_or(ptr::null_mut())
    }

    pub fn sprite_atlas(&self) -> &Arc<MetalAtlas> {
        &self.sprite_atlas
    }

    /// Draw the next frame into `drawable` instead of asking the layer for one.
    ///
    /// A `CAMetalDisplayLink` hands out each frame's drawable early enough for
    /// the frame to reach the display on time; `nextDrawable` would instead
    /// wait for the compositor to release one. Pass `None` once the frame is
    /// done, so a drawable the frame did not use goes back to the layer.
    ///
    /// Returns the drawable this replaces: after a frame, `Some` means the
    /// frame neither drew nor presented.
    ///
    /// This is a setter rather than a guard scoped to one frame because the
    /// window releases its lock on the renderer while GPUI draws.
    pub fn set_frame_drawable(
        &mut self,
        drawable: Option<Retained<ProtocolObject<dyn CAMetalDrawable>>>,
    ) -> Option<Retained<ProtocolObject<dyn CAMetalDrawable>>> {
        match &mut self.display_link_mode {
            DisplayLinkMode::Attached { drawable: current } => mem::replace(current, drawable),
            DisplayLinkMode::Detached => {
                // Only a live link hands out drawables, and a link is detached
                // only after it has been invalidated.
                debug_assert!(
                    drawable.is_none(),
                    "received a display link drawable while detached"
                );
                None
            }
        }
    }

    /// Record whether a `CAMetalDisplayLink` drives the layer. While one
    /// does, frames are drawn only into the drawables it hands out.
    pub fn set_display_link_attached(&mut self, attached: bool) {
        self.display_link_mode = if attached {
            DisplayLinkMode::Attached { drawable: None }
        } else {
            DisplayLinkMode::Detached
        };
    }

    pub fn is_display_link_attached(&self) -> bool {
        matches!(self.display_link_mode, DisplayLinkMode::Attached { .. })
    }

    pub fn presents_with_transaction(&self) -> bool {
        self.presents_with_transaction
    }

    pub fn set_presents_with_transaction(&mut self, presents_with_transaction: bool) {
        self.presents_with_transaction = presents_with_transaction;
        if let Some(layer) = &self.layer {
            layer.setPresentsWithTransaction(presents_with_transaction);
        }
    }

    pub fn update_drawable_size(&mut self, size: Size<DevicePixels>) {
        if let Some(layer) = &self.layer {
            let new = CGSize::new(size.width.0 as f64, size.height.0 as f64);
            let old = layer.drawableSize();
            // QUILL: a new size empties the pool; none of the known
            // drawables will be handed out again. The same size (a move
            // between displays of one scale) keeps them.
            if old.width != new.width || old.height != new.height {
                self.drawables_handed_out.clear();
            }
            layer.setDrawableSize(new);
        }
    }

    /// QUILL: frees the layer's spare drawables while the window is idle.
    ///
    /// A `CAMetalLayer` keeps up to `maximumDrawableCount` (3) window-sized
    /// surfaces in its pool for as long as the window lives, 33 MB each for
    /// a 1700×1240 pt window at 2x. A drawable size change empties the pool;
    /// setting the size back right away leaves only the surface on screen,
    /// which the layer keeps showing until the next present. The next frame
    /// allocates a fresh drawable.
    pub fn release_idle_drawables(&mut self) {
        self.drawables_handed_out.clear();
        // QUILL: the glass atlas goes too. Readable drawables stay readable
        // (`lens::DrawableReads` counts frames, not time): the first frame
        // after idling usually has its glass again, and a display link's
        // drawable can't be swapped for a readable one (`drawables.rs`).
        self.lens.release();
        let Some(layer) = &self.layer else {
            return;
        };
        let size = layer.drawableSize();
        if size.width <= 1.0 || size.height <= 1.0 {
            return;
        }
        layer.setDrawableSize(CGSize::new(1.0, 1.0));
        layer.setDrawableSize(size);
    }

    /// QUILL: frees the layer's spare drawables while the window keeps
    /// drawing.
    ///
    /// A burst (the first frames, a scroll) can fill the pool with all three
    /// drawables, and the layer keeps them for as long as anything animates,
    /// so the idle release above never runs. A slow animation (a looping
    /// custom emoji at 30 or 60 fps on a 120 Hz display) only cycles two:
    /// the third, a whole window-sized surface, sits unused. A drawable not
    /// handed out for `SPARE_DRAWABLE_AFTER` while others are is such a
    /// spare: empty the pool as the idle release does, and the next frames
    /// allocate only the drawables they use. Content that needs all three
    /// keeps handing each one out, so this never fires for it. Two are the
    /// floor (one on screen, one to draw into): emptying a pool of two
    /// frees nothing the next frames don't allocate again.
    fn trim_spare_drawables(&mut self) {
        if self.drawables_handed_out.len() < 3 {
            return;
        }
        let now = Instant::now();
        let age = |at: &Instant| now.saturating_duration_since(*at);
        let spare = self
            .drawables_handed_out
            .iter()
            .any(|(_, at)| age(at) >= SPARE_DRAWABLE_AFTER);
        let drawing = self
            .drawables_handed_out
            .iter()
            .any(|(_, at)| age(at) < DRAWING_WITHIN);
        if spare && drawing {
            self.release_idle_drawables();
        }
    }

    /// QUILL: records that `drawable` was handed out now. A pooled drawable
    /// keeps its IOSurface, so the surface names it.
    fn note_drawable_handed_out(&mut self, drawable: &ProtocolObject<dyn CAMetalDrawable>) {
        let texture = drawable.texture();
        // SAFETY: `iosurface` is a getter returning an unretained IOSurfaceRef
        // (or null).
        let surface: SurfaceRef = unsafe { objc2::msg_send![&*texture, iosurface] };
        if surface.0.is_null() {
            return;
        }
        let id = surface.0 as usize;
        let now = Instant::now();
        match self
            .drawables_handed_out
            .iter_mut()
            .find(|(known, _)| *known == id)
        {
            Some((_, at)) => *at = now,
            None => self.drawables_handed_out.push((id, now)),
        }
    }

    pub fn update_transparency(&mut self, transparent: bool) {
        self.opaque = !transparent;
        if let Some(layer) = &self.layer {
            layer.setOpaque(self.opaque && !self.shows_native_views);
        }
    }

    pub fn destroy(&self) {
        // nothing to do
    }

    pub fn draw(&mut self, scene: &Scene) {
        let layer = match &self.layer {
            Some(l) => l.clone(),
            None => {
                log::error!(
                    "draw() called on headless renderer - use render_scene_to_image() instead"
                );
                return;
            }
        };
        // Native views sit beneath the layer, so it can only be opaque, and
        // eligible for direct-to-display presentation, while none are showing.
        let shows_native_views = !scene.native_views.is_empty();
        if shows_native_views != self.shows_native_views {
            self.shows_native_views = shows_native_views;
            layer.setOpaque(self.opaque && !shows_native_views);
        }
        // QUILL: the frame's snapshots are drawn first, on command buffers
        // of their own, so a frame skipped below (no drawable, a drawable
        // outside the display link's update, a failed frame) still fills
        // and releases them.
        self.draw_snapshot_scenes(scene);
        // QUILL: free spares a burst left before taking the next drawable.
        self.trim_spare_drawables();
        // QUILL: glass reads the drawable back, so a frame with lenses asks
        // the layer for readable drawables (`lens::DrawableReads`).
        self.drawable_reads = self.drawable_reads.after_frame(scene.lenses.len());
        Self::follow_drawable_reads(&layer, self.drawable_reads);
        // QUILL: a frame drawn outside its link's update, or skipped below
        // in one, is presented again in the next update.
        self.presented = drawables::Presented::AsDrawn;
        let (drawable, source) = match &mut self.display_link_mode {
            DisplayLinkMode::Attached { drawable } => {
                let Some(drawable) = drawable.take() else {
                    log::error!("skipped a frame drawn outside its display link's update");
                    self.presented = drawables::Presented::Owed;
                    return;
                };
                (Some(drawable), DrawableSource::Link)
            }
            DisplayLinkMode::Detached => {
                crate::display_link::invalidate_retired();
                (layer.nextDrawable(), DrawableSource::Layer)
            }
        };
        let Some(drawable) = drawable
            .and_then(|drawable| self.usable_drawable(&layer, drawable, source))
            .map(|drawable| self.readable_drawable(&layer, drawable, source, !scene.lenses.is_empty()))
        else {
            if self.presented == drawables::Presented::AsDrawn {
                log::error!(
                    "failed to retrieve next drawable, drawable size: {:?}",
                    layer.drawableSize()
                );
            }
            return;
        };
        // A display link's drawable was sized when the link handed it out,
        // which can be before a resize the layer has since taken.
        let texture = drawable.texture();
        let viewport_size: Size<DevicePixels> = size(
            (texture.width() as i32).into(),
            (texture.height() as i32).into(),
        );
        self.note_drawable_handed_out(&drawable);

        let command_buffer = match self.render_frame(scene, &drawable.texture(), viewport_size) {
            Ok(command_buffer) => command_buffer,
            Err(error) => {
                log::error!("failed to render: {error:#}");
                return;
            }
        };

        if self.presents_with_transaction {
            command_buffer.commit();
            command_buffer.waitUntilScheduled();
            drawable.present();
        } else {
            command_buffer.presentDrawable(ProtocolObject::from_ref(&*drawable));
            command_buffer.commit();
        }
    }

    /// QUILL: a drawable in a format the pipelines can draw. When the float
    /// set can't be built, the layer leaves extended range for the 8-bit set,
    /// which the frame in hand and every later one draws with, display link
    /// or not. The HDR picture comes out clamped to standard range.
    ///
    /// A display link's drawable can't be swapped (`drawables.rs`): the
    /// frame is skipped and presented again in the link's next update,
    /// which hands out a drawable in the new format.
    fn usable_drawable(
        &mut self,
        layer: &CAMetalLayer,
        drawable: Retained<ProtocolObject<dyn CAMetalDrawable>>,
        source: DrawableSource,
    ) -> Option<Retained<ProtocolObject<dyn CAMetalDrawable>>> {
        let Err(error) = self.match_drawable_format(drawable.texture().pixelFormat()) else {
            return Some(drawable);
        };
        log::error!("leaving extended range: {error:#}");
        leave_extended_range(layer);
        let swap = source.wrong_format();
        self.presented = swap.into();
        match swap {
            Swap::AskLayer => layer.nextDrawable(),
            Swap::Keep | Swap::DrawAgainNext => None,
        }
    }

    /// QUILL: the layer's drawables are readable while glass is drawn.
    /// Debug builds with `test-support` keep them readable throughout, for
    /// captures.
    fn follow_drawable_reads(layer: &CAMetalLayer, reads: lens::DrawableReads) {
        let framebuffer_only =
            !reads.readable() && !cfg!(all(feature = "test-support", debug_assertions));
        if layer.framebufferOnly() != framebuffer_only {
            layer.setFramebufferOnly(framebuffer_only);
        }
    }

    /// QUILL: a frame with glass needs a drawable it can read. One the
    /// layer handed out before it turned readable is swapped for the
    /// layer's next; when none comes, the glass draws flat. A display
    /// link's can't be swapped (`drawables.rs`): its glass draws flat, and
    /// the link's next update presents the scene again into a drawable
    /// made since the layer turned readable.
    fn readable_drawable(
        &mut self,
        layer: &CAMetalLayer,
        drawable: Retained<ProtocolObject<dyn CAMetalDrawable>>,
        source: DrawableSource,
        has_glass: bool,
    ) -> Retained<ProtocolObject<dyn CAMetalDrawable>> {
        let swap = source.for_glass(has_glass, drawable.texture().isFramebufferOnly());
        self.presented = swap.into();
        match swap {
            Swap::AskLayer => layer.nextDrawable().unwrap_or(drawable),
            Swap::Keep | Swap::DrawAgainNext => drawable,
        }
    }

    /// QUILL: whether the screen is short of the last scene GPUI drew: the
    /// window asks the display link's next update to present it again.
    pub fn owes_present(&self) -> bool {
        self.presented == drawables::Presented::Owed
    }

    fn render_frame(
        &mut self,
        scene: &Scene,
        texture: &ProtocolObject<dyn MTLTexture>,
        viewport_size: Size<DevicePixels>,
    ) -> Result<Retained<ProtocolObject<dyn MTLCommandBuffer>>> {
        self.match_drawable_format(texture.pixelFormat())?;
        let mut writer = InstanceBufferWriter::new(
            &self.device,
            &self.instance_buffer_pool,
            self.is_unified_memory,
        )?;
        // QUILL: the frame's glass is planned first: which lens backdrops
        // are copied, kept from last frame or drawn again sharper, and
        // where inked labels read them.
        let mut surface_textures = Vec::new();
        let glass = self
            .plan_glass(scene, texture, &mut writer, &mut surface_textures)
            .inspect_err(|error| log::error!("glass draws flat: {error:#}"))
            .ok()
            .flatten();
        let instance_bindings = write_instances(scene, &mut writer, glass.as_ref()).with_context(|| {
            format!(
                "writing instance data: {} paths, {} shadows, {} quads, {} underlines, {} mono, {} poly, {} surfaces",
                scene.paths.len(),
                scene.shadows.len(),
                scene.quads.len(),
                scene.underlines.len(),
                scene.monochrome_sprites.len(),
                scene.polychrome_sprites.len(),
                scene.surfaces.len(),
            )
        })?;
        // QUILL: what this frame copies is kept for the next one first; a
        // batch that fails to fill its slots, or a frame that fails, forgets
        // it again.
        match &glass {
            Some(glass) => self.lens.keep(glass),
            None => self.lens.forget(),
        }
        let command_buffer = self
            .draw_primitives_to_texture(
                scene,
                &instance_bindings,
                &mut writer,
                texture,
                viewport_size,
                &mut surface_textures,
                glass.as_ref(),
                self.frame_clear(),
            )
            .inspect_err(|_| self.lens.forget())?;

        let instance_buffer_pool = self.instance_buffer_pool.clone();
        let frame_resources = Cell::new(Some((writer.finish(), surface_textures)));
        if let Some(glass) = &glass {
            glass.commit_renders();
        }
        // QUILL (glass effects): `QUILL_GPU_PROBE=1` prints each frame's GPU
        // time and its lens count to stderr ("GLASSGPU main <ms> <lenses>"),
        // for the budget table in docs/decisions/glass-effects.md.
        let probe_lenses = gpu_probe_on().then_some(scene.lenses.len());
        let block = RcBlock::new(
            move |buffer: ptr::NonNull<ProtocolObject<dyn MTLCommandBuffer>>| {
                if let Some((instance_buffer, _surface_textures)) = frame_resources.take() {
                    instance_buffer_pool.lock().release(instance_buffer);
                }
                if let Some(lenses) = probe_lenses {
                    // SAFETY: Metal passes the completed, live command buffer.
                    let buffer = unsafe { buffer.as_ref() };
                    let ms = (buffer.GPUEndTime() - buffer.GPUStartTime()) * 1000.0;
                    eprintln!("GLASSGPU main {ms:.4} {lenses}");
                }
            },
        );
        // SAFETY: Metal copies the live block and invokes it once after this frame completes.
        // Its captures own the frame resources and use the pool mutex when returning the buffer.
        unsafe {
            command_buffer.addCompletedHandler(RcBlock::as_ptr(&block));
        }

        Ok(command_buffer)
    }

    /// Renders the scene to a texture and returns the pixel data as an RGBA image.
    /// This does not present the frame to screen - useful for visual testing
    /// where we want to capture what would be rendered without displaying it.
    ///
    /// Note: This requires a layer-backed renderer. For headless rendering,
    /// use `render_scene_to_image()` instead.
    #[cfg(any(test, feature = "test-support"))]
    pub fn render_to_image(&mut self, scene: &Scene) -> Result<RgbaImage> {
        let layer = self
            .layer
            .clone()
            .ok_or_else(|| anyhow::anyhow!("render_to_image requires a layer-backed renderer"))?;
        let viewport_size = layer.drawableSize();
        let viewport_size: Size<DevicePixels> = size(
            (viewport_size.width.ceil() as i32).into(),
            (viewport_size.height.ceil() as i32).into(),
        );
        // CAMetalDisplayLink owns the drawable, so render into a texture we own.
        self.render_offscreen(scene, viewport_size)
    }

    /// QUILL: a window's frame, rendered offscreen at the layer's drawable
    /// size and read back; nothing is presented and the screen is not
    /// read. Backs `Window::capture_frame` on macOS.
    pub fn capture_scene(&mut self, scene: &Scene) -> Result<RgbaImage> {
        let layer = self
            .layer
            .clone()
            .ok_or_else(|| anyhow::anyhow!("the window has no layer to capture"))?;
        let drawable = layer.drawableSize();
        let viewport_size: Size<DevicePixels> = size(
            (drawable.width.ceil() as i32).into(),
            (drawable.height.ceil() as i32).into(),
        );
        self.render_scene_to_image(scene, viewport_size)
    }

    /// Renders a scene to an image without requiring a window or CAMetalLayer.
    ///
    /// This is the primary method for headless rendering. It creates an offscreen
    /// texture, renders the scene to it, and returns the pixel data as an RGBA image.
    ///
    /// QUILL: built in every configuration; a window renders its own scene
    /// through it for in-process captures ([`Self::capture_scene`]).
    pub fn render_scene_to_image(
        &mut self,
        scene: &Scene,
        size: Size<DevicePixels>,
    ) -> Result<RgbaImage> {
        if size.width.0 <= 0 || size.height.0 <= 0 {
            anyhow::bail!("Invalid size for render_scene_to_image: {:?}", size);
        }

        // Headless callers do not have a Cocoa event-loop pool to release
        // autoreleased command buffers and render-pass descriptors.
        objc2::rc::autoreleasepool(|_| {
            // QUILL: the scene's snapshots first, as a window frame does.
            self.draw_snapshot_scenes(scene);
            self.render_offscreen(scene, size)
        })
    }

    /// Renders a scene into a new offscreen texture of `size` and reads it
    /// back, without presenting anything.
    fn render_offscreen(&mut self, scene: &Scene, size: Size<DevicePixels>) -> Result<RgbaImage> {
        if size.width.0 <= 0 || size.height.0 <= 0 {
            anyhow::bail!("Invalid size for render_offscreen: {:?}", size);
        }

        let texture_descriptor = MTLTextureDescriptor::new();
        // SAFETY: The dimensions are positive and this descriptor creates a 2D texture.
        unsafe {
            texture_descriptor.setWidth(size.width.0 as usize);
            texture_descriptor.setHeight(size.height.0 as usize);
        }
        texture_descriptor.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        texture_descriptor.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        // Like the atlas, only Apple GPUs can create shared textures on macOS;
        // Intel Macs cannot, even with unified memory. iOS has no managed storage.
        let uses_shared_storage = cfg!(target_os = "ios") || self.is_apple_gpu;
        texture_descriptor.setStorageMode(if uses_shared_storage {
            MTLStorageMode::Shared
        } else {
            MTLStorageMode::Managed
        });
        let target_texture = self
            .device
            .newTextureWithDescriptor(&texture_descriptor)
            .context("allocating offscreen render target")?;

        let command_buffer = self.render_frame(scene, &target_texture, size)?;

        // Managed textures require an explicit blit synchronize before the CPU
        // can read back the rendered data. Without this, get_bytes returns
        // stale zeros.
        if !uses_shared_storage {
            let blit = command_buffer
                .blitCommandEncoder()
                .context("creating blit command encoder")?;
            blit.synchronizeResource(ProtocolObject::from_ref(&*target_texture));
            blit.endEncoding();
        }

        command_buffer.commit();
        command_buffer.waitUntilCompleted();

        read_texture_to_image(&target_texture)
    }

    /// Renders a scene to a reused offscreen texture without reading pixels
    /// back or blocking on GPU completion.
    ///
    /// This mirrors the CPU cost of presenting a frame to a window (scene
    /// encoding, instance buffer writes, command submission) and is used by
    /// headless benchmark rendering, where the produced pixels are never
    /// inspected.
    #[cfg(any(test, feature = "bench-support", feature = "test-support"))]
    pub fn render_scene(&mut self, scene: &Scene, size: Size<DevicePixels>) -> Result<()> {
        if size.width.0 <= 0 || size.height.0 <= 0 {
            anyhow::bail!("Invalid size for render_scene: {:?}", size);
        }

        objc2::rc::autoreleasepool(|_| {
            // QUILL: the scene's snapshots first, as a window frame does.
            self.draw_snapshot_scenes(scene);
            let needs_new_target = self.headless_render_target.as_ref().is_none_or(|texture| {
                texture.width() != size.width.0 as usize
                    || texture.height() != size.height.0 as usize
            });
            if needs_new_target {
                let texture_descriptor = MTLTextureDescriptor::new();
                unsafe {
                    texture_descriptor.setWidth(size.width.0 as usize);
                    texture_descriptor.setHeight(size.height.0 as usize);
                }
                texture_descriptor.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
                texture_descriptor
                    .setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
                texture_descriptor.setStorageMode(MTLStorageMode::Private);
                self.headless_render_target = Some(
                    self.device
                        .newTextureWithDescriptor(&texture_descriptor)
                        .context("allocating headless render target")?,
                );
            }
            let target_texture = self
                .headless_render_target
                .clone()
                .context("retrieving headless render target")?;

            let command_buffer = self.render_frame(scene, &target_texture, size)?;

            // Commit without waiting, mirroring presentation to a real window where
            // the CPU doesn't block on the GPU.
            command_buffer.commit();
            Ok(())
        })
    }

    fn draw_primitives_to_texture(
        &mut self,
        scene: &Scene,
        instance_bindings: &InstanceBindings,
        writer: &mut InstanceBufferWriter,
        texture: &ProtocolObject<dyn MTLTexture>,
        viewport_size: Size<DevicePixels>,
        surface_textures: &mut Vec<CFRetained<CVMetalTexture>>,
        glass: Option<&lens::GlassFrame>,
        // QUILL: what the target is cleared to (a snapshot's own colour).
        clear: MTLClearColor,
    ) -> Result<Retained<ProtocolObject<dyn MTLCommandBuffer>>> {
        let command_queue = self.command_queue.clone();
        let command_buffer = command_queue
            .commandBuffer()
            .context("creating command buffer")?;
        let depth_texture = self.depth_texture_matching(texture)?;

        let mut command_encoder = new_command_encoder_for_texture(
            &command_buffer,
            texture,
            &depth_texture,
            viewport_size,
            Some(clear),
        )?;
        // QUILL: a frame with lenses draws without the opaque pre-pass. A lens
        // batch ends the pass (its depth buffer is memoryless and goes with
        // it), and opaque quads drawn ahead would show in the glass's backdrop
        // before their turn. A zero depth scale keeps the quad pass from
        // culling them, and depth neither tests nor writes.
        let depth_scale = if scene.lenses.is_empty() {
            quad_depth_scale(scene.quads.len())
        } else {
            0.0
        };
        set_frame_globals(&command_encoder, viewport_size, depth_scale);
        if depth_scale > 0.0 {
            self.draw_opaque_quads(scene.quads.len(), instance_bindings, &command_encoder);
            command_encoder.setDepthStencilState(Some(&self.depth_test_state));
        } else {
            command_encoder.setDepthStencilState(Some(&self.depth_ignore_state));
        }

        let mut quad_cursor: u32 = 0;
        let mut path_tile_cursor = 0;
        for batch in scene.batches() {
            // Quad shaders assign each quad its own depth.
            // In every other batch type, the depth is fixed to the position of the quad cursor.
            let (znear, zfar) = if matches!(&batch, PrimitiveBatch::Quads(_)) {
                (0.0, 1.0)
            } else {
                let depth = quad_depth(quad_cursor, depth_scale) as f64;
                (depth, depth)
            };
            command_encoder.setViewport(MTLViewport {
                originX: 0.0,
                originY: 0.0,
                width: i32::from(viewport_size.width) as f64,
                height: i32::from(viewport_size.height) as f64,
                znear,
                zfar,
            });

            match batch {
                PrimitiveBatch::Shadows(range) => {
                    self.draw_shadows(range, instance_bindings, &command_encoder)
                }
                PrimitiveBatch::Quads(range) => {
                    quad_cursor += range.len() as u32;
                    self.draw_quads(range, &instance_bindings.quads, &command_encoder)
                }
                PrimitiveBatch::Paths(range) => {
                    let tiles_start = path_tile_cursor;
                    path_tile_cursor += scene.paths[range]
                        .iter()
                        .map(PaintedPath::tile_count)
                        .sum::<usize>();
                    self.draw_path_tiles(
                        tiles_start..path_tile_cursor,
                        instance_bindings,
                        &command_encoder,
                    )
                }
                PrimitiveBatch::Underlines(range) => {
                    self.draw_underlines(range, instance_bindings, &command_encoder)
                }
                PrimitiveBatch::MonochromeSprites { texture_id, range } => self
                    .draw_monochrome_sprites(
                        texture_id,
                        range,
                        instance_bindings,
                        &command_encoder,
                    ),
                PrimitiveBatch::PolychromeSprites { texture_id, range } => self
                    .draw_polychrome_sprites(
                        texture_id,
                        range,
                        instance_bindings,
                        &command_encoder,
                    ),
                PrimitiveBatch::Surfaces(range) => {
                    if let Err(error) = self.draw_surfaces(
                        &scene.surfaces[range.clone()],
                        range.start,
                        instance_bindings,
                        &command_encoder,
                        surface_textures,
                    ) {
                        command_encoder.endEncoding();
                        return Err(error);
                    }
                }
                PrimitiveBatch::NativeViews(range) => {
                    self.draw_native_views(range, instance_bindings, &command_encoder)
                }
                PrimitiveBatch::Lenses(range) => {
                    self.draw_lenses(
                        glass,
                        range,
                        instance_bindings,
                        writer,
                        texture,
                        &depth_texture,
                        viewport_size,
                        &command_buffer,
                        &mut command_encoder,
                    )?;
                }
                PrimitiveBatch::Snapshots(range) => {
                    self.draw_snapshots(scene, range, instance_bindings, &command_encoder)
                }
                PrimitiveBatch::SubpixelSprites { .. } => unreachable!(),
            }
        }

        command_encoder.endEncoding();

        Ok(command_buffer)
    }

    /// QUILL: what a frame's target is cleared to: black, opaque when the
    /// window is.
    fn frame_clear(&self) -> MTLClearColor {
        MTLClearColor {
            red: 0.,
            green: 0.,
            blue: 0.,
            alpha: if self.opaque { 1. } else { 0. },
        }
    }

    fn depth_texture_matching(
        &mut self,
        target: &ProtocolObject<dyn MTLTexture>,
    ) -> Result<Retained<ProtocolObject<dyn MTLTexture>>> {
        if let Some(depth_texture) = &self.depth_texture
            && depth_texture.width() == target.width()
            && depth_texture.height() == target.height()
        {
            return Ok(depth_texture.clone());
        }
        let descriptor = MTLTextureDescriptor::new();
        // SAFETY: The size is the size of an existing render target, which the device supports.
        unsafe {
            descriptor.setWidth(target.width());
            descriptor.setHeight(target.height());
        }
        descriptor.setPixelFormat(MTLPixelFormat::Depth32Float);
        descriptor.setUsage(MTLTextureUsage::RenderTarget);
        // The depth buffer never leaves its render pass, so Apple GPUs can keep
        // it in tile memory without allocating any backing store.
        descriptor.setStorageMode(if self.is_apple_gpu {
            MTLStorageMode::Memoryless
        } else {
            MTLStorageMode::Private
        });
        let depth_texture = self
            .device
            .newTextureWithDescriptor(&descriptor)
            .context("allocating depth texture")?;
        self.depth_texture = Some(depth_texture.clone());
        Ok(depth_texture)
    }

    fn draw_opaque_quads(
        &self,
        quad_count: usize,
        instance_bindings: &InstanceBindings,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) {
        if quad_count == 0 {
            return;
        }
        let quad_count_value = quad_count as u32;

        command_encoder.setRenderPipelineState(&self.opaque_quads_pipeline_state);
        command_encoder.setDepthStencilState(Some(&self.depth_write_state));
        // SAFETY: The scene's quads and the copied count match the shader layouts and slots.
        // Metal retains the buffer, and writer.finish publishes its writes.
        unsafe {
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&instance_bindings.quads.buffer),
                instance_bindings.quads.offset,
                QuadInputIndex::Quads as usize,
            );
            command_encoder.setVertexBytes_length_atIndex(
                ptr::NonNull::from(&quad_count_value).cast(),
                mem::size_of_val(&quad_count_value),
                QuadInputIndex::QuadCount as usize,
            );
            command_encoder.drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                MTLPrimitiveType::TriangleStrip,
                0,
                4,
                quad_count,
                0,
            );
        }
    }

    fn draw_native_views(
        &self,
        native_views: Range<usize>,
        instance_bindings: &InstanceBindings,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) {
        if native_views.is_empty() {
            return;
        }

        command_encoder.setRenderPipelineState(&self.native_views_pipeline_state);
        // SAFETY: The SurfaceBounds instances match the shader layout and slot.
        // Metal retains the buffer, and writer.finish publishes its writes.
        unsafe {
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&instance_bindings.native_views.buffer),
                instance_bindings.native_views.offset,
                SurfaceInputIndex::Surfaces as usize,
            );
            command_encoder.drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                MTLPrimitiveType::TriangleStrip,
                0,
                4,
                native_views.len(),
                native_views.start,
            );
        }
    }

    fn draw_path_tiles(
        &self,
        tiles: Range<usize>,
        instance_bindings: &InstanceBindings,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) {
        if tiles.is_empty() {
            return;
        }

        command_encoder.setRenderPipelineState(&self.path_tiles_pipeline_state);
        // SAFETY: The scene's tile range and its curves and paints match the uploaded shader layouts and slots.
        // Metal retains the buffers, and writer.finish publishes their writes.
        unsafe {
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&instance_bindings.path_tiles.buffer),
                instance_bindings.path_tiles.offset,
                PathTileInputIndex::Tiles as usize,
            );
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&instance_bindings.path_paints.buffer),
                instance_bindings.path_paints.offset,
                PathTileInputIndex::Paints as usize,
            );
            command_encoder.setFragmentBuffer_offset_atIndex(
                Some(&instance_bindings.path_tiles.buffer),
                instance_bindings.path_tiles.offset,
                PathTileInputIndex::Tiles as usize,
            );
            command_encoder.setFragmentBuffer_offset_atIndex(
                Some(&instance_bindings.path_curves.buffer),
                instance_bindings.path_curves.offset,
                PathTileInputIndex::Curves as usize,
            );
            command_encoder.setFragmentBuffer_offset_atIndex(
                Some(&instance_bindings.path_paints.buffer),
                instance_bindings.path_paints.offset,
                PathTileInputIndex::Paints as usize,
            );
            command_encoder.drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                MTLPrimitiveType::TriangleStrip,
                0,
                4,
                tiles.len(),
                tiles.start,
            );
        }
    }

    fn draw_shadows(
        &self,
        shadows: Range<usize>,
        instance_bindings: &InstanceBindings,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) {
        if shadows.is_empty() {
            return;
        }

        command_encoder.setRenderPipelineState(&self.shadows_pipeline_state);
        // SAFETY: The scene's shadow range matches the uploaded shader layouts and slots.
        // Metal retains the buffers, and writer.finish publishes their writes.
        unsafe {
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&instance_bindings.shadows.buffer),
                instance_bindings.shadows.offset,
                ShadowInputIndex::Shadows as usize,
            );
            command_encoder.setFragmentBuffer_offset_atIndex(
                Some(&instance_bindings.shadows.buffer),
                instance_bindings.shadows.offset,
                ShadowInputIndex::Shadows as usize,
            );
            command_encoder.drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                MTLPrimitiveType::TriangleStrip,
                0,
                4,
                shadows.len(),
                shadows.start,
            );
        }
    }

    fn draw_quads(
        &self,
        quads: Range<usize>,
        binding: &InstanceBinding,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) {
        if quads.is_empty() {
            return;
        }

        command_encoder.setRenderPipelineState(&self.quads_pipeline_state);
        // SAFETY: The scene's quad range matches the uploaded shader layouts and slots.
        // Metal retains the buffers, and writer.finish publishes their writes.
        unsafe {
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&binding.buffer),
                binding.offset,
                QuadInputIndex::Quads as usize,
            );
            command_encoder.setFragmentBuffer_offset_atIndex(
                Some(&binding.buffer),
                binding.offset,
                QuadInputIndex::Quads as usize,
            );
            command_encoder.drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                MTLPrimitiveType::TriangleStrip,
                0,
                4,
                quads.len(),
                quads.start,
            );
        }
    }

    fn draw_underlines(
        &self,
        underlines: Range<usize>,
        instance_bindings: &InstanceBindings,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) {
        if underlines.is_empty() {
            return;
        }

        command_encoder.setRenderPipelineState(&self.underlines_pipeline_state);
        // SAFETY: The scene's underline range matches the uploaded shader layouts and slots.
        // Metal retains the buffers, and writer.finish publishes their writes.
        unsafe {
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&instance_bindings.underlines.buffer),
                instance_bindings.underlines.offset,
                UnderlineInputIndex::Underlines as usize,
            );
            command_encoder.setFragmentBuffer_offset_atIndex(
                Some(&instance_bindings.underlines.buffer),
                instance_bindings.underlines.offset,
                UnderlineInputIndex::Underlines as usize,
            );
            command_encoder.drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                MTLPrimitiveType::TriangleStrip,
                0,
                4,
                underlines.len(),
                underlines.start,
            );
        }
    }

    fn draw_monochrome_sprites(
        &self,
        texture_id: AtlasTextureId,
        sprites: Range<usize>,
        instance_bindings: &InstanceBindings,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) {
        if sprites.is_empty() {
            return;
        }

        let Some(texture) = self.sprite_atlas.metal_texture(texture_id) else {
            return;
        };
        let texture_size = size(
            DevicePixels(texture.width() as i32),
            DevicePixels(texture.height() as i32),
        );
        command_encoder.setRenderPipelineState(&self.monochrome_sprites_pipeline_state);
        // SAFETY: The scene's sprite range and copied sizes match the shader layouts and slots.
        // Metal retains the buffers and texture, and writer.finish publishes the instance writes.
        unsafe {
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&instance_bindings.monochrome_sprites.buffer),
                instance_bindings.monochrome_sprites.offset,
                SpriteInputIndex::Sprites as usize,
            );
            command_encoder.setVertexBytes_length_atIndex(
                ptr::NonNull::from(&texture_size).cast(),
                mem::size_of_val(&texture_size),
                SpriteInputIndex::AtlasTextureSize as usize,
            );
            command_encoder.setFragmentBuffer_offset_atIndex(
                Some(&instance_bindings.monochrome_sprites.buffer),
                instance_bindings.monochrome_sprites.offset,
                SpriteInputIndex::Sprites as usize,
            );
            command_encoder.setFragmentTexture_atIndex(
                Some(&texture),
                SpriteInputIndex::AtlasTexture as usize,
            );
            // QUILL (glass stage 5): inked labels read the lens atlas; without
            // one the sprite atlas stands in and is never sampled (every
            // record then says the glass drew flat).
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&instance_bindings.lens_inks.buffer),
                instance_bindings.lens_inks.offset,
                SpriteInputIndex::Inks as usize,
            );
            command_encoder.setVertexTexture_atIndex(
                Some(instance_bindings.lens_atlas.as_deref().unwrap_or(&texture)),
                SpriteInputIndex::LensAtlas as usize,
            );
            command_encoder.drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                MTLPrimitiveType::TriangleStrip,
                0,
                4,
                sprites.len(),
                sprites.start,
            );
        }
    }

    fn draw_polychrome_sprites(
        &self,
        texture_id: AtlasTextureId,
        sprites: Range<usize>,
        instance_bindings: &InstanceBindings,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    ) {
        if sprites.is_empty() {
            return;
        }

        let Some(texture) = self.sprite_atlas.metal_texture(texture_id) else {
            return;
        };
        let texture_size = size(
            DevicePixels(texture.width() as i32),
            DevicePixels(texture.height() as i32),
        );
        command_encoder.setRenderPipelineState(&self.polychrome_sprites_pipeline_state);
        // SAFETY: The scene's sprite range and copied sizes match the shader layouts and slots.
        // Metal retains the buffers and texture, and writer.finish publishes the instance writes.
        unsafe {
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&instance_bindings.polychrome_sprites.buffer),
                instance_bindings.polychrome_sprites.offset,
                SpriteInputIndex::Sprites as usize,
            );
            command_encoder.setVertexBytes_length_atIndex(
                ptr::NonNull::from(&texture_size).cast(),
                mem::size_of_val(&texture_size),
                SpriteInputIndex::AtlasTextureSize as usize,
            );
            command_encoder.setFragmentBuffer_offset_atIndex(
                Some(&instance_bindings.polychrome_sprites.buffer),
                instance_bindings.polychrome_sprites.offset,
                SpriteInputIndex::Sprites as usize,
            );
            command_encoder.setFragmentTexture_atIndex(
                Some(&texture),
                SpriteInputIndex::AtlasTexture as usize,
            );
            command_encoder.drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                MTLPrimitiveType::TriangleStrip,
                0,
                4,
                sprites.len(),
                sprites.start,
            );
        }
    }

    fn draw_surfaces(
        &mut self,
        surfaces: &[PaintSurface],
        first_surface: usize,
        instance_bindings: &InstanceBindings,
        command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
        surface_textures: &mut Vec<CFRetained<CVMetalTexture>>,
    ) -> Result<()> {
        if surfaces.is_empty() {
            return Ok(());
        }

        command_encoder.setRenderPipelineState(&self.surfaces_pipeline_state);
        // SAFETY: The SurfaceBounds instances match the shader layout and slot.
        // Metal retains the buffers, and writer.finish publishes CPU writes before submission.
        unsafe {
            command_encoder.setVertexBuffer_offset_atIndex(
                Some(&instance_bindings.surfaces.buffer),
                instance_bindings.surfaces.offset,
                SurfaceInputIndex::Surfaces as usize,
            );
        }

        // QUILL: the surface's pixel format picks the pipeline; a format
        // nothing here draws is skipped (upstream asserted).
        let extended_output = u32::from(self.drawable_format == MTLPixelFormat::RGBA16Float);
        for (index, surface) in surfaces.iter().enumerate() {
            let image_buffer = &surface.image_buffer;
            let texture_size = size(
                DevicePixels::from(CVPixelBufferGetWidth(image_buffer) as i32),
                DevicePixels::from(CVPixelBufferGetHeight(image_buffer) as i32),
            );
            // QUILL: a texture over one plane of the buffer, or `None` when
            // CoreVideo can't make one (the surface is then skipped).
            let texture_from_image = |pixel_format, width, height, plane| {
                let mut texture = ptr::null_mut();
                // SAFETY: a live cache and pixel buffer; on success `texture`
                // holds a +1 CVMetalTexture, which `from_raw` takes over.
                let status = unsafe {
                    CVMetalTextureCache::create_texture_from_image(
                        None,
                        &self.core_video_texture_cache,
                        image_buffer,
                        None,
                        pixel_format,
                        width,
                        height,
                        plane,
                        ptr::NonNull::from(&mut texture),
                    )
                };
                if status != kCVReturnSuccess {
                    return None;
                }
                ptr::NonNull::new(texture)
                    .map(|texture| unsafe { CFRetained::<CVMetalTexture>::from_raw(texture) })
            };
            let format = CVPixelBufferGetPixelFormatType(image_buffer);
            if format == kCVPixelFormatType_64RGBAHalf {
                let Some(texture) = texture_from_image(
                    MTLPixelFormat::RGBA16Float,
                    CVPixelBufferGetWidth(image_buffer),
                    CVPixelBufferGetHeight(image_buffer),
                    0,
                ) else {
                    continue;
                };
                let Some(metal_texture) = CVMetalTextureGetTexture(&texture) else {
                    continue;
                };
                let params = HdrSurfaceParams {
                    headroom: hdr_headroom(image_buffer),
                    extended_output,
                };
                command_encoder.setRenderPipelineState(&self.hdr_surfaces_pipeline_state);
                // SAFETY: the sizes are copied from live values, `metal_texture` is the
                // live MTLTexture of `texture`, which outlives this draw's encoding, and
                // the six unit vertices draw one instance within the uploaded range.
                unsafe {
                    command_encoder.setVertexBytes_length_atIndex(
                        ptr::NonNull::from(&texture_size).cast(),
                        mem::size_of_val(&texture_size),
                        SurfaceInputIndex::TextureSize as usize,
                    );
                    command_encoder.setFragmentBytes_length_atIndex(
                        ptr::NonNull::from(&params).cast(),
                        mem::size_of_val(&params),
                        0,
                    );
                    command_encoder.setFragmentTexture_atIndex(
                        Some(&metal_texture),
                        SurfaceInputIndex::YTexture as usize,
                    );
                    command_encoder
                        .drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                            MTLPrimitiveType::Triangle,
                            0,
                            6,
                            1,
                            first_surface + index,
                        );
                }
                surface_textures.push(texture);
                continue;
            }
            if format != kCVPixelFormatType_420YpCbCr8BiPlanarFullRange {
                log::warn!("skipping a surface in an unsupported pixel format {format:#x}");
                continue;
            }
            command_encoder.setRenderPipelineState(&self.surfaces_pipeline_state);

            let plane_texture = |plane, pixel_format| {
                texture_from_image(
                    pixel_format,
                    CVPixelBufferGetWidthOfPlane(image_buffer, plane),
                    CVPixelBufferGetHeightOfPlane(image_buffer, plane),
                    plane,
                )
            };
            let Some(y_texture) = plane_texture(0, MTLPixelFormat::R8Unorm) else {
                continue;
            };
            let Some(cb_cr_texture) = plane_texture(1, MTLPixelFormat::RG8Unorm)
            else {
                continue;
            };
            let (Some(y_metal), Some(cb_cr_metal)) = (
                CVMetalTextureGetTexture(&y_texture),
                CVMetalTextureGetTexture(&cb_cr_texture),
            ) else {
                continue;
            };

            // SAFETY: The size is copied from a live value, and both planes match their shader texture slots.
            // The frame completion handler retains the CoreVideo wrappers until GPU reads finish.
            unsafe {
                command_encoder.setVertexBytes_length_atIndex(
                    ptr::NonNull::from(&texture_size).cast(),
                    mem::size_of_val(&texture_size),
                    SurfaceInputIndex::TextureSize as usize,
                );
                command_encoder.setFragmentTexture_atIndex(
                    Some(&y_metal),
                    SurfaceInputIndex::YTexture as usize,
                );
                command_encoder.setFragmentTexture_atIndex(
                    Some(&cb_cr_metal),
                    SurfaceInputIndex::CbCrTexture as usize,
                );
            }
            surface_textures.extend([y_texture, cb_cr_texture]);

            // SAFETY: This draws one instance within the uploaded scene's surface range.
            unsafe {
                command_encoder.drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                    MTLPrimitiveType::TriangleStrip,
                    0,
                    4,
                    1,
                    first_surface + index,
                );
            }
        }
        Ok(())
    }

    /// QUILL: rebuild (or swap back) the pipelines that draw into the
    /// drawable when its pixel format changes (an EDR float layer and back).
    /// Path rasterization keeps its own 8-bit intermediate.
    fn match_drawable_format(&mut self, format: MTLPixelFormat) -> Result<()> {
        if format == self.drawable_format {
            return Ok(());
        }
        anyhow::ensure!(
            self.failed_format != Some(format),
            "the {format:?} pipelines did not build earlier"
        );
        let stashed = self
            .other_pipelines
            .take_if(|pipelines| pipelines.format == format);
        let incoming = match stashed {
            Some(pipelines) => pipelines,
            None => DrawablePipelines::build(&self.device, &self.library, format)
                .inspect_err(|_| self.failed_format = Some(format))?,
        };
        let outgoing = DrawablePipelines {
            format: self.drawable_format,
            path_tiles: mem::replace(&mut self.path_tiles_pipeline_state, incoming.path_tiles),
            shadows: mem::replace(&mut self.shadows_pipeline_state, incoming.shadows),
            quads: mem::replace(&mut self.quads_pipeline_state, incoming.quads),
            opaque_quads: mem::replace(
                &mut self.opaque_quads_pipeline_state,
                incoming.opaque_quads,
            ),
            underlines: mem::replace(&mut self.underlines_pipeline_state, incoming.underlines),
            monochrome_sprites: mem::replace(
                &mut self.monochrome_sprites_pipeline_state,
                incoming.monochrome_sprites,
            ),
            polychrome_sprites: mem::replace(
                &mut self.polychrome_sprites_pipeline_state,
                incoming.polychrome_sprites,
            ),
            surfaces: mem::replace(&mut self.surfaces_pipeline_state, incoming.surfaces),
            native_views: mem::replace(
                &mut self.native_views_pipeline_state,
                incoming.native_views,
            ),
            hdr_surfaces: mem::replace(&mut self.hdr_surfaces_pipeline_state, incoming.hdr_surfaces),
        };
        self.other_pipelines = Some(outgoing);
        self.drawable_format = format;
        Ok(())
    }
}

/// QUILL: the state Quill's `set_window_edr` puts back when the viewer closes.
/// That code only touches the layer when the viewer's state changes, so the
/// layer stays 8-bit after this.
fn leave_extended_range(layer: &CAMetalLayer) {
    layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
    layer.setWantsExtendedDynamicRangeContent(false);
    layer.setColorspace(None);
}

/// QUILL: the pipelines that draw into the drawable, for one pixel format.
struct DrawablePipelines {
    format: MTLPixelFormat,
    path_tiles: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    shadows: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    quads: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    opaque_quads: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    underlines: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    monochrome_sprites: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    polychrome_sprites: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    surfaces: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    native_views: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    hdr_surfaces: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
}

impl DrawablePipelines {
    /// Built as `build` builds the 8-bit set: blending on, except the
    /// opaque pre-pass and native views, which replace what is beneath.
    fn build(
        device: &ProtocolObject<dyn MTLDevice>,
        library: &ProtocolObject<dyn MTLLibrary>,
        format: MTLPixelFormat,
    ) -> Result<Self> {
        let blended = |label: &str, vertex: &str, fragment: &str| {
            build_pipeline_state(device, library, label, vertex, fragment, format, true)
        };
        let replacing = |label: &str, vertex: &str, fragment: &str| {
            build_pipeline_state(device, library, label, vertex, fragment, format, false)
        };
        Ok(Self {
            format,
            path_tiles: blended("path_tiles", "path_tile_vertex", "path_tile_fragment")?,
            shadows: blended("shadows", "shadow_vertex", "shadow_fragment")?,
            quads: blended("quads", "quad_vertex", "quad_fragment")?,
            opaque_quads: replacing("opaque_quads", "opaque_quad_vertex", "opaque_quad_fragment")?,
            underlines: blended("underlines", "underline_vertex", "underline_fragment")?,
            monochrome_sprites: blended(
                "monochrome_sprites",
                "monochrome_sprite_vertex",
                "monochrome_sprite_fragment",
            )?,
            polychrome_sprites: blended(
                "polychrome_sprites",
                "polychrome_sprite_vertex",
                "polychrome_sprite_fragment",
            )?,
            surfaces: blended("surfaces", "surface_vertex", "surface_fragment")?,
            native_views: replacing("native_views", "native_view_vertex", "native_view_fragment")?,
            hdr_surfaces: blended("hdr_surfaces", "surface_vertex", "hdr_surface_fragment")?,
        })
    }
}

/// QUILL: `hdr_surface_fragment`'s parameters (`HdrSurfaceParams` in
/// shaders.metal).
#[repr(C)]
struct HdrSurfaceParams {
    headroom: f32,
    extended_output: u32,
}

/// QUILL: the EDR headroom the app attached to an HDR pixel buffer
/// (`QuillHdrHeadroom`, a CFNumber); 1.0 without one.
fn hdr_headroom(buffer: &objc2_core_video::CVPixelBuffer) -> f32 {
    use objc2_core_foundation::{CFNumber, CFString};
    let key = CFString::from_static_str("QuillHdrHeadroom");
    // SAFETY: a null attachment mode is allowed; the copy comes back +1 and
    // `CFRetained` releases it.
    let Some(value) = (unsafe { buffer.attachment(&key, ptr::null_mut()) }) else {
        return 1.0;
    };
    value
        .downcast_ref::<CFNumber>()
        .and_then(CFNumber::as_f32)
        .filter(|headroom| headroom.is_finite() && *headroom >= 1.0)
        .unwrap_or(1.0)
}

fn new_command_encoder_for_texture(
    command_buffer: &ProtocolObject<dyn MTLCommandBuffer>,
    texture: &ProtocolObject<dyn MTLTexture>,
    depth_texture: &ProtocolObject<dyn MTLTexture>,
    viewport_size: Size<DevicePixels>,
    // QUILL: `None` loads what the target holds, for the pass a lens batch
    // reopens over what was drawn.
    clear_color: Option<MTLClearColor>,
) -> Result<Retained<ProtocolObject<dyn MTLRenderCommandEncoder>>> {
    let render_pass_descriptor = MTLRenderPassDescriptor::new();
    // SAFETY: Index zero is within the render pass's color attachment array.
    let color_attachment = unsafe {
        render_pass_descriptor
            .colorAttachments()
            .objectAtIndexedSubscript(0)
    };
    color_attachment.setTexture(Some(texture));
    color_attachment.setStoreAction(MTLStoreAction::Store);
    match clear_color {
        Some(clear_color) => {
            color_attachment.setLoadAction(MTLLoadAction::Clear);
            color_attachment.setClearColor(clear_color);
        }
        None => color_attachment.setLoadAction(MTLLoadAction::Load),
    }

    let depth_attachment = render_pass_descriptor.depthAttachment();
    depth_attachment.setTexture(Some(depth_texture));
    depth_attachment.setLoadAction(MTLLoadAction::Clear);
    depth_attachment.setClearDepth(0.0);
    // The depth buffer is only needed within this pass.
    depth_attachment.setStoreAction(MTLStoreAction::DontCare);

    let command_encoder = command_buffer
        .renderCommandEncoderWithDescriptor(&render_pass_descriptor)
        .context("creating render command encoder")?;
    command_encoder.setViewport(MTLViewport {
        originX: 0.0,
        originY: 0.0,
        width: i32::from(viewport_size.width) as f64,
        height: i32::from(viewport_size.height) as f64,
        znear: 0.0,
        zfar: 1.0,
    });
    Ok(command_encoder)
}

fn read_texture_to_image(texture: &ProtocolObject<dyn MTLTexture>) -> Result<RgbaImage> {
    let width = texture.width() as u32;
    let height = texture.height() as u32;
    let bytes_per_row = width as usize * 4;
    let mut pixels = vec![0u8; height as usize * bytes_per_row];

    let region = MTLRegion {
        origin: MTLOrigin { x: 0, y: 0, z: 0 },
        size: MTLSize {
            width: width as usize,
            height: height as usize,
            depth: 1,
        },
    };
    unsafe {
        texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
            ptr::NonNull::from(pixels.as_mut_slice()).cast(),
            bytes_per_row,
            region,
            0,
        );
    }

    // Convert BGRA to RGBA (swap B and R channels)
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }

    RgbaImage::from_raw(width, height, pixels).context("creating RGBA image from pixel data")
}

#[cfg(feature = "runtime_shaders")]
fn library_from_source(
    device: &ProtocolObject<dyn MTLDevice>,
    source: &str,
) -> Result<Retained<ProtocolObject<dyn MTLLibrary>>> {
    device
        .newLibraryWithSource_options_error(
            &NSString::from_str(source),
            Some(&objc2_metal::MTLCompileOptions::new()),
        )
        .context("building Metal library")
}

#[cfg(quill_metallib)]
fn precompiled_library(
    device: &ProtocolObject<dyn MTLDevice>,
) -> Result<Retained<ProtocolObject<dyn MTLLibrary>>> {
    device
        .newLibraryWithData_error(&dispatch2::DispatchData::from_static_bytes(
            SHADERS_METALLIB,
        ))
        .context("loading Metal library")
}

fn built_in_library(
    device: &ProtocolObject<dyn MTLDevice>,
) -> Result<Retained<ProtocolObject<dyn MTLLibrary>>> {
    #[cfg(quill_metallib)]
    let library = precompiled_library(device);
    #[cfg(all(quill_metallib, feature = "runtime_shaders"))]
    let library = library.or_else(|error| {
        log::warn!("the precompiled shaders did not load: {error:#}");
        library_from_source(device, SHADERS_SOURCE_FILE)
    });
    #[cfg(not(quill_metallib))]
    let library = library_from_source(device, SHADERS_SOURCE_FILE);
    library
}

fn build_pipeline_state(
    device: &ProtocolObject<dyn MTLDevice>,
    library: &ProtocolObject<dyn MTLLibrary>,
    label: &str,
    vertex_fn_name: &str,
    fragment_fn_name: &str,
    pixel_format: MTLPixelFormat,
    blending: bool,
) -> Result<Retained<ProtocolObject<dyn MTLRenderPipelineState>>> {
    let vertex_fn = library
        .newFunctionWithName(&NSString::from_str(vertex_fn_name))
        .with_context(|| format!("locating vertex function {vertex_fn_name}"))?;
    let fragment_fn = library
        .newFunctionWithName(&NSString::from_str(fragment_fn_name))
        .with_context(|| format!("locating fragment function {fragment_fn_name}"))?;

    let descriptor = MTLRenderPipelineDescriptor::new();
    descriptor.setLabel(Some(&NSString::from_str(label)));
    descriptor.setVertexFunction(Some(vertex_fn.as_ref()));
    descriptor.setFragmentFunction(Some(fragment_fn.as_ref()));
    // SAFETY: Index zero is within the pipeline's color attachment array.
    let color_attachment = unsafe { descriptor.colorAttachments().objectAtIndexedSubscript(0) };
    color_attachment.setPixelFormat(pixel_format);
    color_attachment.setBlendingEnabled(blending);
    color_attachment.setRgbBlendOperation(MTLBlendOperation::Add);
    color_attachment.setAlphaBlendOperation(MTLBlendOperation::Add);
    color_attachment.setSourceRGBBlendFactor(MTLBlendFactor::SourceAlpha);
    color_attachment.setSourceAlphaBlendFactor(MTLBlendFactor::One);
    color_attachment.setDestinationRGBBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
    color_attachment.setDestinationAlphaBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
    descriptor.setDepthAttachmentPixelFormat(MTLPixelFormat::Depth32Float);

    device
        .newRenderPipelineStateWithDescriptor_error(&descriptor)
        .with_context(|| format!("creating {label} render pipeline"))
}

fn build_depth_stencil_state(
    device: &ProtocolObject<dyn MTLDevice>,
    write_depth: bool,
) -> Result<Retained<ProtocolObject<dyn MTLDepthStencilState>>> {
    let descriptor = MTLDepthStencilDescriptor::new();
    descriptor.setDepthCompareFunction(MTLCompareFunction::Greater);
    descriptor.setDepthWriteEnabled(write_depth);
    device
        .newDepthStencilStateWithDescriptor(&descriptor)
        .context("creating depth stencil state")
}

/// QUILL: depth that neither tests nor writes, for frames drawn without the
/// opaque pre-pass (frames with glass).
fn build_depth_ignore_state(
    device: &ProtocolObject<dyn MTLDevice>,
) -> Result<Retained<ProtocolObject<dyn MTLDepthStencilState>>> {
    let descriptor = MTLDepthStencilDescriptor::new();
    descriptor.setDepthCompareFunction(MTLCompareFunction::Always);
    descriptor.setDepthWriteEnabled(false);
    device
        .newDepthStencilStateWithDescriptor(&descriptor)
        .context("creating depth-ignoring stencil state")
}

/// The bytes every vertex shader reads at `GlobalInputIndex`. QUILL: also
/// set again on the encoder a lens batch reopens.
fn set_frame_globals(
    command_encoder: &ProtocolObject<dyn MTLRenderCommandEncoder>,
    viewport_size: Size<DevicePixels>,
    depth_scale: f32,
) {
    // SAFETY: Metal copies both values, which match every vertex shader's
    // `GlobalInputIndex_ViewportSize` and `GlobalInputIndex_DepthScale` arguments.
    unsafe {
        command_encoder.setVertexBytes_length_atIndex(
            ptr::NonNull::from(&viewport_size).cast(),
            mem::size_of_val(&viewport_size),
            GlobalInputIndex::ViewportSize as usize,
        );
        command_encoder.setVertexBytes_length_atIndex(
            ptr::NonNull::from(&depth_scale).cast(),
            mem::size_of_val(&depth_scale),
            GlobalInputIndex::DepthScale as usize,
        );
    }
}

#[derive(Clone)]
struct InstanceBinding {
    buffer: Retained<ProtocolObject<dyn MTLBuffer>>,
    offset: usize,
}

struct InstanceBindings {
    quads: InstanceBinding,
    path_tiles: InstanceBinding,
    path_curves: InstanceBinding,
    path_paints: InstanceBinding,
    shadows: InstanceBinding,
    underlines: InstanceBinding,
    monochrome_sprites: InstanceBinding,
    polychrome_sprites: InstanceBinding,
    surfaces: InstanceBinding,
    native_views: InstanceBinding,
    /// QUILL: each lens's fallback quad, for batches that draw flat.
    lens_fallbacks: InstanceBinding,
    /// QUILL (glass stage 5): where each inked label reads its glass, and the
    /// atlas it reads (`None` when the frame's glass draws flat).
    lens_inks: InstanceBinding,
    lens_atlas: Option<Retained<ProtocolObject<dyn MTLTexture>>>,
    /// QUILL: each snapshot quad as a polychrome sprite.
    snapshot_sprites: InstanceBinding,
}

fn write_instances(
    scene: &Scene,
    writer: &mut InstanceBufferWriter,
    glass: Option<&lens::GlassFrame>,
) -> Result<InstanceBindings> {
    let mut path_paints = Vec::with_capacity(scene.paths.len());
    let mut path_curves = Vec::new();
    let mut path_tiles = Vec::new();
    for path in &scene.paths {
        path.append_to(&mut path_paints, &mut path_curves, &mut path_tiles);
    }
    let inks = gpui::lens_ink_instances(scene, glass.map(|glass| &glass.plan));
    Ok(InstanceBindings {
        lens_inks: writer.write(&inks)?,
        lens_atlas: glass.map(|glass| glass.atlas().clone()),
        quads: writer.write(&scene.quads)?,
        path_tiles: writer.write(&path_tiles)?,
        path_curves: writer.write(&path_curves)?,
        path_paints: writer.write(&path_paints)?,
        shadows: writer.write(&scene.shadows)?,
        underlines: writer.write(&scene.underlines)?,
        monochrome_sprites: writer.write(&scene.monochrome_sprites)?,
        polychrome_sprites: writer.write(&scene.polychrome_sprites)?,
        surfaces: writer.write_iter(scene.surfaces.iter().map(|surface| SurfaceBounds {
            bounds: surface.bounds,
            content_mask: surface.content_mask,
        }))?,
        native_views: writer.write_iter(scene.native_views.iter().map(|native_view| {
            SurfaceBounds {
                bounds: native_view.bounds,
                content_mask: native_view.content_mask,
            }
        }))?,
        lens_fallbacks: writer.write_iter(scene.lenses.iter().map(BackdropLens::fallback_quad))?,
        snapshot_sprites: writer.write_iter(scene.snapshots.iter().map(SnapshotQuad::sprite))?,
    })
}

struct InstanceBufferWriter {
    device: Retained<ProtocolObject<dyn MTLDevice>>,
    pool: Arc<Mutex<InstanceBufferPool>>,
    unified_memory: bool,
    filled: Vec<(InstanceBuffer, usize)>,
    current: InstanceBuffer,
    offset: usize,
}

impl InstanceBufferWriter {
    fn new(
        device: &ProtocolObject<dyn MTLDevice>,
        pool: &Arc<Mutex<InstanceBufferPool>>,
        unified_memory: bool,
    ) -> Result<Self> {
        let current = pool.lock().acquire(device, unified_memory)?;
        Ok(Self {
            device: device.retain(),
            pool: pool.clone(),
            unified_memory,
            filled: Vec::new(),
            current,
            offset: 0,
        })
    }

    fn allocate<T>(&mut self, count: usize) -> Result<(InstanceBinding, &mut [MaybeUninit<T>])> {
        let size = mem::size_of::<T>() * count;
        let mut offset = self.offset.next_multiple_of(INSTANCE_BUFFER_ALIGNMENT);
        if offset + size > self.current.size {
            self.grow(size)?;
            offset = 0;
        }
        self.offset = offset + size;

        let binding = InstanceBinding {
            buffer: self.current.metal_buffer.clone(),
            offset,
        };
        // SAFETY: The aligned reservation lies within this frame's exclusively owned buffer and does not overlap prior writes.
        let values = unsafe {
            let start = self
                .current
                .metal_buffer
                .contents()
                .as_ptr()
                .cast::<u8>()
                .add(offset);
            slice::from_raw_parts_mut(start.cast::<MaybeUninit<T>>(), count)
        };
        Ok((binding, values))
    }

    fn write<T>(&mut self, values: &[T]) -> Result<InstanceBinding> {
        let (binding, destination) = self.allocate::<T>(values.len())?;
        // SAFETY: The disjoint source and destination hold the same number of T values, and the destination stays MaybeUninit.
        unsafe {
            ptr::copy_nonoverlapping(
                values.as_ptr(),
                destination.as_mut_ptr().cast::<T>(),
                values.len(),
            );
        }
        Ok(binding)
    }

    fn write_iter<T>(
        &mut self,
        values: impl ExactSizeIterator<Item = T>,
    ) -> Result<InstanceBinding> {
        let (binding, destination) = self.allocate::<T>(values.len())?;
        for (slot, value) in destination.iter_mut().zip(values) {
            slot.write(value);
        }
        Ok(binding)
    }

    fn grow(&mut self, required: usize) -> Result<()> {
        let mut pool = self.pool.lock();
        let buffer_size = (pool.buffer_size * 2)
            .max(required.next_power_of_two())
            .min(MAX_INSTANCE_BUFFER_SIZE);
        anyhow::ensure!(
            buffer_size >= required,
            "instance buffer needs {required} bytes, above the maximum of {MAX_INSTANCE_BUFFER_SIZE}"
        );
        anyhow::ensure!(
            buffer_size > self.current.size,
            "frame instance data exceeds the {MAX_INSTANCE_BUFFER_SIZE}-byte maximum"
        );
        if buffer_size != pool.buffer_size {
            log::info!("increased instance buffer size to {buffer_size}");
            pool.reset(buffer_size);
        }
        let buffer = pool.acquire(&self.device, self.unified_memory)?;
        drop(pool);

        let filled = mem::replace(&mut self.current, buffer);
        self.filled.push((filled, self.offset));
        self.offset = 0;
        Ok(())
    }

    fn finish(self) -> InstanceBuffer {
        let Self {
            unified_memory,
            filled,
            current,
            offset,
            ..
        } = self;

        if !unified_memory {
            for (buffer, written) in &filled {
                if *written == 0 {
                    continue;
                }
                buffer.metal_buffer.didModifyRange(NSRange {
                    location: 0,
                    length: *written,
                });
            }
            if offset > 0 {
                current.metal_buffer.didModifyRange(NSRange {
                    location: 0,
                    length: offset,
                });
            }
        }

        // Metal retains encoded resources until the command buffer completes.
        // Only the final, largest buffer is worth keeping in the pool.
        drop(filled);
        current
    }
}

/// Vertex buffer slots bound once per render pass and shared by every
/// pipeline. Each primitive's own slots start after these.
#[repr(C)]
enum GlobalInputIndex {
    ViewportSize = 0,
    DepthScale = 1,
}

#[repr(C)]
enum ShadowInputIndex {
    Shadows = 2,
}

#[repr(C)]
enum QuadInputIndex {
    Quads = 2,
    QuadCount = 3,
}

#[repr(C)]
enum UnderlineInputIndex {
    Underlines = 2,
}

#[repr(C)]
enum SpriteInputIndex {
    Sprites = 2,
    AtlasTextureSize = 3,
    AtlasTexture = 4,
    /// QUILL (glass stage 5): inked labels' records and the lens atlas.
    Inks = 5,
    LensAtlas = 6,
}

#[repr(C)]
enum SurfaceInputIndex {
    Surfaces = 2,
    TextureSize = 3,
    YTexture = 4,
    CbCrTexture = 5,
}

#[repr(C)]
enum PathTileInputIndex {
    Tiles = 2,
    Curves = 3,
    Paints = 4,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct SurfaceBounds {
    pub bounds: Bounds<ScaledPixels>,
    pub content_mask: ContentMask<ScaledPixels>,
}

#[cfg(any(feature = "bench-support", feature = "test-support"))]
pub struct MetalHeadlessRenderer {
    renderer: MetalRenderer,
}

#[cfg(any(feature = "bench-support", feature = "test-support"))]
impl MetalHeadlessRenderer {
    pub fn new() -> Result<Self> {
        let instance_buffer_pool = Arc::new(Mutex::new(InstanceBufferPool::default()));
        let renderer = MetalRenderer::new_headless(instance_buffer_pool)?;
        Ok(Self { renderer })
    }
}

#[cfg(any(feature = "bench-support", feature = "test-support"))]
impl gpui::PlatformHeadlessRenderer for MetalHeadlessRenderer {
    fn render_scene_to_image(
        &mut self,
        scene: &Scene,
        size: Size<DevicePixels>,
    ) -> anyhow::Result<image::RgbaImage> {
        self.renderer.render_scene_to_image(scene, size)
    }

    fn render_scene(&mut self, scene: &Scene, size: Size<DevicePixels>) -> anyhow::Result<()> {
        self.renderer.render_scene(scene, size)
    }

    fn sprite_atlas(&self) -> Arc<dyn gpui::PlatformAtlas> {
        self.renderer.sprite_atlas().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Corners, Hsla, Point, Quad, point};
    use std::cell::Cell;
    use objc2_core_foundation::{CFDictionary, CFType};
    use objc2_core_video::{
        CVPixelBufferCreate, CVPixelBufferGetBaseAddressOfPlane,
        CVPixelBufferGetBytesPerRowOfPlane, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
        CVPixelBufferUnlockBaseAddress, kCVPixelBufferIOSurfacePropertiesKey,
    };
    use objc2_metal::MTLCommandBufferStatus;

    #[test]
    fn render_to_image_rejects_zero_sized_layer() -> Result<()> {
        let mut renderer = MetalRenderer::new(Default::default(), false, false)?;
        for (width, height) in [(0, 0), (0, 64), (64, 0)] {
            renderer.update_drawable_size(size(DevicePixels(width), DevicePixels(height)));
            assert!(renderer.render_to_image(&Scene::default()).is_err());
        }
        Ok(())
    }

    #[test]
    fn display_link_mode_consumes_and_clears_drawables() -> Result<()> {
        let mut renderer = MetalRenderer::new(Default::default(), false, false)?;
        renderer.update_drawable_size(size(DevicePixels(64), DevicePixels(64)));
        let drawable = renderer
            .layer()
            .and_then(|layer| layer.nextDrawable())
            .context("window renderer must supply a drawable")?;

        renderer.set_display_link_attached(true);
        assert!(matches!(
            renderer.display_link_mode,
            DisplayLinkMode::Attached { drawable: None }
        ));
        assert!(
            renderer
                .set_frame_drawable(Some(drawable.clone()))
                .is_none()
        );
        assert!(matches!(
            renderer.display_link_mode,
            DisplayLinkMode::Attached { drawable: Some(_) }
        ));
        renderer.draw(&Scene::default());
        assert!(matches!(
            renderer.display_link_mode,
            DisplayLinkMode::Attached { drawable: None }
        ));
        // A frame that drew consumed its drawable; one that didn't hands it back.
        assert!(
            renderer
                .set_frame_drawable(Some(drawable.clone()))
                .is_none()
        );
        assert!(renderer.set_frame_drawable(None).is_some());

        renderer.set_frame_drawable(Some(drawable));
        renderer.set_display_link_attached(false);
        assert!(matches!(
            renderer.display_link_mode,
            DisplayLinkMode::Detached
        ));
        Ok(())
    }

    fn renderer_from(
        library: impl Fn(
            &ProtocolObject<dyn MTLDevice>,
        ) -> Result<Retained<ProtocolObject<dyn MTLLibrary>>>,
    ) -> Result<MetalRenderer> {
        MetalRenderer::with_shaders(
            MetalRenderer::create_device()?,
            None,
            true,
            Arc::new(Mutex::new(InstanceBufferPool::default())),
            library,
        )
    }

    #[test]
    fn shaders_that_do_not_compile_are_an_error() {
        let tries = Cell::new(0);
        let result = renderer_from(|device| {
            tries.set(tries.get() + 1);
            library_from_source(device, "this is not a shader")
        });
        assert!(result.is_err());
        assert_eq!(tries.get(), 2);
    }

    #[test]
    fn a_library_without_our_functions_is_an_error() {
        let error = renderer_from(|device| library_from_source(device, "#include <metal_stdlib>"))
            .err()
            .expect("the renderer needs its shader functions");
        assert!(format!("{error:#}").contains("locating vertex function"));
    }

    #[cfg(quill_metallib)]
    #[test]
    fn the_precompiled_shaders_build_every_pipeline() {
        renderer_from(precompiled_library).unwrap();
    }

    #[test]
    fn one_failed_library_build_is_retried() {
        let tries = Cell::new(0);
        let result = renderer_from(|device| {
            tries.set(tries.get() + 1);
            if tries.get() == 1 {
                library_from_source(device, "this is not a shader")
            } else {
                built_in_library(device)
            }
        });
        assert!(result.is_ok());
        assert_eq!(tries.get(), 2);
    }

    fn library_without_functions(
        device: &ProtocolObject<dyn MTLDevice>,
    ) -> Retained<ProtocolObject<dyn MTLLibrary>> {
        library_from_source(device, "#include <metal_stdlib>").unwrap()
    }

    fn renderer_on_a_float_layer() -> MetalRenderer {
        let renderer = MetalRenderer::new(Default::default(), false, false).unwrap();
        let layer = renderer.layer().unwrap();
        layer.setDrawableSize(CGSize::new(16.0, 16.0));
        layer.setFramebufferOnly(false);
        layer.setPixelFormat(MTLPixelFormat::RGBA16Float);
        layer.setWantsExtendedDynamicRangeContent(true);
        renderer
    }

    fn empty_scene() -> Scene {
        let mut scene = Scene::default();
        scene.finish();
        scene
    }

    #[test]
    fn a_failed_edr_switch_leaves_edr_and_still_draws() {
        let mut renderer = renderer_on_a_float_layer();
        renderer.library = library_without_functions(&renderer.device);

        renderer.draw(&empty_scene());

        let layer = renderer.layer().unwrap();
        assert_eq!(layer.pixelFormat(), MTLPixelFormat::BGRA8Unorm);
        assert!(!layer.wantsExtendedDynamicRangeContent());
    }

    #[test]
    fn a_failed_edr_switch_under_the_display_link_leaves_edr_and_still_draws() {
        let mut renderer = renderer_on_a_float_layer();
        renderer.library = library_without_functions(&renderer.device);
        let drawable = renderer.layer().unwrap().nextDrawable().unwrap();
        renderer.set_display_link_attached(true);
        renderer.set_frame_drawable(Some(drawable));

        renderer.draw(&empty_scene());

        let layer = renderer.layer().unwrap();
        assert_eq!(layer.pixelFormat(), MTLPixelFormat::BGRA8Unorm);
        assert!(!layer.wantsExtendedDynamicRangeContent());
        assert!(matches!(
            renderer.display_link_mode,
            DisplayLinkMode::Attached { drawable: None }
        ));
    }

    #[test]
    fn a_format_that_failed_is_not_built_again() {
        let mut renderer = renderer_on_a_float_layer();
        let library = mem::replace(
            &mut renderer.library,
            library_without_functions(&renderer.device),
        );
        renderer.draw(&empty_scene());
        assert_eq!(
            renderer.layer().unwrap().pixelFormat(),
            MTLPixelFormat::BGRA8Unorm
        );

        // A rebuild would work now, and would keep the layer on float.
        renderer.library = library;
        let layer = renderer.layer().unwrap();
        layer.setPixelFormat(MTLPixelFormat::RGBA16Float);
        renderer.draw(&empty_scene());

        assert_eq!(
            renderer.layer().unwrap().pixelFormat(),
            MTLPixelFormat::BGRA8Unorm
        );
    }

    #[test]
    fn a_failed_switch_keeps_the_stashed_pipelines() {
        let mut renderer = MetalRenderer::new_headless(Default::default()).unwrap();
        renderer
            .match_drawable_format(MTLPixelFormat::RGBA16Float)
            .unwrap();
        renderer
            .match_drawable_format(MTLPixelFormat::BGRA8Unorm)
            .unwrap();
        renderer.library = library_without_functions(&renderer.device);

        assert!(
            renderer
                .match_drawable_format(MTLPixelFormat::RGBA8Unorm)
                .is_err()
        );

        renderer
            .match_drawable_format(MTLPixelFormat::RGBA16Float)
            .unwrap();
    }

    #[test]
    fn render_core_video_surface() {
        let surface_properties_key = unsafe { kCVPixelBufferIOSurfacePropertiesKey };
        let attributes = CFDictionary::from_slices(
            &[surface_properties_key],
            &[&*CFDictionary::<CFType, CFType>::empty()],
        );
        let mut image_buffer = ptr::null_mut();
        assert_eq!(
            unsafe {
                CVPixelBufferCreate(
                    None,
                    16,
                    16,
                    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
                    Some(attributes.as_opaque()),
                    ptr::NonNull::from(&mut image_buffer),
                )
            },
            kCVReturnSuccess
        );
        let image_buffer = ptr::NonNull::new(image_buffer).unwrap();
        let image_buffer = unsafe { CFRetained::from_raw(image_buffer) };
        unsafe {
            assert_eq!(
                CVPixelBufferLockBaseAddress(&image_buffer, CVPixelBufferLockFlags::empty()),
                kCVReturnSuccess
            );
            for plane in 0..2 {
                let bytes = CVPixelBufferGetBaseAddressOfPlane(&image_buffer, plane).cast::<u8>();
                let length = CVPixelBufferGetBytesPerRowOfPlane(&image_buffer, plane)
                    * CVPixelBufferGetHeightOfPlane(&image_buffer, plane);
                slice::from_raw_parts_mut(bytes, length).fill(128);
            }
            assert_eq!(
                CVPixelBufferUnlockBaseAddress(&image_buffer, CVPixelBufferLockFlags::empty()),
                kCVReturnSuccess
            );
        }

        let bounds = Bounds {
            origin: Point::default(),
            size: size(ScaledPixels(16.0), ScaledPixels(16.0)),
        };
        let mut scene = Scene::default();
        scene.insert_primitive(PaintSurface {
            order: 0,
            bounds,
            content_mask: ContentMask::rect(bounds),
            image_buffer: image_buffer.clone(),
        });
        drop(image_buffer);
        scene.finish();

        let mut renderer =
            MetalRenderer::new_headless(Arc::new(Mutex::new(InstanceBufferPool::default())))
                .unwrap();
        let image = renderer
            .render_scene_to_image(&scene, size(DevicePixels(16), DevicePixels(16)))
            .unwrap();
        for image::Rgba([red, green, blue, alpha]) in image.pixels() {
            assert!(red.abs_diff(128) <= 2);
            assert!(green.abs_diff(128) <= 2);
            assert!(blue.abs_diff(128) <= 2);
            assert_eq!(*alpha, 255);
        }

        let descriptor = MTLTextureDescriptor::new();
        unsafe {
            descriptor.setWidth(16);
            descriptor.setHeight(16);
        }
        descriptor.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        descriptor.setUsage(MTLTextureUsage::RenderTarget);
        let uses_shared_storage = cfg!(target_os = "ios") || renderer.is_apple_gpu;
        descriptor.setStorageMode(if uses_shared_storage {
            MTLStorageMode::Shared
        } else {
            MTLStorageMode::Managed
        });
        let target_texture = renderer
            .device
            .newTextureWithDescriptor(&descriptor)
            .unwrap();
        let command_buffer = renderer
            .render_frame(
                &scene,
                &target_texture,
                size(DevicePixels(16), DevicePixels(16)),
            )
            .unwrap();
        if !uses_shared_storage {
            let blit = command_buffer.blitCommandEncoder().unwrap();
            blit.synchronizeResource(ProtocolObject::from_ref(&*target_texture));
            blit.endEncoding();
        }
        drop(scene);
        drop(renderer);
        command_buffer.commit();
        command_buffer.waitUntilCompleted();
        assert_eq!(command_buffer.status(), MTLCommandBufferStatus::Completed);
        assert_eq!(read_texture_to_image(&target_texture).unwrap(), image);
    }

    /// QUILL (docs/decisions/rounded-content-masks.md): the coverage the
    /// shaders give a rounded mask at a pixel centre, computed on the CPU.
    fn rounded_coverage(x: f32, y: f32, side: f32, radius: f32) -> f32 {
        let half = side / 2.0;
        let (cx, cy) = (x - half, y - half);
        let corner = ((cx.abs() - half) + radius, (cy.abs() - half) + radius);
        let outside = (corner.0.max(0.0).powi(2) + corner.1.max(0.0).powi(2)).sqrt();
        let inside = corner.0.max(corner.1).min(0.0);
        (0.5 - (outside + inside - radius)).clamp(0.0, 1.0)
    }

    /// QUILL: an opaque white quad filling a 64 x 64 target (cleared to
    /// black) under a mask rounded to 24 is cut at its corners with the
    /// coverage the CPU computes, within one grey level; a quad under the same mask but away
    /// from its corners (culled to the plain path) is whole.
    #[test]
    fn a_rounded_mask_cuts_an_opaque_quad_at_its_corners() {
        let side = 64.0;
        let target = Bounds {
            origin: Point::default(),
            size: size(ScaledPixels(side), ScaledPixels(side)),
        };
        let mask = ContentMask::rounded(target, Corners::all(ScaledPixels(24.0)));
        let white = Hsla {
            h: 0.0,
            s: 0.0,
            l: 1.0,
            a: 1.0,
        };
        let mut scene = Scene::default();
        scene.insert_primitive(Quad {
            bounds: target,
            content_mask: mask,
            background: white.into(),
            ..Quad::default()
        });
        scene.finish();
        let mut renderer =
            MetalRenderer::new_headless(Arc::new(Mutex::new(InstanceBufferPool::default())))
                .unwrap();
        let image = renderer
            .render_scene_to_image(&scene, size(DevicePixels(64), DevicePixels(64)))
            .unwrap();
        let mut worst = (0u8, 0, 0);
        for (x, y, image::Rgba([red, _, _, _])) in image.enumerate_pixels() {
            let expected = rounded_coverage(x as f32 + 0.5, y as f32 + 0.5, side, 24.0);
            let expected = (expected * 255.0).round() as u8;
            worst = worst.max((red.abs_diff(expected), x, y));
        }
        assert!(
            worst.0 <= 1,
            "worst difference {worst:?}: {:?}",
            image.get_pixel(worst.1, worst.2)
        );
        let corner = image.get_pixel(0, 0);
        assert_eq!(corner.0[0], 0, "the corner is cut away");

        // The same mask over a band in the middle: culled, drawn whole.
        let band = Bounds {
            origin: point(ScaledPixels(0.0), ScaledPixels(28.0)),
            size: size(ScaledPixels(side), ScaledPixels(8.0)),
        };
        let mut scene = Scene::default();
        scene.insert_primitive(Quad {
            bounds: band,
            content_mask: mask,
            background: white.into(),
            ..Quad::default()
        });
        scene.finish();
        assert!(scene.quads.iter().all(|quad| quad.content_mask.rounded.is_none()));
        let image = renderer
            .render_scene_to_image(&scene, size(DevicePixels(64), DevicePixels(64)))
            .unwrap();
        assert_eq!(image.get_pixel(0, 30).0, [255, 255, 255, 255]);
        assert_eq!(image.get_pixel(0, 20).0[0], 0);
    }

    /// QUILL (docs/decisions/snapshot-transitions.md): the hand-back
    /// invariant. Content drawn into a snapshot, and the snapshot drawn at
    /// identity over the frame, gives the pixels the content gives drawn
    /// live: exactly where it is opaque, within one level where it blends.
    #[test]
    fn a_snapshot_at_identity_draws_the_pixels_of_its_content() {
        let px = |x: f32, y: f32, w: f32, h: f32| Bounds {
            origin: point(ScaledPixels(x), ScaledPixels(y)),
            size: size(ScaledPixels(w), ScaledPixels(h)),
        };
        let hsla = |h: f32, s: f32, l: f32, a: f32| Hsla { h, s, l, a };
        let target = px(0.0, 0.0, 64.0, 64.0);
        let background = Quad {
            bounds: target,
            content_mask: ContentMask::rect(target),
            background: hsla(0.6, 0.5, 0.3, 1.0).into(),
            ..Quad::default()
        };
        // The content, its top left at `at`: an opaque rounded card and a
        // half-transparent bar over its edge.
        let content = |at: f32| {
            let card = px(at + 4.0, at + 4.0, 40.0, 40.0);
            let bar = px(at + 20.0, at + 24.0, 36.0, 12.0);
            [
                Quad {
                    bounds: card,
                    content_mask: ContentMask::rect(px(at, at, 56.0, 56.0)),
                    background: hsla(0.0, 0.0, 1.0, 1.0).into(),
                    corner_radii: Corners::all(ScaledPixels(12.0)),
                    ..Quad::default()
                },
                Quad {
                    bounds: bar,
                    content_mask: ContentMask::rect(px(at, at, 56.0, 56.0)),
                    background: hsla(0.0, 1.0, 0.5, 0.5).into(),
                    ..Quad::default()
                },
            ]
        };
        let mut live = Scene::default();
        live.insert_primitive(background);
        for quad in content(8.0) {
            live.insert_primitive(quad);
        }
        live.finish();

        let mut captured = Scene::default();
        for quad in content(0.0) {
            captured.insert_primitive(quad);
        }
        captured.finish();
        let texels = size(DevicePixels(56), DevicePixels(56));
        let mut through = Scene::default();
        through.insert_primitive(background);
        through.insert_primitive(gpui::SnapshotQuad {
            order: 0,
            bounds: px(8.0, 8.0, 56.0, 56.0),
            content_mask: ContentMask::rect(target),
            opacity: 1.0,
            slot: gpui::SnapshotSlot(1),
            size: texels,
        });
        through.finish();
        through.snapshot_scenes.push(gpui::SnapshotScene {
            slot: gpui::SnapshotSlot(1),
            size: texels,
            backing: None,
            scene: captured,
        });

        let mut renderer =
            MetalRenderer::new_headless(Arc::new(Mutex::new(InstanceBufferPool::default())))
                .unwrap();
        let frame = size(DevicePixels(64), DevicePixels(64));
        let expected = renderer.render_scene_to_image(&live, frame).unwrap();
        let actual = renderer.render_scene_to_image(&through, frame).unwrap();
        let mut worst = (0u8, 0, 0);
        for (x, y, pixel) in actual.enumerate_pixels() {
            let want = expected.get_pixel(x, y);
            for (got, want) in pixel.0.iter().zip(want.0) {
                worst = worst.max((got.abs_diff(want), x, y));
            }
        }
        assert!(
            worst.0 <= 1,
            "worst difference {worst:?}: {:?} against {:?}",
            actual.get_pixel(worst.1, worst.2),
            expected.get_pixel(worst.1, worst.2)
        );
        // The card's middle is opaque: exactly the live pixel, and white.
        assert_eq!(actual.get_pixel(20, 20), expected.get_pixel(20, 20));
        assert_eq!(actual.get_pixel(20, 20).0, [255, 255, 255, 255]);

        // Its texture released, the quad draws nothing over the background.
        let mut released = Scene::default();
        released.insert_primitive(background);
        released.insert_primitive(gpui::SnapshotQuad {
            order: 0,
            bounds: px(8.0, 8.0, 56.0, 56.0),
            content_mask: ContentMask::rect(target),
            opacity: 1.0,
            slot: gpui::SnapshotSlot(1),
            size: texels,
        });
        released.finish();
        released.snapshot_releases.push(gpui::SnapshotSlot(1));
        let image = renderer.render_scene_to_image(&released, frame).unwrap();
        assert_eq!(image.get_pixel(20, 20), image.get_pixel(2, 2));
    }
}

/// QUILL (glass effects): whether `QUILL_GPU_PROBE` asks for per-frame GPU
/// times. Read once.
fn gpu_probe_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("QUILL_GPU_PROBE").is_ok_and(|value| value == "1"))
}
