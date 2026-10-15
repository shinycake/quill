//! The FIDO HID devices plugged into this computer (usage page 0xF1D0):
//! `/dev/hidraw*` on Linux, IOKit on macOS. Windows has none here: it
//! goes through `webauthn.dll`.
#![deny(clippy::indexing_slicing)]

use super::ctaphid::HidDevice;

#[cfg(target_os = "linux")]
#[path = "hid_linux.rs"]
mod platform;

#[cfg(target_os = "macos")]
#[path = "hid_macos.rs"]
mod platform;

/// The FIDO usage page every security key reports.
pub const FIDO_USAGE_PAGE: u32 = 0xF1D0;
/// Usage `CTAPHID` on that page.
pub const FIDO_USAGE_CTAPHID: u32 = 0x01;

/// Open every FIDO device present. Devices that fail to open (busy, no
/// permission) are skipped and logged.
pub fn open_all() -> Vec<Box<dyn HidDevice>> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        platform::open_all()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Vec::new()
    }
}

/// Usage page, usage and report sizes read from a HID report descriptor
/// (the subset of HID 1.11 section 6.2.2 the FIDO descriptors use).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DescriptorSummary {
    pub usage_page: u32,
    pub usage: u32,
    pub input_len: usize,
    pub output_len: usize,
}

impl DescriptorSummary {
    pub fn is_fido(&self) -> bool {
        self.usage_page == FIDO_USAGE_PAGE && self.usage == FIDO_USAGE_CTAPHID
    }

    /// The report length both directions use, when they agree and are
    /// between 8 and 64 bytes (libfido2's bounds).
    pub fn report_len(&self) -> Option<usize> {
        (self.input_len == self.output_len && (8..=64).contains(&self.input_len))
            .then_some(self.input_len)
    }
}

/// Walk short items: the first usage page and usage, and the
/// `Report Size × Report Count` of the input and output items.
pub fn parse_descriptor(descriptor: &[u8]) -> DescriptorSummary {
    let mut summary = DescriptorSummary::default();
    let mut report_size: usize = 0;
    let mut report_count: usize = 0;
    let mut have_page = false;
    let mut have_usage = false;
    let mut rest = descriptor;
    while let Some((&prefix, tail)) = rest.split_first() {
        if prefix == 0xFE {
            // Long item: its data length is the next byte.
            let len = usize::from(tail.first().copied().unwrap_or_default());
            rest = tail.get(len.saturating_add(2)..).unwrap_or_default();
            continue;
        }
        let len = match prefix & 0x03 {
            3 => 4,
            n => usize::from(n),
        };
        let Some(data) = tail.get(..len) else {
            break;
        };
        rest = tail.get(len..).unwrap_or_default();
        let value = data
            .iter()
            .rev()
            .fold(0u32, |acc, byte| (acc << 8) | u32::from(*byte));
        let value_len = usize::try_from(value).unwrap_or(usize::MAX);
        match prefix & 0xFC {
            0x04 if !have_page => {
                summary.usage_page = value;
                have_page = true;
            }
            0x08 if !have_usage => {
                summary.usage = value;
                have_usage = true;
            }
            0x74 => report_size = value_len,
            0x94 => report_count = value_len,
            0x80 => {
                summary.input_len = report_size.saturating_mul(report_count) / 8;
            }
            0x90 => {
                summary.output_len = report_size.saturating_mul(report_count) / 8;
            }
            _ => {}
        }
    }
    summary
}
