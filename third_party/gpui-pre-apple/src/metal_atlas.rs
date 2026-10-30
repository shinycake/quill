// Modified by the Quill project (2026) from gpui-pre-apple 0.3.9 (Apache-2.0):
// images in textures of their own. See QUILL-CHANGES.md.
use anyhow::{Context as _, Result};
use derive_more::{Deref, DerefMut};
use etagere::BucketedAtlasAllocator;
use gpui::{
    AtlasBackend, AtlasKey, AtlasState, AtlasTextureId, AtlasTextureKind, AtlasTextureList,
    AtlasTile, Bounds, DevicePixels, PlatformAtlas, Point, Size,
};
use metal::Device;
use parking_lot::Mutex;
use std::borrow::Cow;

pub struct MetalAtlas(Mutex<AtlasState<MetalAtlasTextures>>);

impl MetalAtlas {
    pub(crate) fn new(device: Device, supports_shared_storage: bool) -> Self {
        MetalAtlas(Mutex::new(AtlasState::new(MetalAtlasTextures {
            device: AssertSend(device),
            supports_shared_storage,
            monochrome_textures: Default::default(),
            polychrome_textures: Default::default(),
        })))
    }

    /// Returns the GPU texture backing `id`, or `None` once every tile in it
    /// has been removed. A scene can still reference such a texture when a
    /// cached view replays a paint from before the image was dropped, so
    /// callers must skip those sprites rather than assume the texture exists.
    pub(crate) fn metal_texture(&self, id: AtlasTextureId) -> Option<metal::Texture> {
        Some(self.0.lock().backend.texture(id)?.metal_texture.clone())
    }
}

struct MetalAtlasTextures {
    device: AssertSend<Device>,
    supports_shared_storage: bool,
    monochrome_textures: AtlasTextureList<MetalAtlasTexture>,
    polychrome_textures: AtlasTextureList<MetalAtlasTexture>,
}

impl PlatformAtlas for MetalAtlas {
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
}

impl AtlasBackend for MetalAtlasTextures {
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

        let textures = match id.kind {
            AtlasTextureKind::Monochrome => &mut self.monochrome_textures,
            AtlasTextureKind::Polychrome => &mut self.polychrome_textures,
            AtlasTextureKind::Subpixel => unreachable!(),
        };

        let Some(texture_slot) = textures
            .textures
            .iter_mut()
            .find(|texture| texture.as_ref().is_some_and(|v| v.id == id))
        else {
            return;
        };

        if let Some(mut texture) = texture_slot.take() {
            texture.allocator.deallocate(tile.tile_id.into());
            texture.decrement_ref_count();
            if texture.is_unreferenced() {
                textures.free_list.push(id.index as usize);
            } else {
                *texture_slot = Some(texture);
            }
        }
    }
}

impl MetalAtlasTextures {
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
        let texture = self
            .texture(tile.texture_id)
            .context("allocated tile refers to a missing texture")?;
        texture.upload(tile.bounds, bytes);
        Ok(tile)
    }

    fn allocate(
        &mut self,
        size: Size<DevicePixels>,
        texture_kind: AtlasTextureKind,
        class: TileClass,
    ) -> Option<AtlasTile> {
        {
            let textures = match texture_kind {
                AtlasTextureKind::Monochrome => &mut self.monochrome_textures,
                AtlasTextureKind::Polychrome => &mut self.polychrome_textures,
                AtlasTextureKind::Subpixel => unreachable!(),
            };

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
    ) -> &mut MetalAtlasTexture {
        const DEFAULT_ATLAS_SIZE: Size<DevicePixels> = Size {
            width: DevicePixels(1024),
            height: DevicePixels(1024),
        };
        // Max texture size on all modern Apple GPUs. Anything bigger than that crashes in validateWithDevice.
        const MAX_ATLAS_SIZE: Size<DevicePixels> = Size {
            width: DevicePixels(16384),
            height: DevicePixels(16384),
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
        let size = min_size.min(&MAX_ATLAS_SIZE).max(&floor);
        let texture_descriptor = metal::TextureDescriptor::new();
        texture_descriptor.set_width(size.width.into());
        texture_descriptor.set_height(size.height.into());
        let pixel_format;
        let usage;
        match kind {
            AtlasTextureKind::Monochrome => {
                pixel_format = metal::MTLPixelFormat::A8Unorm;
                usage = metal::MTLTextureUsage::ShaderRead;
            }
            AtlasTextureKind::Polychrome => {
                pixel_format = metal::MTLPixelFormat::BGRA8Unorm;
                usage = metal::MTLTextureUsage::ShaderRead;
            }
            AtlasTextureKind::Subpixel => unreachable!(),
        }
        texture_descriptor.set_pixel_format(pixel_format);
        texture_descriptor.set_usage(usage);
        // Shared memory mode can be used only on Apple GPU families
        // https://developer.apple.com/documentation/metal/mtlresourceoptions/storagemodeshared
        texture_descriptor.set_storage_mode(if self.supports_shared_storage {
            metal::MTLStorageMode::Shared
        } else {
            metal::MTLStorageMode::Managed
        });
        let metal_texture = self.device.new_texture(&texture_descriptor);

        let texture_list = match kind {
            AtlasTextureKind::Monochrome => &mut self.monochrome_textures,
            AtlasTextureKind::Polychrome => &mut self.polychrome_textures,
            AtlasTextureKind::Subpixel => unreachable!(),
        };

        let index = texture_list.free_list.pop();

        let atlas_texture = MetalAtlasTexture {
            id: AtlasTextureId {
                index: index.unwrap_or(texture_list.textures.len()) as u32,
                kind,
            },
            allocator: etagere::BucketedAtlasAllocator::new(size_to_etagere(size)),
            metal_texture: AssertSend(metal_texture),
            live_atlas_keys: 0,
            class,
        };

        if let Some(ix) = index {
            texture_list.textures[ix] = Some(atlas_texture);
            texture_list.textures.get_mut(ix)
        } else {
            texture_list.textures.push(Some(atlas_texture));
            texture_list.textures.last_mut()
        }
        .unwrap()
        .as_mut()
        .unwrap()
    }

    fn texture(&self, id: AtlasTextureId) -> Option<&MetalAtlasTexture> {
        let textures = match id.kind {
            AtlasTextureKind::Monochrome => &self.monochrome_textures,
            AtlasTextureKind::Polychrome => &self.polychrome_textures,
            AtlasTextureKind::Subpixel => unreachable!(),
        };
        textures.textures.get(id.index as usize)?.as_ref()
    }
}

struct MetalAtlasTexture {
    id: AtlasTextureId,
    allocator: BucketedAtlasAllocator,
    metal_texture: AssertSend<metal::Texture>,
    live_atlas_keys: u32,
    /// QUILL: the class of every tile it holds.
    class: TileClass,
}

impl MetalAtlasTexture {
    fn allocate(&mut self, size: Size<DevicePixels>) -> Option<AtlasTile> {
        let allocation = self.allocator.allocate(size_to_etagere(size))?;
        let tile = AtlasTile {
            texture_id: self.id,
            tile_id: allocation.id.into(),
            bounds: Bounds {
                origin: point_from_etagere(allocation.rectangle.min),
                size,
            },
            padding: 0,
        };
        self.live_atlas_keys += 1;
        Some(tile)
    }

    fn upload(&self, bounds: Bounds<DevicePixels>, bytes: &[u8]) {
        let region = metal::MTLRegion::new_2d(
            bounds.origin.x.into(),
            bounds.origin.y.into(),
            bounds.size.width.into(),
            bounds.size.height.into(),
        );
        self.metal_texture.replace_region(
            region,
            0,
            bytes.as_ptr() as *const _,
            bounds.size.width.to_bytes(self.bytes_per_pixel()) as u64,
        );
    }

    fn bytes_per_pixel(&self) -> u8 {
        use metal::MTLPixelFormat::*;
        match self.metal_texture.pixel_format() {
            A8Unorm | R8Unorm => 1,
            RGBA8Unorm | BGRA8Unorm => 4,
            _ => unimplemented!(),
        }
    }

    fn decrement_ref_count(&mut self) {
        self.live_atlas_keys -= 1;
    }

    fn is_unreferenced(&mut self) -> bool {
        self.live_atlas_keys == 0
    }
}

fn size_to_etagere(size: Size<DevicePixels>) -> etagere::Size {
    etagere::Size::new(size.width.into(), size.height.into())
}

fn point_from_etagere(value: etagere::Point) -> Point<DevicePixels> {
    Point {
        x: DevicePixels::from(value.x),
        y: DevicePixels::from(value.y),
    }
}

#[derive(Deref, DerefMut)]
struct AssertSend<T>(T);

unsafe impl<T> Send for AssertSend<T> {}

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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::PlatformAtlas;
    use std::borrow::Cow;

    fn create_atlas() -> Option<MetalAtlas> {
        let device = metal::Device::system_default()?;
        Some(MetalAtlas::new(device, true))
    }

    fn make_image_key(image_id: usize, frame_index: usize) -> AtlasKey {
        AtlasKey::Image(gpui::RenderImageParams {
            image_id: gpui::ImageId(image_id),
            frame_index,
        })
    }

    fn insert_tile(atlas: &MetalAtlas, key: AtlasKey, size: Size<DevicePixels>) -> AtlasTile {
        atlas
            .get_or_insert_with(key, &mut || {
                let byte_count = (size.width.0 as usize) * (size.height.0 as usize) * 4;
                Ok(Some((size, Cow::Owned(vec![0u8; byte_count]))))
            })
            .expect("allocation should succeed")
            .expect("callback returns Some")
    }

    #[test]
    fn test_remove_clears_stale_keys_from_tiles_by_key() {
        let Some(atlas) = create_atlas() else {
            return;
        };

        let small = Size {
            width: DevicePixels(64),
            height: DevicePixels(64),
        };

        let key_a = make_image_key(1, 0);
        let key_b = make_image_key(2, 0);
        let key_c = make_image_key(3, 0);

        let tile_a = insert_tile(&atlas, key_a.clone(), small);
        let tile_b = insert_tile(&atlas, key_b.clone(), small);
        let tile_c = insert_tile(&atlas, key_c.clone(), small);

        assert_eq!(tile_a.texture_id, tile_b.texture_id);
        assert_eq!(tile_b.texture_id, tile_c.texture_id);

        // Remove A: texture still has B and C, so it stays.
        // The key for A must be removed from tiles_by_key.
        atlas.remove(&key_a);

        // Remove B: texture still has C.
        atlas.remove(&key_b);

        // Remove C: texture becomes unreferenced and is deleted.
        atlas.remove(&key_c);

        // Re-inserting A must allocate a fresh tile on a new texture,
        // NOT return a stale tile referencing the deleted texture.
        let tile_a2 = insert_tile(&atlas, key_a, small);

        // The texture must actually exist — this would panic before the fix.
        assert!(atlas.metal_texture(tile_a2.texture_id).is_some());
    }

    #[test]
    fn test_metal_texture_is_none_after_last_tile_removed() {
        let Some(atlas) = create_atlas() else {
            return;
        };

        let key = make_image_key(1, 0);
        let tile = insert_tile(
            &atlas,
            key.clone(),
            Size {
                width: DevicePixels(64),
                height: DevicePixels(64),
            },
        );
        assert!(atlas.metal_texture(tile.texture_id).is_some());

        // A scene built before the removal may still carry `tile`; looking its
        // texture up must report the gap instead of panicking.
        atlas.remove(&key);
        assert!(atlas.metal_texture(tile.texture_id).is_none());
    }

    #[test]
    fn test_remove_deallocates_tile_space_for_reuse() {
        let Some(atlas) = create_atlas() else {
            return;
        };

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

        let keeper_key = make_image_key(1, 0);
        let big_key_a = make_image_key(2, 0);
        let big_key_b = make_image_key(3, 0);

        let keeper_tile = insert_tile(&atlas, keeper_key, small);
        let tile_a = insert_tile(&atlas, big_key_a.clone(), big);
        assert_eq!(keeper_tile.texture_id, tile_a.texture_id);

        atlas.remove(&big_key_a);
        let tile_b = insert_tile(&atlas, big_key_b, big);
        assert_eq!(tile_b.texture_id, keeper_tile.texture_id);
    }

    fn make_emoji_key(glyph: u32) -> AtlasKey {
        AtlasKey::Glyph(gpui::RenderGlyphParams {
            font_id: gpui::FontId(0),
            glyph_id: gpui::GlyphId(glyph),
            font_size: gpui::px(16.),
            subpixel_variant: Point::default(),
            scale_factor: 2.,
            is_emoji: true,
            subpixel_rendering: false,
            dilation: 0,
        })
    }

    /// QUILL: a color emoji drawn while a photo was on screen landed in the
    /// photo's texture and kept it alive after the photo left.
    #[test]
    fn test_glyphs_never_keep_an_image_texture_alive() {
        let Some(atlas) = create_atlas() else {
            return;
        };
        let photo = make_image_key(1, 0);
        let emoji = make_emoji_key(7);
        let photo_tile = insert_tile(
            &atlas,
            photo.clone(),
            Size {
                width: DevicePixels(720),
                height: DevicePixels(540),
            },
        );
        let emoji_tile = insert_tile(
            &atlas,
            emoji,
            Size {
                width: DevicePixels(64),
                height: DevicePixels(64),
            },
        );
        assert_eq!(emoji_tile.texture_id.kind, photo_tile.texture_id.kind);
        assert_ne!(emoji_tile.texture_id, photo_tile.texture_id);
        atlas.remove(&photo);
        assert!(atlas.metal_texture(photo_tile.texture_id).is_none());
        assert!(atlas.metal_texture(emoji_tile.texture_id).is_some());
    }

    /// QUILL: an avatar left on screen kept the 4 MB texture larger
    /// pictures had come and gone in.
    #[test]
    fn test_small_pictures_get_small_textures() {
        let Some(atlas) = create_atlas() else {
            return;
        };
        let side = |tile: &AtlasTile| {
            let texture = atlas.metal_texture(tile.texture_id).unwrap();
            (texture.width(), texture.height())
        };
        let size = |width, height| Size {
            width: DevicePixels(width),
            height: DevicePixels(height),
        };
        let avatar = insert_tile(&atlas, make_image_key(1, 0), size(92, 92));
        assert_eq!(side(&avatar), (512, 512));
        let photo = insert_tile(&atlas, make_image_key(2, 0), size(720, 540));
        assert_ne!(photo.texture_id, avatar.texture_id);
        assert_eq!(side(&photo), (1024, 1024));
        let emoji = insert_tile(&atlas, make_emoji_key(7), size(64, 64));
        assert_eq!(side(&emoji), (1024, 1024));
    }

    #[test]
    fn test_remove_nonexistent_key_is_noop() {
        let Some(atlas) = create_atlas() else {
            return;
        };
        let key = make_image_key(999, 0);
        atlas.remove(&key);
    }
}
