// Added by the Quill project (2026). See third_party/gpui-pre-windows/QUILL-CHANGES.md.
//! HDR output for HDR stills: the application asks for an scRGB swap chain
//! on a window ([`set_hdr_output`]) while it shows one, and reads what the
//! window's display can do ([`display_info`]).
//!
//! The renderer switches its swap chain to `R16G16B16A16_FLOAT` in the
//! scRGB colour space (linear, 1.0 = 80 nits) only while the window asks
//! for it and the display is in Windows HDR mode. Every pixel shader then
//! writes the user's SDR white for UI white, so the rest of the window looks
//! the same; HDR surfaces go above it.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL, DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SDR_WHITE_LEVEL, DISPLAYCONFIG_SOURCE_DEVICE_NAME, DisplayConfigGetDeviceInfo,
    GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows::Win32::Foundation::{ERROR_SUCCESS, HWND};
use windows::Win32::Graphics::Dxgi::Common::DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020;
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput6};
use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromWindow};
use windows::core::Interface;

/// scRGB's unit: 1.0 is 80 nits.
pub const SCRGB_NITS: f32 = 80.0;

/// The SDR white Windows uses when it can't say (its default slider value).
const DEFAULT_SDR_WHITE_NITS: f32 = 200.0;

/// How long a display reading is reused.
const INFO_TTL: Duration = Duration::from_secs(1);

/// What a window's display can show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HdrDisplayInfo {
    /// Windows HDR ("Use HDR") is on for the display.
    pub hdr_enabled: bool,
    /// The user's SDR content brightness, in nits.
    pub sdr_white_nits: f32,
    /// The display's peak brightness, in nits.
    pub max_nits: f32,
}

impl HdrDisplayInfo {
    const SDR: Self = Self {
        hdr_enabled: false,
        sdr_white_nits: SCRGB_NITS,
        max_nits: SCRGB_NITS,
    };

    /// How far above SDR white the display goes (1.0 = none).
    pub fn headroom(&self) -> f32 {
        if !self.hdr_enabled || self.sdr_white_nits <= 0.0 {
            return 1.0;
        }
        (self.max_nits / self.sdr_white_nits).max(1.0)
    }

    /// SDR white in scRGB units.
    pub fn output_scale(&self) -> f32 {
        (self.sdr_white_nits / SCRGB_NITS).max(1.0)
    }
}

static WANTED: LazyLock<Mutex<HashMap<isize, bool>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static INFO: LazyLock<Mutex<HashMap<isize, (Instant, HdrDisplayInfo)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Ask for (`true`) or release (`false`) HDR output on the window with the
/// given handle. Takes effect on the window's next frame, and only while
/// its display is in HDR mode.
pub fn set_hdr_output(hwnd: isize, on: bool) {
    if let Ok(mut wanted) = WANTED.lock() {
        if on {
            wanted.insert(hwnd, true);
        } else {
            wanted.remove(&hwnd);
        }
    }
}

/// QUILL: windows whose switch to HDR output failed (the renderer stays in
/// SDR until the window releases HDR output).
static FAILED: LazyLock<Mutex<HashMap<isize, bool>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// QUILL: the renderer records whether its switch to HDR output failed.
pub(crate) fn note_failed(hwnd: HWND, failed: bool) {
    if let Ok(mut map) = FAILED.lock() {
        if failed {
            map.insert(hwnd.0 as isize, true);
        } else {
            map.remove(&(hwnd.0 as isize));
        }
    }
}

/// QUILL: whether the window asked for HDR output and its renderer could
/// not switch to it, so the window draws in SDR.
pub fn hdr_failed(hwnd: isize) -> bool {
    FAILED.lock().ok().is_some_and(|map| map.contains_key(&hwnd))
}

/// Drop what is recorded for a window that is going away.
pub(crate) fn forget(hwnd: HWND) {
    let key = hwnd.0 as isize;
    if let Ok(mut wanted) = WANTED.lock() {
        wanted.remove(&key);
    }
    if let Ok(mut failed) = FAILED.lock() {
        failed.remove(&key);
    }
    if let Ok(mut info) = INFO.lock() {
        info.remove(&key);
    }
}

/// Whether the window asked for HDR output.
pub(crate) fn wanted(hwnd: HWND) -> bool {
    WANTED
        .lock()
        .ok()
        .is_some_and(|wanted| wanted.contains_key(&(hwnd.0 as isize)))
}

/// What the display the window is on can show (re-read at most once a
/// second; SDR when it can't be read).
pub fn display_info(hwnd: isize) -> HdrDisplayInfo {
    if let Ok(cache) = INFO.lock()
        && let Some((at, info)) = cache.get(&hwnd)
        && at.elapsed() < INFO_TTL
    {
        return *info;
    }
    let info = read_display_info(HWND(hwnd as _)).unwrap_or(HdrDisplayInfo::SDR);
    if let Ok(mut cache) = INFO.lock() {
        cache.insert(hwnd, (Instant::now(), info));
    }
    info
}

fn read_display_info(hwnd: HWND) -> Option<HdrDisplayInfo> {
    // SAFETY: plain Win32 / DXGI calls on a window handle and the COM
    // objects they return; every result is checked.
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut adapter_index = 0;
        while let Ok(adapter) = factory.EnumAdapters1(adapter_index) {
            adapter_index += 1;
            let mut output_index = 0;
            while let Ok(output) = adapter.EnumOutputs(output_index) {
                output_index += 1;
                let Ok(desc) = output.GetDesc() else {
                    continue;
                };
                if desc.Monitor != monitor {
                    continue;
                }
                let output6: IDXGIOutput6 = output.cast().ok()?;
                let desc1 = output6.GetDesc1().ok()?;
                let hdr_enabled = desc1.ColorSpace == DXGI_COLOR_SPACE_RGB_FULL_G2084_NONE_P2020;
                let sdr_white_nits =
                    sdr_white_nits(&desc1.DeviceName).unwrap_or(DEFAULT_SDR_WHITE_NITS);
                return Some(HdrDisplayInfo {
                    hdr_enabled,
                    sdr_white_nits,
                    max_nits: if desc1.MaxLuminance > 0.0 {
                        desc1.MaxLuminance
                    } else {
                        sdr_white_nits
                    },
                });
            }
        }
        None
    }
}

/// The SDR white level of the display whose GDI device name is
/// `device_name`, through the display configuration API.
fn sdr_white_nits(device_name: &[u16; 32]) -> Option<f32> {
    let mut path_count = 0_u32;
    let mut mode_count = 0_u32;
    // SAFETY: the counts are written by the call.
    let sizes = unsafe {
        GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
    };
    if sizes != ERROR_SUCCESS {
        return None;
    }
    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
    // SAFETY: the arrays hold the counts the previous call returned, and the
    // call writes back how many it filled.
    let queried = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
    };
    if queried != ERROR_SUCCESS {
        return None;
    }
    paths.truncate(path_count as usize);
    for path in &paths {
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: std::mem::size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                adapterId: path.sourceInfo.adapterId,
                id: path.sourceInfo.id,
            },
            ..Default::default()
        };
        // SAFETY: the packet starts with its header, sized for its type.
        if unsafe { DisplayConfigGetDeviceInfo(&mut source.header) } != 0 {
            continue;
        }
        if !same_name(&source.viewGdiDeviceName, device_name) {
            continue;
        }
        let mut white = DISPLAYCONFIG_SDR_WHITE_LEVEL {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SDR_WHITE_LEVEL,
                size: std::mem::size_of::<DISPLAYCONFIG_SDR_WHITE_LEVEL>() as u32,
                adapterId: path.targetInfo.adapterId,
                id: path.targetInfo.id,
            },
            SDRWhiteLevel: 0,
        };
        // SAFETY: as above.
        if unsafe { DisplayConfigGetDeviceInfo(&mut white.header) } != 0 {
            return None;
        }
        // 1000 means 80 nits.
        let nits = white.SDRWhiteLevel as f32 / 1000.0 * SCRGB_NITS;
        return (nits > 0.0).then_some(nits);
    }
    None
}

/// Compare two NUL-terminated UTF-16 names.
fn same_name(a: &[u16], b: &[u16]) -> bool {
    let end = |name: &[u16]| name.iter().position(|&c| c == 0).unwrap_or(name.len());
    a.get(..end(a)) == b.get(..end(b))
}
