// Modified for gpui-pre (snapshot of zed@08cbb88): adapt device queues and sRGB detection to wgpu 30.0.1.
// Modified by the Quill project (2026) from gpui-pre-wgpu 0.3.9 (Apache-2.0):
// images in textures of their own; uploads without a copy; a spare texture for
// pictures larger than an atlas texture. See QUILL-CHANGES.md.
use anyhow::{Context as _, Result};
use etagere::{BucketedAtlasAllocator, size2};
use gpui::{
    AtlasBackend, AtlasKey, AtlasState, AtlasTextureId, AtlasTextureKind, AtlasTextureList,
    AtlasTile, Bounds, DevicePixels, PlatformAtlas, Point, Size,
};
use parking_lot::Mutex;
use std::{borrow::Cow, ops, sync::Arc};

use crate::WgpuContext;

fn device_size_to_etagere(size: Size<DevicePixels>) -> etagere::Size {
    size2(size.width.0, size.height.0)
}

fn etagere_point_to_device(point: etagere::Point) -> Point<DevicePixels> {
    Point {
        x: DevicePixels(point.x),
        y: DevicePixels(point.y),
    }
}

pub struct WgpuAtlas(Mutex<AtlasState<WgpuAtlasTextures>>);

/// QUILL: the side of a default atlas texture. A picture larger than this
/// gets a texture of exactly its size.
const DEFAULT_ATLAS_SIDE: i32 = 1024;

/// QUILL: freed oversized textures kept for the next picture of the same
/// size (one per playing clip: its next picture is uploaded while the one
/// on screen still holds the other texture).
const MAX_SPARES: usize = 2;

/// QUILL: a spare not taken within this many frames is freed (the clip
/// stopped or went away).
const SPARE_FRAMES: u64 = 8;

/// QUILL: a freed texture larger than [`DEFAULT_ATLAS_SIDE`], kept so the
/// next picture of its size doesn't create one.
struct SpareTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    format: wgpu::TextureFormat,
    /// [`WgpuAtlasTextures::frame`] when it was freed.
    freed: u64,
}

struct WgpuAtlasTextures {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    max_texture_size: u32,
    color_texture_format: wgpu::TextureFormat,
    storage: WgpuAtlasStorage,
    next_texture_generation: u64,
    spares: Vec<SpareTexture>,
    /// Counts frames ([`WgpuAtlas::before_frame`]).
    frame: u64,
    /// Textures created, for tests.
    #[cfg(test)]
    textures_created: u64,
}

pub struct WgpuTextureInfo {
    pub view: wgpu::TextureView,
    /// Distinguishes new textures that reuse an [`AtlasTextureId`].
    pub generation: u64,
}

impl WgpuAtlas {
    pub fn new(
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        color_texture_format: wgpu::TextureFormat,
    ) -> Self {
        let max_texture_size = device.limits().max_texture_dimension_2d;
        WgpuAtlas(Mutex::new(AtlasState::new(WgpuAtlasTextures {
            device,
            queue,
            max_texture_size,
            color_texture_format,
            storage: WgpuAtlasStorage::default(),
            next_texture_generation: 0,
            spares: Vec::new(),
            frame: 0,
            #[cfg(test)]
            textures_created: 0,
        })))
    }

    pub fn from_context(context: &WgpuContext) -> Self {
        Self::new(
            context.device.clone(),
            context.queue.clone(),
            context.color_texture_format(),
        )
    }

    /// A frame starts: spares not taken lately are freed.
    pub fn before_frame(&self) {
        let mut lock = self.0.lock();
        let textures = &mut lock.backend;
        let frame = textures.frame;
        textures
            .spares
            .retain(|spare| frame.saturating_sub(spare.freed) < SPARE_FRAMES);
        textures.frame = frame.wrapping_add(1);
    }

    /// Returns the view backing `id`, or `None` once every tile in it has been
    /// removed. A scene can still reference such a texture when a cached view
    /// replays a paint from before the image was dropped, so callers must skip
    /// those sprites rather than assume the texture exists.
    pub fn get_texture_info(&self, id: AtlasTextureId) -> Option<WgpuTextureInfo> {
        let lock = self.0.lock();
        let texture = lock.backend.storage.get(id)?;
        Some(WgpuTextureInfo {
            view: texture.view.clone(),
            generation: texture.generation,
        })
    }

    /// Clears all cached textures and tiles, forcing them to be recreated.
    /// Use this for incremental recovery when the device is still valid.
    pub fn clear(&self) {
        self.0.lock().clear(|textures| {
            textures.storage = WgpuAtlasStorage::default();
            textures.spares.clear();
        });
    }

    /// Handles device lost by clearing all textures and cached tiles.
    /// The atlas will lazily recreate textures as needed on subsequent frames.
    pub fn handle_device_lost(&self, context: &WgpuContext) {
        self.0.lock().clear(|textures| {
            textures.device = context.device.clone();
            textures.queue = context.queue.clone();
            textures.color_texture_format = context.color_texture_format();
            textures.storage = WgpuAtlasStorage::default();
            textures.spares.clear();
        });
    }
}

impl PlatformAtlas for WgpuAtlas {
    fn get_or_insert_with<'a>(
        &self,
        key: AtlasKey,
        build: &mut dyn FnMut() -> Result<Option<(Size<DevicePixels>, Cow<'a, [u8]>)>>,
    ) -> Result<Option<AtlasTile>> {
        self.0.lock().get_or_insert_with(key, build)
    }

    fn remove(&self, key: &AtlasKey) {
        self.0.lock().remove(key);
    }

    fn next_use_mark(&self) -> u64 {
        self.0.lock().next_use_mark()
    }

    fn emoji_glyph_count(&self) -> usize {
        self.0.lock().emoji_glyph_count()
    }

    fn remove_emoji_glyphs_unused_since(&self, mark: u64) -> usize {
        self.0.lock().remove_emoji_glyphs_unused_since(mark)
    }

    fn text_glyph_count(&self) -> usize {
        self.0.lock().text_glyph_count()
    }

    fn remove_text_glyphs_unused_since(&self, mark: u64) -> usize {
        self.0.lock().remove_text_glyphs_unused_since(mark)
    }
}

impl AtlasBackend for WgpuAtlasTextures {
    fn insert(
        &mut self,
        kind: AtlasTextureKind,
        size: Size<DevicePixels>,
        bytes: &[u8],
    ) -> Result<AtlasTile> {
        self.insert_tile(kind, size, bytes, false)
    }

    // QUILL: images in textures of their own.
    fn insert_image(
        &mut self,
        kind: AtlasTextureKind,
        size: Size<DevicePixels>,
        bytes: &[u8],
    ) -> Result<AtlasTile> {
        self.insert_tile(kind, size, bytes, true)
    }

    fn remove(&mut self, tile: AtlasTile) {
        let id = tile.texture_id;
        let Some(texture_slot) = self.storage[id.kind].textures.get_mut(id.index as usize) else {
            return;
        };

        if let Some(mut texture) = texture_slot.take() {
            texture.allocator.deallocate(tile.tile_id.into());
            texture.decrement_ref_count();
            if texture.is_unreferenced() {
                self.storage[id.kind]
                    .free_list
                    .push(texture.id.index as usize);
                self.keep_spare(texture);
            } else {
                *texture_slot = Some(texture);
            }
        }
    }
}

impl WgpuAtlasTextures {
    fn insert_tile(
        &mut self,
        kind: AtlasTextureKind,
        size: Size<DevicePixels>,
        bytes: &[u8],
        images: bool,
    ) -> Result<AtlasTile> {
        let tile = self
            .allocate(size, kind, TileClass::of(images, size))
            .context("failed to allocate")?;
        self.upload_texture(tile.texture_id, tile.bounds, bytes);
        Ok(tile)
    }

    fn allocate(
        &mut self,
        size: Size<DevicePixels>,
        texture_kind: AtlasTextureKind,
        class: TileClass,
    ) -> Option<AtlasTile> {
        {
            let textures = &mut self.storage[texture_kind];

            // QUILL: only textures holding the same class of tile.
            if let Some(tile) = textures
                .iter_mut()
                .rev()
                .filter(|texture| texture.class == class)
                .find_map(|texture| texture.allocate(size))
            {
                return Some(tile);
            }
        }

        let texture = self.push_texture(size, texture_kind, class);
        texture.allocate(size)
    }

    fn push_texture(
        &mut self,
        min_size: Size<DevicePixels>,
        kind: AtlasTextureKind,
        class: TileClass,
    ) -> &mut WgpuAtlasTexture {
        const DEFAULT_ATLAS_SIZE: Size<DevicePixels> = Size {
            width: DevicePixels(DEFAULT_ATLAS_SIDE),
            height: DevicePixels(DEFAULT_ATLAS_SIDE),
        };
        let max_texture_size = self.max_texture_size as i32;
        let max_atlas_size = Size {
            width: DevicePixels(max_texture_size),
            height: DevicePixels(max_texture_size),
        };

        // QUILL: small images get 512² textures (`TileClass`).
        const SMALL_IMAGE_ATLAS_SIZE: Size<DevicePixels> = Size {
            width: DevicePixels(512),
            height: DevicePixels(512),
        };
        let floor = if class == TileClass::SmallImages {
            SMALL_IMAGE_ATLAS_SIZE
        } else {
            DEFAULT_ATLAS_SIZE
        };
        let size = min_size.min(&max_atlas_size).max(&floor);
        let format = match kind {
            AtlasTextureKind::Monochrome => wgpu::TextureFormat::R8Unorm,
            AtlasTextureKind::Subpixel | AtlasTextureKind::Polychrome => self.color_texture_format,
        };

        let (width, height) = (size.width.0 as u32, size.height.0 as u32);
        let spare = self.spares.iter().position(|spare| {
            spare.format == format
                && spare.texture.width() == width
                && spare.texture.height() == height
        });
        let (texture, view) = match spare {
            Some(index) => {
                let spare = self.spares.swap_remove(index);
                (spare.texture, spare.view)
            }
            None => {
                #[cfg(test)]
                {
                    self.textures_created += 1;
                }
                let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("atlas"),
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
                });
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                (texture, view)
            }
        };

        let texture_list = &mut self.storage[kind];
        let index = texture_list.free_list.pop();
        let generation = self.next_texture_generation;
        self.next_texture_generation = self.next_texture_generation.wrapping_add(1);

        let atlas_texture = WgpuAtlasTexture {
            id: AtlasTextureId {
                index: index.unwrap_or(texture_list.textures.len()) as u32,
                kind,
            },
            generation,
            allocator: BucketedAtlasAllocator::new(device_size_to_etagere(size)),
            format,
            texture,
            view,
            live_atlas_keys: 0,
            class,
        };

        if let Some(ix) = index {
            texture_list.textures[ix] = Some(atlas_texture);
            texture_list
                .textures
                .get_mut(ix)
                .and_then(|t| t.as_mut())
                .expect("texture must exist")
        } else {
            texture_list.textures.push(Some(atlas_texture));
            texture_list
                .textures
                .last_mut()
                .and_then(|t| t.as_mut())
                .expect("texture must exist")
        }
    }

    /// QUILL: the bytes go to the queue at once (it stages them for the
    /// next submit), so they are not copied into a pending list first.
    fn upload_texture(&mut self, id: AtlasTextureId, bounds: Bounds<DevicePixels>, bytes: &[u8]) {
        let Some(texture) = self.storage.get(id) else {
            return;
        };
        let data = swizzle_upload_data(bytes, texture.format);
        let bytes_per_pixel = texture.bytes_per_pixel();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: bounds.origin.x.0 as u32,
                    y: bounds.origin.y.0 as u32,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bounds.size.width.0 as u32 * bytes_per_pixel as u32),
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: bounds.size.width.0 as u32,
                height: bounds.size.height.0 as u32,
                depth_or_array_layers: 1,
            },
        );
    }

    /// QUILL: keep a freed texture made for one oversized picture as a
    /// spare; the oldest spare goes when there are too many.
    fn keep_spare(&mut self, texture: WgpuAtlasTexture) {
        let side = DEFAULT_ATLAS_SIDE as u32;
        if texture.texture.width() <= side && texture.texture.height() <= side {
            return;
        }
        if self.spares.len() >= MAX_SPARES {
            if let Some(oldest) = self
                .spares
                .iter()
                .enumerate()
                .min_by_key(|(_, spare)| spare.freed)
                .map(|(index, _)| index)
            {
                self.spares.swap_remove(oldest);
            }
        }
        self.spares.push(SpareTexture {
            texture: texture.texture,
            view: texture.view,
            format: texture.format,
            freed: self.frame,
        });
    }
}

#[derive(Default)]
struct WgpuAtlasStorage {
    monochrome_textures: AtlasTextureList<WgpuAtlasTexture>,
    subpixel_textures: AtlasTextureList<WgpuAtlasTexture>,
    polychrome_textures: AtlasTextureList<WgpuAtlasTexture>,
}

impl ops::Index<AtlasTextureKind> for WgpuAtlasStorage {
    type Output = AtlasTextureList<WgpuAtlasTexture>;
    fn index(&self, kind: AtlasTextureKind) -> &Self::Output {
        match kind {
            AtlasTextureKind::Monochrome => &self.monochrome_textures,
            AtlasTextureKind::Subpixel => &self.subpixel_textures,
            AtlasTextureKind::Polychrome => &self.polychrome_textures,
        }
    }
}

impl ops::IndexMut<AtlasTextureKind> for WgpuAtlasStorage {
    fn index_mut(&mut self, kind: AtlasTextureKind) -> &mut Self::Output {
        match kind {
            AtlasTextureKind::Monochrome => &mut self.monochrome_textures,
            AtlasTextureKind::Subpixel => &mut self.subpixel_textures,
            AtlasTextureKind::Polychrome => &mut self.polychrome_textures,
        }
    }
}

impl WgpuAtlasStorage {
    fn get(&self, id: AtlasTextureId) -> Option<&WgpuAtlasTexture> {
        self[id.kind]
            .textures
            .get(id.index as usize)
            .and_then(|t| t.as_ref())
    }
}

struct WgpuAtlasTexture {
    id: AtlasTextureId,
    generation: u64,
    allocator: BucketedAtlasAllocator,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    format: wgpu::TextureFormat,
    live_atlas_keys: u32,
    /// QUILL: the class of every tile it holds.
    class: TileClass,
}

impl WgpuAtlasTexture {
    fn allocate(&mut self, size: Size<DevicePixels>) -> Option<AtlasTile> {
        let allocation = self.allocator.allocate(device_size_to_etagere(size))?;
        let tile = AtlasTile {
            texture_id: self.id,
            tile_id: allocation.id.into(),
            padding: 0,
            bounds: Bounds {
                origin: etagere_point_to_device(allocation.rectangle.min),
                size,
            },
        };
        self.live_atlas_keys += 1;
        Some(tile)
    }

    fn bytes_per_pixel(&self) -> u8 {
        match self.format {
            wgpu::TextureFormat::R8Unorm => 1,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm => 4,
            _ => 4,
        }
    }

    fn decrement_ref_count(&mut self) {
        self.live_atlas_keys -= 1;
    }

    fn is_unreferenced(&self) -> bool {
        self.live_atlas_keys == 0
    }
}

fn swizzle_upload_data(bytes: &[u8], format: wgpu::TextureFormat) -> Cow<'_, [u8]> {
    match format {
        wgpu::TextureFormat::Rgba8Unorm => {
            let mut data = bytes.to_vec();
            for pixel in data.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            Cow::Owned(data)
        }
        _ => Cow::Borrowed(bytes),
    }
}

/// QUILL: which tiles share a texture. Images never share one with glyphs
/// and SVGs (a long-lived glyph kept a photo's texture alive). Small images
/// (avatars, custom emoji: both sides at most 128 px) get 512² (1 MB)
/// textures of their own, so the few left on screen after browsing keep
/// 1 MB textures instead of 4 MB ones larger pictures came and went in.
/// Larger images keep upstream's 1024² textures: animation frames of a few
/// hundred pixels rotate through one, where a smaller texture would hold
/// one frame and be made anew for each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TileClass {
    Glyphs,
    SmallImages,
    Images,
}

impl TileClass {
    fn of(images: bool, size: Size<DevicePixels>) -> Self {
        if !images {
            Self::Glyphs
        } else if size.width.0 <= 128 && size.height.0 <= 128 {
            Self::SmallImages
        } else {
            Self::Images
        }
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use gpui::block_on;
    use gpui::{ImageId, RenderImageParams};
    use std::sync::Arc;

    fn test_device_and_queue() -> anyhow::Result<(Arc<wgpu::Device>, Arc<wgpu::Queue>)> {
        block_on(async {
            let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
                backends: wgpu::Backends::all(),
                flags: wgpu::InstanceFlags::default(),
                backend_options: wgpu::BackendOptions::default(),
                memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
                display: None,
            });
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::LowPower,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                    apply_limit_buckets: false,
                })
                .await
                .map_err(|error| anyhow::anyhow!("failed to request adapter: {error}"))?;
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: Some("wgpu_atlas_test_device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_defaults()
                        .using_resolution(adapter.limits())
                        .using_alignment(adapter.limits()),
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                    trace: wgpu::Trace::Off,
                    experimental_features: wgpu::ExperimentalFeatures::disabled(),
                })
                .await
                .map_err(|error| anyhow::anyhow!("failed to request device: {error}"))?;
            Ok((Arc::new(device), Arc::new(queue)))
        })
    }

    #[test]
    fn a_tile_removed_before_the_frame_is_safe() -> anyhow::Result<()> {
        let (device, queue) = test_device_and_queue()?;

        let atlas = WgpuAtlas::new(device, queue, wgpu::TextureFormat::Bgra8Unorm);
        let key = AtlasKey::Image(RenderImageParams {
            image_id: ImageId(1),
            frame_index: 0,
        });
        let size = Size {
            width: DevicePixels(1),
            height: DevicePixels(1),
        };
        let mut build = || Ok(Some((size, Cow::Owned(vec![0, 0, 0, 255]))));

        // Upstream regression test: this panicked while uploads waited for
        // the frame.
        atlas
            .get_or_insert_with(key.clone(), &mut build)?
            .expect("tile should be created");
        atlas.remove(&key);
        atlas.before_frame();
        Ok(())
    }

    #[test]
    fn remove_deallocates_tile_space_for_reuse() -> anyhow::Result<()> {
        let (device, queue) = test_device_and_queue()?;
        let atlas = WgpuAtlas::new(device, queue, wgpu::TextureFormat::Bgra8Unorm);

        // QUILL: larger than a small image (`TileClass`), so it shares the
        // big tiles' texture.
        let small = Size {
            width: DevicePixels(200),
            height: DevicePixels(200),
        };
        let big = Size {
            width: DevicePixels(700),
            height: DevicePixels(700),
        };

        let make_key = |image_id: usize| {
            AtlasKey::Image(RenderImageParams {
                image_id: ImageId(image_id),
                frame_index: 0,
            })
        };
        let insert = |key: AtlasKey, size: Size<DevicePixels>| {
            let byte_count = (size.width.0 as usize) * (size.height.0 as usize) * 4;
            atlas
                .get_or_insert_with(key, &mut || {
                    Ok(Some((size, Cow::Owned(vec![0u8; byte_count]))))
                })
                .expect("allocation should succeed")
                .expect("callback returns Some")
        };

        let keeper_key = make_key(1);
        let big_key_a = make_key(2);
        let big_key_b = make_key(3);

        let keeper_tile = insert(keeper_key, small);
        let tile_a = insert(big_key_a.clone(), big);
        assert_eq!(keeper_tile.texture_id, tile_a.texture_id);

        atlas.remove(&big_key_a);
        let tile_b = insert(big_key_b, big);
        assert_eq!(tile_b.texture_id, keeper_tile.texture_id);
        Ok(())
    }

    #[test]
    fn reused_texture_id_has_new_generation() -> anyhow::Result<()> {
        let (device, queue) = test_device_and_queue()?;
        let atlas = WgpuAtlas::new(device, queue, wgpu::TextureFormat::Bgra8Unorm);
        let size = Size {
            width: DevicePixels(700),
            height: DevicePixels(700),
        };
        let make_key = |image_id| {
            AtlasKey::Image(RenderImageParams {
                image_id: ImageId(image_id),
                frame_index: 0,
            })
        };
        let insert = |key: AtlasKey| {
            atlas
                .get_or_insert_with(key, &mut || {
                    Ok(Some((
                        size,
                        Cow::Owned(vec![0; size.width.0 as usize * size.height.0 as usize * 4]),
                    )))
                })
                .expect("allocation should succeed")
                .expect("callback returns Some")
        };

        let first_key = make_key(1);
        let first_tile = insert(first_key.clone());
        let first_generation = atlas
            .get_texture_info(first_tile.texture_id)
            .context("first texture should exist")?
            .generation;
        atlas.remove(&first_key);
        assert!(atlas.get_texture_info(first_tile.texture_id).is_none());

        let second_tile = insert(make_key(2));
        let second_generation = atlas
            .get_texture_info(second_tile.texture_id)
            .context("second texture should exist")?
            .generation;

        assert_eq!(second_tile.texture_id, first_tile.texture_id);
        assert_ne!(second_generation, first_generation);
        Ok(())
    }

    /// QUILL: a color emoji drawn while a photo was on screen landed in the
    /// photo's texture and kept it alive after the photo left.
    #[test]
    fn glyphs_never_keep_an_image_texture_alive() -> anyhow::Result<()> {
        let (device, queue) = test_device_and_queue()?;
        let atlas = WgpuAtlas::new(device, queue, wgpu::TextureFormat::Bgra8Unorm);
        let insert = |key: AtlasKey, size: Size<DevicePixels>| {
            let byte_count = (size.width.0 as usize) * (size.height.0 as usize) * 4;
            atlas
                .get_or_insert_with(key, &mut || {
                    Ok(Some((size, Cow::Owned(vec![0u8; byte_count]))))
                })
                .expect("allocation should succeed")
                .expect("callback returns Some")
        };
        let photo = AtlasKey::Image(RenderImageParams {
            image_id: ImageId(1),
            frame_index: 0,
        });
        let emoji = AtlasKey::Glyph(gpui::RenderGlyphParams {
            font_id: gpui::FontId(0),
            glyph_id: gpui::GlyphId(7),
            font_size: gpui::px(16.),
            subpixel_variant: Point::default(),
            scale_factor: 2.,
            is_emoji: true,
            subpixel_rendering: false,
            dilation: 0,
        });
        let photo_tile = insert(
            photo.clone(),
            Size {
                width: DevicePixels(720),
                height: DevicePixels(540),
            },
        );
        let emoji_tile = insert(
            emoji,
            Size {
                width: DevicePixels(64),
                height: DevicePixels(64),
            },
        );
        assert_eq!(emoji_tile.texture_id.kind, photo_tile.texture_id.kind);
        assert_ne!(emoji_tile.texture_id, photo_tile.texture_id);
        atlas.remove(&photo);
        assert!(atlas.get_texture_info(photo_tile.texture_id).is_none());
        assert!(atlas.get_texture_info(emoji_tile.texture_id).is_some());
        Ok(())
    }

    fn insert_image(atlas: &WgpuAtlas, key: AtlasKey, width: i32, height: i32) -> AtlasTile {
        let size = Size {
            width: DevicePixels(width),
            height: DevicePixels(height),
        };
        atlas
            .get_or_insert_with(key, &mut || {
                let byte_count = (width * height * 4) as usize;
                Ok(Some((size, Cow::Owned(vec![0u8; byte_count]))))
            })
            .expect("allocation should succeed")
            .expect("callback returns Some")
    }

    fn image_key(image_id: usize, frame_index: usize) -> AtlasKey {
        AtlasKey::Image(RenderImageParams {
            image_id: ImageId(image_id),
            frame_index,
        })
    }

    /// QUILL: an avatar left on screen kept the 4 MB texture larger
    /// pictures had come and gone in.
    #[test]
    fn small_pictures_keep_to_textures_of_their_own() -> anyhow::Result<()> {
        let (device, queue) = test_device_and_queue()?;
        let atlas = WgpuAtlas::new(device, queue, wgpu::TextureFormat::Bgra8Unorm);
        let side = |tile: &AtlasTile| {
            let state = atlas.0.lock();
            let texture = &state.backend.storage.get(tile.texture_id).unwrap().texture;
            (texture.width(), texture.height())
        };
        let avatar = insert_image(&atlas, image_key(1, 0), 92, 92);
        assert_eq!(side(&avatar), (512, 512));
        let photo = insert_image(&atlas, image_key(2, 0), 600, 400);
        assert_ne!(photo.texture_id, avatar.texture_id);
        assert_eq!(side(&photo), (1024, 1024));
        let emoji = insert_image(&atlas, image_key(3, 0), 40, 40);
        assert_eq!(emoji.texture_id, avatar.texture_id);
        let thumbnail = insert_image(&atlas, image_key(4, 0), 200, 200);
        assert_eq!(thumbnail.texture_id, photo.texture_id);
        // The edge `image_budget::atlas_cost` charges by.
        let small = insert_image(&atlas, image_key(5, 0), 128, 128);
        assert_eq!(small.texture_id, avatar.texture_id);
        let large = insert_image(&atlas, image_key(6, 0), 129, 128);
        assert_eq!(large.texture_id, photo.texture_id);
        atlas.remove(&image_key(2, 0));
        atlas.remove(&image_key(4, 0));
        atlas.remove(&image_key(6, 0));
        assert!(atlas.get_texture_info(photo.texture_id).is_none());
        assert!(atlas.get_texture_info(avatar.texture_id).is_some());
        Ok(())
    }

    /// QUILL: frames of an animation a few hundred pixels wide (a sticker,
    /// a GIF) rotate through one texture; a texture with room for a single
    /// frame would be made anew for every frame.
    #[test]
    fn animation_frames_rotate_through_one_texture() -> anyhow::Result<()> {
        let (device, queue) = test_device_and_queue()?;
        let atlas = WgpuAtlas::new(device, queue, wgpu::TextureFormat::Bgra8Unorm);
        let mut textures = std::collections::HashSet::new();
        for frame in 0..60 {
            let tile = insert_image(&atlas, image_key(1, frame), 300, 300);
            let generation = atlas
                .get_texture_info(tile.texture_id)
                .context("the frame's texture should exist")?
                .generation;
            textures.insert((tile.texture_id, generation));
            // Three frames are alive at a time: on screen, drawn, decoded.
            if let Some(gone) = frame.checked_sub(2) {
                atlas.remove(&image_key(1, gone));
            }
        }
        assert_eq!(textures.len(), 1);
        Ok(())
    }

    /// QUILL: a 1080p video shown as images made a texture for every
    /// picture and freed it after the next one.
    #[test]
    fn video_pictures_take_turns_in_two_textures() -> anyhow::Result<()> {
        let (device, queue) = test_device_and_queue()?;
        let atlas = WgpuAtlas::new(device, queue, wgpu::TextureFormat::Bgra8Unorm);
        let created = |atlas: &WgpuAtlas| atlas.0.lock().backend.textures_created;
        for picture in 0..30 {
            atlas.before_frame();
            insert_image(&atlas, image_key(picture, 0), 1920, 1080);
            // The picture on screen goes once the next one is up.
            if let Some(gone) = picture.checked_sub(1) {
                atlas.remove(&image_key(gone, 0));
            }
        }
        assert_eq!(created(&atlas), 2);
        // Closed: both textures wait as spares, then go after a few frames.
        atlas.remove(&image_key(29, 0));
        assert_eq!(atlas.0.lock().backend.spares.len(), 2);
        for _ in 0..SPARE_FRAMES {
            atlas.before_frame();
        }
        assert_eq!(atlas.0.lock().backend.spares.len(), 2);
        atlas.before_frame();
        assert!(atlas.0.lock().backend.spares.is_empty());
        Ok(())
    }

    #[test]
    fn swizzle_upload_data_preserves_bgra_uploads() {
        let input = vec![0x10, 0x20, 0x30, 0x40];
        let output = swizzle_upload_data(&input, wgpu::TextureFormat::Bgra8Unorm);
        // Borrowed: no copy.
        assert!(matches!(output, Cow::Borrowed(_)));
        assert_eq!(output, input);
    }

    #[test]
    fn swizzle_upload_data_converts_bgra_to_rgba() {
        let input = vec![0x10, 0x20, 0x30, 0x40, 0xAA, 0xBB, 0xCC, 0xDD];
        assert_eq!(
            swizzle_upload_data(&input, wgpu::TextureFormat::Rgba8Unorm).as_ref(),
            [0x30, 0x20, 0x10, 0x40, 0xCC, 0xBB, 0xAA, 0xDD]
        );
    }
}
