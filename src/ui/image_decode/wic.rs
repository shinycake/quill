//! Windows Imaging Component: HEIC / HEIF and AVIF through Microsoft's HEIF
//! and AV1 Video extensions (installed with Windows 11, free in the Store
//! for Windows 10), plus whatever other codecs the system has (camera RAW,
//! JPEG XL extension). Without the extension WIC reports the format as
//! unknown and the file shows its thumbnail with "open in another app".
//!
//! The frame's embedded colour profile is converted to sRGB with a WIC
//! colour transform; the EXIF / HEIF orientation is read from the frame's
//! metadata (`System.Photo.Orientation`) and applied afterwards.

use super::{DecodeError, DecodeSize, area_allowed};
use image::metadata::Orientation;
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_WICPixelFormat32bppBGRA, IWICBitmapFrameDecode, IWICBitmapSource,
    IWICColorContext, IWICImagingFactory, WICBitmapDitherTypeNone, WICBitmapInterpolationModeFant,
    WICBitmapPaletteTypeCustom, WICDecodeMetadataCacheOnDemand,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::core::{Interface, w};

fn failed(err: windows::core::Error) -> DecodeError {
    DecodeError::Failed(err.message())
}

/// One `CoInitializeEx` on this thread, balanced by `CoUninitialize` when it
/// succeeded (`S_OK` or `S_FALSE`). A thread already in another apartment
/// (`RPC_E_CHANGED_MODE`) keeps it, and WIC works there too.
struct ComApartment(bool);

impl ComApartment {
    fn join() -> Self {
        // SAFETY: joining this worker thread to the multithreaded apartment.
        Self(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the successful `CoInitializeEx` above, on the
            // same thread, after every WIC interface of the call is gone.
            unsafe { CoUninitialize() };
        }
    }
}

pub(super) fn decode(bytes: &[u8], size: DecodeSize) -> Result<image::RgbaImage, DecodeError> {
    let _apartment = ComApartment::join();
    decode_in_apartment(bytes, size)
}

fn decode_in_apartment(bytes: &[u8], size: DecodeSize) -> Result<image::RgbaImage, DecodeError> {
    // SAFETY: creating the in-process WIC factory.
    let factory: IWICImagingFactory =
        unsafe { CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER) }
            .map_err(failed)?;
    // SAFETY: WIC calls on live interfaces; `bytes` outlives the stream,
    // which is dropped before this function returns.
    unsafe {
        let stream = factory.CreateStream().map_err(failed)?;
        stream.InitializeFromMemory(bytes).map_err(failed)?;
        let decoder = factory
            .CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand)
            .map_err(|_| DecodeError::Unsupported)?;
        let frame = decoder.GetFrame(0).map_err(failed)?;
        let (mut width, mut height) = (0_u32, 0_u32);
        frame.GetSize(&mut width, &mut height).map_err(failed)?;
        area_allowed(width, height)?;
        let orientation = orientation(&frame);
        let (target_w, target_h) = size.fit((width, height));
        let mut source: IWICBitmapSource = frame.cast().map_err(failed)?;
        if (target_w, target_h) != (width, height) {
            let scaler = factory.CreateBitmapScaler().map_err(failed)?;
            scaler
                .Initialize(&source, target_w, target_h, WICBitmapInterpolationModeFant)
                .map_err(failed)?;
            source = scaler.cast().map_err(failed)?;
        }
        let converter = factory.CreateFormatConverter().map_err(failed)?;
        converter
            .Initialize(
                &source,
                &GUID_WICPixelFormat32bppBGRA,
                WICBitmapDitherTypeNone,
                None,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(failed)?;
        source = converter.cast().map_err(failed)?;
        if let Some(managed) = to_srgb(&factory, &frame, &source) {
            source = managed;
        }
        let stride = target_w.checked_mul(4).ok_or(DecodeError::TooLarge)?;
        let len = (stride as usize)
            .checked_mul(target_h as usize)
            .ok_or(DecodeError::TooLarge)?;
        let mut pixels = vec![0_u8; len];
        source
            .CopyPixels(std::ptr::null(), stride, &mut pixels)
            .map_err(failed)?;
        // BGRA → RGBA (straight alpha: `32bppBGRA` is not premultiplied).
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let image = image::RgbaImage::from_raw(target_w, target_h, pixels)
            .ok_or_else(|| DecodeError::Failed("WIC size mismatch".into()))?;
        let mut image = image::DynamicImage::ImageRgba8(image);
        image.apply_orientation(orientation);
        Ok(image.into_rgba8())
    }
}

/// The frame's first embedded colour profile converted to sRGB, or `None`
/// when it has none (the pixels are taken as sRGB) or WIC cannot convert.
///
/// # Safety
/// Live WIC interfaces from the same factory.
unsafe fn to_srgb(
    factory: &IWICImagingFactory,
    frame: &IWICBitmapFrameDecode,
    source: &IWICBitmapSource,
) -> Option<IWICBitmapSource> {
    // SAFETY: the caller's interfaces are live; the count query passes a
    // null array with a count of 0 (the documented way to ask), the second
    // call a slice of that many created contexts.
    unsafe {
        let mut count = 0_u32;
        (Interface::vtable(frame).GetColorContexts)(
            Interface::as_raw(frame),
            0,
            std::ptr::null_mut(),
            &mut count,
        )
        .ok()
        .ok()?;
        if count == 0 {
            return None;
        }
        let mut contexts: Vec<Option<IWICColorContext>> = (0..count.min(8))
            .map(|_| factory.CreateColorContext().ok())
            .collect();
        if contexts.iter().any(Option::is_none) {
            return None;
        }
        frame.GetColorContexts(&mut contexts, &mut count).ok()?;
        let from = contexts.into_iter().next().flatten()?;
        let to = factory.CreateColorContext().ok()?;
        // EXIF colour space 1 = sRGB.
        to.InitializeFromExifColorSpace(1).ok()?;
        let transform = factory.CreateColorTransformer().ok()?;
        transform
            .Initialize(source, &from, &to, &GUID_WICPixelFormat32bppBGRA)
            .ok()?;
        transform.cast().ok()
    }
}

/// `System.Photo.Orientation` from the frame's metadata (EXIF for JPEG and
/// TIFF, `irot` / `imir` for HEIF), or none.
///
/// # Safety
/// A live frame.
unsafe fn orientation(frame: &IWICBitmapFrameDecode) -> Orientation {
    // SAFETY: the caller's frame is live; `value` is a PROPVARIANT the
    // reader fills and that is dropped (cleared) here.
    let exif = unsafe {
        frame.GetMetadataQueryReader().ok().and_then(|reader| {
            let mut value = PROPVARIANT::default();
            reader
                .GetMetadataByName(w!("System.Photo.Orientation"), &mut value)
                .ok()?;
            u16::try_from(&value).ok()
        })
    };
    exif.and_then(|value| u8::try_from(value).ok())
        .and_then(Orientation::from_exif)
        .unwrap_or(Orientation::NoTransforms)
}
