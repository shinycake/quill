//! HEIC / HEIF / AVIF through the system libheif, opened at run time
//! (`libheif.so.1`, which GNOME, KDE and most distributions install for
//! their own image viewers). Quill neither links nor ships libheif: without
//! it these files show their thumbnail and open in another app. libheif's
//! AVIF support depends on how the distribution built it (dav1d or aom).
//!
//! libheif applies the file's rotation and mirroring while decoding; the
//! colour profile (ICC or nclx) is converted to sRGB here.

use super::rust_codecs::to_srgb;
use super::{DecodeError, DecodeSize, area_allowed};
use std::ffi::{c_char, c_int, c_void};
use std::sync::OnceLock;

#[repr(C)]
#[derive(Clone, Copy)]
struct HeifError {
    code: c_int,
    _subcode: c_int,
    _message: *const c_char,
}

impl HeifError {
    fn check(self) -> Result<(), DecodeError> {
        if self.code == 0 {
            Ok(())
        } else {
            Err(DecodeError::Failed(format!("libheif error {}", self.code)))
        }
    }
}

/// The start of `struct heif_color_profile_nclx` (libheif `heif.h`): a
/// version byte, then the int-sized enums. Only these fields are read.
#[repr(C)]
struct NclxProfile {
    _version: u8,
    color_primaries: c_int,
    transfer_characteristics: c_int,
}

enum Context {}
enum Handle {}
enum Image {}

const COLORSPACE_RGB: c_int = 1;
const CHROMA_INTERLEAVED_RGBA: c_int = 11;
const CHANNEL_INTERLEAVED: c_int = 10;
/// `heif_color_profile_type_nclx` ('nclx' as a big-endian four-char code).
const PROFILE_NCLX: c_int = i32::from_be_bytes(*b"nclx");

struct Api {
    context_alloc: unsafe extern "C" fn() -> *mut Context,
    context_free: unsafe extern "C" fn(*mut Context),
    read_from_memory: unsafe extern "C" fn(*mut Context, *const c_void, usize, *const c_void) -> HeifError,
    primary_handle: unsafe extern "C" fn(*mut Context, *mut *mut Handle) -> HeifError,
    handle_release: unsafe extern "C" fn(*const Handle),
    handle_width: unsafe extern "C" fn(*const Handle) -> c_int,
    handle_height: unsafe extern "C" fn(*const Handle) -> c_int,
    profile_type: unsafe extern "C" fn(*const Handle) -> c_int,
    raw_profile_size: unsafe extern "C" fn(*const Handle) -> usize,
    raw_profile: unsafe extern "C" fn(*const Handle, *mut c_void) -> HeifError,
    nclx_profile: unsafe extern "C" fn(*const Handle, *mut *mut NclxProfile) -> HeifError,
    nclx_free: unsafe extern "C" fn(*mut NclxProfile),
    decode_image: unsafe extern "C" fn(*const Handle, *mut *mut Image, c_int, c_int, *const c_void) -> HeifError,
    image_release: unsafe extern "C" fn(*const Image),
    image_width: unsafe extern "C" fn(*const Image, c_int) -> c_int,
    image_height: unsafe extern "C" fn(*const Image, c_int) -> c_int,
    plane: unsafe extern "C" fn(*const Image, c_int, *mut c_int) -> *const u8,
    /// Keeps the functions above loaded.
    _library: libloading::Library,
}

fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(load).as_ref()
}

#[cfg(target_os = "macos")]
const LIBRARY_NAMES: &[&str] = &[
    "libheif.1.dylib",
    "/opt/homebrew/lib/libheif.1.dylib",
    "/usr/local/lib/libheif.1.dylib",
];
#[cfg(not(target_os = "macos"))]
const LIBRARY_NAMES: &[&str] = &["libheif.so.1", "libheif.so"];

fn load() -> Option<Api> {
    let library = LIBRARY_NAMES.iter().find_map(|name| {
        // SAFETY: libheif's initialisers only set up its plugin registry.
        unsafe { libloading::Library::new(*name) }.ok()
    })?;
    macro_rules! symbol {
        ($name:literal) => {
            // SAFETY: the declared signature matches libheif's `heif.h`
            // (stable C API since libheif 1.x).
            *unsafe { library.get(concat!($name, "\0").as_bytes()) }.ok()?
        };
    }
    Some(Api {
        context_alloc: symbol!("heif_context_alloc"),
        context_free: symbol!("heif_context_free"),
        read_from_memory: symbol!("heif_context_read_from_memory_without_copy"),
        primary_handle: symbol!("heif_context_get_primary_image_handle"),
        handle_release: symbol!("heif_image_handle_release"),
        handle_width: symbol!("heif_image_handle_get_width"),
        handle_height: symbol!("heif_image_handle_get_height"),
        profile_type: symbol!("heif_image_handle_get_color_profile_type"),
        raw_profile_size: symbol!("heif_image_handle_get_raw_color_profile_size"),
        raw_profile: symbol!("heif_image_handle_get_raw_color_profile"),
        nclx_profile: symbol!("heif_image_handle_get_nclx_color_profile"),
        nclx_free: symbol!("heif_nclx_color_profile_free"),
        decode_image: symbol!("heif_decode_image"),
        image_release: symbol!("heif_image_release"),
        image_width: symbol!("heif_image_get_width"),
        image_height: symbol!("heif_image_get_height"),
        plane: symbol!("heif_image_get_plane_readonly"),
        _library: library,
    })
}

/// Frees what libheif allocated, whatever path the decode leaves by.
struct Owned<'a> {
    api: &'a Api,
    context: *mut Context,
    handle: *mut Handle,
    image: *mut Image,
}

impl Drop for Owned<'_> {
    fn drop(&mut self) {
        // SAFETY: each pointer is either null or was returned by libheif and
        // not freed yet; libheif's free functions accept only those.
        unsafe {
            if !self.image.is_null() {
                (self.api.image_release)(self.image);
            }
            if !self.handle.is_null() {
                (self.api.handle_release)(self.handle);
            }
            if !self.context.is_null() {
                (self.api.context_free)(self.context);
            }
        }
    }
}

pub(super) fn decode(bytes: &[u8], size: DecodeSize) -> Result<image::RgbaImage, DecodeError> {
    let api = api().ok_or(DecodeError::Unsupported)?;
    let mut owned = Owned {
        api,
        // SAFETY: allocation has no preconditions.
        context: unsafe { (api.context_alloc)() },
        handle: std::ptr::null_mut(),
        image: std::ptr::null_mut(),
    };
    if owned.context.is_null() {
        return Err(DecodeError::Failed("libheif context".into()));
    }
    // SAFETY: `bytes` outlives the context (`owned` drops before this
    // function returns); null options mean libheif's defaults.
    unsafe {
        (api.read_from_memory)(
            owned.context,
            bytes.as_ptr().cast(),
            bytes.len(),
            std::ptr::null(),
        )
    }
    .check()
    .map_err(|_| DecodeError::Unsupported)?;
    // SAFETY: the context holds a parsed file; the handle lands in `owned`.
    unsafe { (api.primary_handle)(owned.context, &mut owned.handle) }.check()?;
    // SAFETY: `owned.handle` is a live handle.
    let (width, height) = unsafe { ((api.handle_width)(owned.handle), (api.handle_height)(owned.handle)) };
    area_allowed(
        u32::try_from(width).unwrap_or(0),
        u32::try_from(height).unwrap_or(0),
    )?;
    let profile = colour_profile(api, owned.handle);
    // SAFETY: as above; null options decode with the file's rotation and
    // mirroring applied.
    unsafe {
        (api.decode_image)(
            owned.handle,
            &mut owned.image,
            COLORSPACE_RGB,
            CHROMA_INTERLEAVED_RGBA,
            std::ptr::null(),
        )
    }
    .check()?;
    let mut stride: c_int = 0;
    // SAFETY: `owned.image` is a live interleaved RGBA image.
    let (plane, out_w, out_h) = unsafe {
        (
            (api.plane)(owned.image, CHANNEL_INTERLEAVED, &mut stride),
            (api.image_width)(owned.image, CHANNEL_INTERLEAVED),
            (api.image_height)(owned.image, CHANNEL_INTERLEAVED),
        )
    };
    let (Ok(out_w), Ok(out_h), Ok(stride)) = (
        usize::try_from(out_w),
        usize::try_from(out_h),
        usize::try_from(stride),
    ) else {
        return Err(DecodeError::Failed("libheif plane".into()));
    };
    let row = out_w.checked_mul(4).ok_or(DecodeError::TooLarge)?;
    if plane.is_null() || stride < row || out_w == 0 || out_h == 0 {
        return Err(DecodeError::Failed("libheif plane".into()));
    }
    let total = stride.checked_mul(out_h).ok_or(DecodeError::TooLarge)?;
    // SAFETY: libheif's plane is `stride * height` bytes and lives as long
    // as `owned.image`, which outlives this slice.
    let source = unsafe { std::slice::from_raw_parts(plane, total) };
    let mut pixels = Vec::with_capacity(row * out_h);
    for line in source.chunks_exact(stride).take(out_h) {
        pixels.extend_from_slice(line.get(..row).unwrap_or_default());
    }
    drop(owned);
    let (w32, h32) = (
        u32::try_from(out_w).map_err(|_| DecodeError::TooLarge)?,
        u32::try_from(out_h).map_err(|_| DecodeError::TooLarge)?,
    );
    let mut image = image::RgbaImage::from_raw(w32, h32, pixels)
        .ok_or_else(|| DecodeError::Failed("libheif size mismatch".into()))?;
    let target = size.fit((w32, h32));
    if target != (w32, h32) {
        image = image::imageops::resize(
            &image,
            target.0,
            target.1,
            image::imageops::FilterType::Triangle,
        );
    }
    match profile {
        Profile::Icc(icc) => to_srgb(&mut image, moxcms::Layout::Rgba, &icc),
        Profile::Cicp(source) => {
            if let Ok(transform) = source.create_transform_8bit(
                moxcms::Layout::Rgba,
                &moxcms::ColorProfile::new_srgb(),
                moxcms::Layout::Rgba,
                moxcms::TransformOptions::default(),
            ) {
                let mut converted = vec![0_u8; image.len()];
                if transform.transform(&image, &mut converted).is_ok() {
                    image.copy_from_slice(&converted);
                }
            }
        }
        Profile::Srgb => {}
    }
    Ok(image)
}

enum Profile {
    Srgb,
    Icc(Vec<u8>),
    Cicp(Box<moxcms::ColorProfile>),
}

/// The handle's colour profile: an embedded ICC profile, or the nclx
/// primaries mapped to the matching standard profile (Display P3 from
/// phones, BT.2020 from HDR cameras; HDR transfer curves are clipped to
/// SDR here).
fn colour_profile(api: &Api, handle: *mut Handle) -> Profile {
    // SAFETY: `handle` is a live handle.
    let kind = unsafe { (api.profile_type)(handle) };
    if kind == 0 {
        return Profile::Srgb;
    }
    if kind != PROFILE_NCLX {
        // SAFETY: as above.
        let size = unsafe { (api.raw_profile_size)(handle) };
        if size == 0 || size > 4 * 1024 * 1024 {
            return Profile::Srgb;
        }
        let mut icc = vec![0_u8; size];
        // SAFETY: `icc` has the size libheif reported.
        let read = unsafe { (api.raw_profile)(handle, icc.as_mut_ptr().cast()) };
        return if read.check().is_ok() {
            Profile::Icc(icc)
        } else {
            Profile::Srgb
        };
    }
    let mut nclx: *mut NclxProfile = std::ptr::null_mut();
    // SAFETY: as above; the profile is freed below.
    if unsafe { (api.nclx_profile)(handle, &mut nclx) }.check().is_err() || nclx.is_null() {
        return Profile::Srgb;
    }
    // SAFETY: libheif returned a live profile; its first fields match
    // `NclxProfile`.
    let (primaries, transfer) =
        unsafe { ((*nclx).color_primaries, (*nclx).transfer_characteristics) };
    // SAFETY: freeing the profile libheif allocated, once.
    unsafe { (api.nclx_free)(nclx) };
    const PQ: c_int = 16;
    const HLG: c_int = 18;
    match (primaries, transfer) {
        (12, _) => Profile::Cicp(Box::new(moxcms::ColorProfile::new_display_p3())),
        (9, PQ) => Profile::Cicp(Box::new(moxcms::ColorProfile::new_bt2020_pq())),
        (9, HLG) => Profile::Cicp(Box::new(moxcms::ColorProfile::new_bt2020_hlg())),
        (9, _) => Profile::Cicp(Box::new(moxcms::ColorProfile::new_bt2020())),
        _ => Profile::Srgb,
    }
}
